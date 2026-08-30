#[derive(Debug)]
struct LegacyTemplateInferenceWorkspace {
    root: PathBuf,
}

impl Drop for LegacyTemplateInferenceWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug, Default, Serialize)]
struct LegacyTemplateInferenceSummary {
    upgraded_documents: usize,
    inferred_fields: usize,
    untouched_static_documents: usize,
}

#[derive(Debug)]
struct TemplateContractCompilation {
    changed: bool,
    path: PathBuf,
    template_text: String,
    applied_field_ids: Vec<String>,
}

fn selected_filled_medical_markup(
    template_text: &str,
    excluded_fields: &BTreeSet<String>,
) -> Vec<TemplateMarkupReplacement> {
    // Value-based detection is a compatibility fallback only. Anything that has
    // a structural owner is excluded so a coincidentally repeated old value can
    // never override donor-style label/block binding.
    dokkomplekt_core::suggest_filled_medical_template_markup(template_text, current_year_utc())
        .into_iter()
        .filter(|candidate| {
            candidate.selected_by_default && !excluded_fields.contains(&candidate.field_id)
        })
        .map(|candidate| TemplateMarkupReplacement {
            field_id: candidate.field_id,
            value: candidate.value,
            action: Default::default(),
        })
        .collect()
}

fn compile_template_contract_copy(
    input_path: &Path,
    output_path: &Path,
    scratch_root: &Path,
    domain: &DomainKind,
    role_id: &str,
    infer_blank_zones: bool,
) -> Result<TemplateContractCompilation, String> {
    let template_text = extract_docx_text(input_path).map_err(|error| error.to_string())?;
    let analysis = analyze_template_text_with_domain_hint(&template_text, Some(domain));
    let blank_candidates = if infer_blank_zones {
        dokkomplekt_core::infer_legacy_template_fields(
            &template_text,
            Some(domain),
            Some(role_id),
        )
    } else {
        Vec::new()
    };
    let structural_bindings = if domain == &DomainKind::Medical {
        dokkomplekt_core::infer_structural_template_values(
            &template_text,
            Some(domain),
            Some(role_id),
        )
    } else {
        Vec::new()
    };
    let mut excluded_fields = analysis
        .placeholders
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    excluded_fields.extend(
        blank_candidates
            .iter()
            .map(|candidate| candidate.field_id.clone()),
    );
    excluded_fields.extend(
        structural_bindings
            .iter()
            .map(|binding| binding.field_id.clone()),
    );
    let fallback_replacements = if domain == &DomainKind::Medical {
        selected_filled_medical_markup(&template_text, &excluded_fields)
    } else {
        Vec::new()
    };

    if blank_candidates.is_empty()
        && structural_bindings.is_empty()
        && fallback_replacements.is_empty()
    {
        return Ok(TemplateContractCompilation {
            changed: false,
            path: input_path.to_path_buf(),
            template_text,
            applied_field_ids: Vec::new(),
        });
    }

    std::fs::create_dir_all(scratch_root)
        .map_err(|error| format!("Не удалось создать рабочую папку compiler шаблона: {error}"))?;
    let extension = input_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "docx" | "docm") {
        return Err(format!(
            "Compiler шаблонов поддерживает только DOCX/DOCM: {}",
            input_path.display()
        ));
    }

    let mut current_input = input_path.to_path_buf();
    let mut applied_field_ids = Vec::new();
    if !blank_candidates.is_empty() {
        let more_stages = !structural_bindings.is_empty() || !fallback_replacements.is_empty();
        let blank_output = if more_stages {
            scratch_root.join(format!("blank-{}.{}", Uuid::new_v4(), extension))
        } else {
            output_path.to_path_buf()
        };
        let fields = blank_candidates
            .iter()
            .map(|candidate| TemplateLearningMapField {
                field_id: candidate.field_id.clone(),
                line_index: candidate.line_index,
                blank_line: candidate.blank_line.clone(),
                common_prefix: candidate.common_prefix.clone(),
                common_suffix: candidate.common_suffix.clone(),
            })
            .collect::<Vec<_>>();
        let report = apply_template_learning_map_file(&current_input, &blank_output, &fields)
            .map_err(|error| format!("Не удалось разметить однозначные пустые зоны: {error}"))?;
        if !report.skipped_field_ids.is_empty() || report.applied_field_ids.len() != fields.len() {
            return Err(format!(
                "Не все однозначные пустые зоны удалось скомпилировать: {}",
                report.skipped_field_ids.join(", ")
            ));
        }
        applied_field_ids.extend(report.applied_field_ids);
        current_input = blank_output;
    }

    if !structural_bindings.is_empty() {
        let structural_output = if fallback_replacements.is_empty() {
            output_path.to_path_buf()
        } else {
            scratch_root.join(format!("structural-{}.{}", Uuid::new_v4(), extension))
        };
        let report = compile_labeled_template_file(
            &current_input,
            &structural_output,
            domain,
            role_id,
        )
        .map_err(|error| format!("Не удалось скомпилировать структурные якоря: {error}"))?;
        if report.binding_count != structural_bindings.len() {
            return Err(format!(
                "Структурный compiler ожидал {} якорей, но подтвердил {}.",
                structural_bindings.len(),
                report.binding_count
            ));
        }
        applied_field_ids.extend(report.applied_field_ids);
        current_input = structural_output;
    }

    if !fallback_replacements.is_empty() {
        let report = apply_template_markup_file(
            &current_input,
            output_path,
            &fallback_replacements,
        )
        .map_err(|error| format!("Не удалось применить fallback старого шаблона: {error}"))?;
        if !report.skipped_values.is_empty()
            || report.replacement_count != fallback_replacements.len()
        {
            return Err(
                "Fallback старого шаблона не смог однозначно переписать все оставшиеся значения."
                    .into(),
            );
        }
        applied_field_ids.extend(
            fallback_replacements
                .iter()
                .map(|replacement| replacement.field_id.clone()),
        );
    }

    applied_field_ids.sort();
    applied_field_ids.dedup();
    let derived_text = extract_docx_text(output_path)
        .map_err(|error| format!("Не удалось проверить скомпилированную копию: {error}"))?;
    let derived_analysis = analyze_template_text_with_domain_hint(&derived_text, Some(domain));
    if derived_analysis.is_static {
        return Err("Скомпилированная копия не содержит placeholder-полей.".into());
    }
    for field_id in &applied_field_ids {
        if !derived_analysis
            .placeholders
            .iter()
            .any(|candidate| candidate == field_id)
        {
            return Err(format!(
                "Compiler не подтвердил созданное semantic-поле {field_id}."
            ));
        }
    }
    Ok(TemplateContractCompilation {
        changed: true,
        path: output_path.to_path_buf(),
        template_text: derived_text,
        applied_field_ids,
    })
}

fn validate_medical_template_output_contract(
    document: &DocumentTemplateSpec,
) -> Result<(), String> {
    let missing = missing_medical_template_render_paths(document);
    if missing.is_empty() {
        return Ok(());
    }
    let titles = missing
        .iter()
        .map(|field_id| dokkomplekt_core::title_for_field(field_id))
        .collect::<Vec<_>>();
    Err(format!(
        "Шаблон «{}» не может быть опубликован как рабочий документ: в его Word-структуре отсутствуют точки подстановки для обязательных данных: {}. Исправьте шаблон или повторно импортируйте исходный рабочий DOCX; неполный шаблон не будет сохранён.",
        document.button_label,
        titles.join(", ")
    ))
}

fn apply_compiled_contract_to_document(
    document: &mut DocumentTemplateSpec,
    compiled_text: &str,
) -> Result<(), String> {
    let analysis = analyze_template_text_with_domain_hint(compiled_text, Some(&document.category));
    if analysis.is_static || analysis.placeholders.is_empty() {
        return Err(format!(
            "Скомпилированный шаблон «{}» не содержит semantic-placeholders.",
            document.button_label
        ));
    }
    document.placeholders = analysis.placeholders.clone();
    document
        .required_fields
        .extend(analysis.placeholders.iter().cloned());
    document.required_fields.sort();
    document.required_fields.dedup();
    document.is_static_copy = false;
    validate_medical_template_output_contract(document)
}

fn migrate_loaded_medical_template_contracts(
    app: &tauri::AppHandle,
    repo: &mut LocalRepository,
    pack: &mut DocumentPack,
    case: &SemanticCase,
    license: &Option<LicenseDocument>,
) -> Result<usize, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-contract-migration")
        .join(Uuid::new_v4().to_string());
    let workspace = LegacyTemplateInferenceWorkspace { root: root.clone() };
    let mut drafts = Vec::new();
    let mut migrated = 0usize;

    for document in &mut pack.documents {
        if document.category != DomainKind::Medical {
            continue;
        }
        let input_path = resolve_user_path(app, &document.template_path)?;
        let extension = input_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("docx");
        let output_path = root.join(format!(
            "{}-migration-{}.{}",
            sanitize_path_component(&document.id),
            Uuid::new_v4(),
            extension
        ));
        let compiled = match compile_template_contract_copy(
            &input_path,
            &output_path,
            &root,
            &DomainKind::Medical,
            &document.role_id,
            true,
        ) {
            Ok(compiled) => compiled,
            // A legacy template that cannot be transformed deterministically is
            // left untouched. Runtime generation remains fail-closed and the
            // doctor can repair/re-import it explicitly; startup itself remains usable.
            Err(_) => continue,
        };
        if !compiled.changed {
            continue;
        }
        let mut candidate_document = document.clone();
        if apply_compiled_contract_to_document(&mut candidate_document, &compiled.template_text)
            .is_err()
        {
            // A deterministic partial repair is still not publishable. Keep the
            // previous version intact and let the runtime safety-net explain the
            // missing render paths if the doctor tries to generate it.
            continue;
        }
        let template_sha256 = file_content_signature(&compiled.path)?.2;
        let draft = prepare_template_version_draft(
            app,
            &candidate_document.id,
            &compiled.path,
            &template_sha256,
            "Автоматическая миграция старого doctor-owned шаблона в структурный semantic-contract.",
        )?;
        candidate_document.template_path = draft.template_path.clone();
        *document = candidate_document;
        drafts.push(draft);
        migrated += 1;
    }

    if !drafts.is_empty() {
        repo.save_desktop_snapshot_with_template_versions(DesktopSnapshotPublication {
            case_id: "current",
            pack_id: "default",
            case,
            pack,
            state_key: "license_document",
            state_value: license,
            versions: &drafts,
        })
        .map_err(|error| error.to_string())?;
    }
    drop(workspace);
    Ok(migrated)
}


struct PreparedMedicalRenderTemplate {
    path: PathBuf,
    template_text: String,
    _workspace: Option<LegacyTemplateInferenceWorkspace>,
}

fn prepare_medical_template_for_render(
    app: &tauri::AppHandle,
    document: &DocumentTemplateSpec,
    template_path: &Path,
) -> Result<PreparedMedicalRenderTemplate, String> {
    let template_text = extract_docx_text(template_path).map_err(|error| error.to_string())?;
    if document.category != DomainKind::Medical {
        return Ok(PreparedMedicalRenderTemplate {
            path: template_path.to_path_buf(),
            template_text,
            _workspace: None,
        });
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-render-inference")
        .join(Uuid::new_v4().to_string());
    let extension = template_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx");
    let output_path = root.join(format!("render-template.{extension}"));
    let compiled = compile_template_contract_copy(
        template_path,
        &output_path,
        &root,
        &DomainKind::Medical,
        &document.role_id,
        true,
    )?;
    let mut effective_document = document.clone();
    let effective_text = if compiled.changed {
        compiled.template_text.clone()
    } else {
        template_text.clone()
    };
    apply_compiled_contract_to_document(&mut effective_document, &effective_text)?;
    if !compiled.changed {
        return Ok(PreparedMedicalRenderTemplate {
            path: template_path.to_path_buf(),
            template_text,
            _workspace: None,
        });
    }
    Ok(PreparedMedicalRenderTemplate {
        path: compiled.path,
        template_text: compiled.template_text,
        _workspace: Some(LegacyTemplateInferenceWorkspace { root }),
    })
}

fn infer_static_template_rows(
    app: &tauri::AppHandle,
    rows: &[TemplateConfirmationRow],
    infer_blank_zones: bool,
) -> Result<(
    Vec<TemplateConfirmationRow>,
    Option<LegacyTemplateInferenceWorkspace>,
    LegacyTemplateInferenceSummary,
), String> {
    let mut updated_rows = rows.to_vec();
    let mut workspace: Option<LegacyTemplateInferenceWorkspace> = None;
    let mut summary = LegacyTemplateInferenceSummary::default();

    for row in &mut updated_rows {
        let domain = row
            .domain_override
            .clone()
            .unwrap_or_else(|| dokkomplekt_core::best_domain(&row.analysis));
        let legacy_static = row.is_static_copy || row.analysis.is_static;
        let structural_medical = matches!(domain, DomainKind::Medical);
        if !structural_medical && !(infer_blank_zones && legacy_static) {
            continue;
        }
        if workspace.is_none() {
            let root = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("template-inference-work")
                .join(Uuid::new_v4().to_string());
            workspace = Some(LegacyTemplateInferenceWorkspace { root });
        }
        let root = &workspace
            .as_ref()
            .ok_or_else(|| "template compiler workspace was not initialized".to_string())?
            .root;
        let input_path = resolve_user_path(app, &row.template_path)?;
        let extension = input_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("docx");
        let safe_document_id = sanitize_path_component(&row.document_id);
        let output_path = root.join(format!(
            "{}-compiled-{}.{}",
            if safe_document_id.is_empty() {
                "template"
            } else {
                safe_document_id.as_str()
            },
            Uuid::new_v4(),
            extension
        ));
        let compiled = compile_template_contract_copy(
            &input_path,
            &output_path,
            root,
            &domain,
            &row.analysis.role_id,
            infer_blank_zones && legacy_static,
        )
        .map_err(|error| {
            format!(
                "Шаблон «{}» не удалось скомпилировать в рабочий semantic-template: {error}",
                row.editable_button_label
            )
        })?;
        if !compiled.changed {
            if legacy_static {
                summary.untouched_static_documents += 1;
            }
            continue;
        }

        let derived_analysis = analyze_template_text_with_domain_hint(
            &compiled.template_text,
            row.domain_override.as_ref(),
        );
        row.template_path = compiled.path.display().to_string();
        row.detected_title = derived_analysis.title.clone();
        row.suggested_button_label = derived_analysis.suggested_button_label.clone();
        row.role_id = derived_analysis.role_id.clone();
        row.is_static_copy = false;
        row.analysis = derived_analysis;
        summary.upgraded_documents += 1;
        summary.inferred_fields += compiled.applied_field_ids.len();
    }

    Ok((updated_rows, workspace, summary))
}

#[cfg(test)]
mod legacy_template_runtime_tests {
    use super::*;

    fn medical_document() -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "discharge".into(),
            button_label: "Выписной эпикриз".into(),
            template_path: "discharge.docx".into(),
            category: DomainKind::Medical,
            role_id: "discharge".into(),
            required_fields: vec!["medical.discharge_date".into()],
            placeholders: vec!["medical.discharge_condition".into()],
            is_static_copy: false,
            popup_fields: vec![PopupFieldConfig::new(
                "medical.discharge_date",
                "Своя дата выписки",
            )],
            popup_configured: true,
        }
    }

    #[test]
    fn structural_medical_compilation_is_independent_from_blank_zone_opt_in() {
        let legacy_static = false;
        let infer_blank_zones = false;
        let domain = DomainKind::Medical;
        let structural_medical = matches!(domain, DomainKind::Medical);
        assert!(structural_medical);
        assert!(!(infer_blank_zones && legacy_static));
    }

    #[test]
    fn parser_fallback_never_competes_with_structural_bindings() {
        let text = concat!(
            "Выписной эпикриз\n",
            "{{medical.expert_anamnesis}}\n",
            "Ф.И.О.: Иванов Иван Иванович\n",
            "Номер истории болезни: 1234\n",
            "Дата поступления: 01.09.2026\n",
            "Диагноз: F20.0 состояние стабильное\n",
            "Дата выписки: 09.09.2026\n",
            "Лечение: терапия\n",
            "Место работы: Завод\n",
            "Должность: инженер"
        );
        let structural = dokkomplekt_core::infer_structural_template_values(
            text,
            Some(&DomainKind::Medical),
            Some("discharge"),
        );
        let mut excluded = BTreeSet::from(["medical.expert_anamnesis".to_string()]);
        excluded.extend(structural.iter().map(|item| item.field_id.clone()));
        let fallback = selected_filled_medical_markup(text, &excluded);
        assert!(fallback
            .iter()
            .all(|item| !excluded.contains(&item.field_id)));
        for field_id in structural.iter().map(|item| item.field_id.as_str()) {
            assert!(
                !fallback.iter().any(|item| item.field_id == field_id),
                "structural field leaked into parser fallback: {field_id}"
            );
        }
    }

    #[test]
    fn compiled_contract_becomes_the_persisted_document_contract_without_losing_popup_customization() {
        let mut document = medical_document();
        let popup_before = document.popup_fields.clone();
        apply_compiled_contract_to_document(
            &mut document,
            concat!(
                "Выписной эпикриз\n",
                "{{subject.name}}\n",
                "{{medical.case_number}}\n",
                "{{medical.admission_date}}\n",
                "{{medical.diagnosis}}\n",
                "{{medical.discharge_date}}\n",
                "{{medical.treatment}}\n",
                "{{medical.expert_anamnesis}}\n",
                "{{medical.discharge_condition}}"
            ),
        )
        .expect("compiled contract");
        for field_id in [
            "subject.name",
            "medical.case_number",
            "medical.admission_date",
            "medical.diagnosis",
            "medical.discharge_date",
            "medical.treatment",
            "medical.discharge_condition",
        ] {
            assert!(document.placeholders.iter().any(|item| item == field_id));
            assert!(document.required_fields.iter().any(|item| item == field_id));
        }
        assert_eq!(document.popup_fields, popup_before);
        assert!(document.popup_configured);
        assert!(!document.is_static_copy);
    }
}
