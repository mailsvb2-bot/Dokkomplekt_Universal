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

fn selected_filled_medical_markup_for_stories(
    stories: &BTreeMap<String, String>,
    excluded_fields: &BTreeSet<String>,
) -> BTreeMap<String, Vec<TemplateMarkupReplacement>> {
    let mut raw = Vec::<(String, TemplateMarkupReplacement)>::new();
    let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
    for (story, text) in stories {
        for replacement in selected_filled_medical_markup(text, excluded_fields) {
            owners
                .entry(replacement.value.clone())
                .or_default()
                .insert(replacement.field_id.clone());
            raw.push((story.clone(), replacement));
        }
    }

    let mut grouped = BTreeMap::<String, Vec<TemplateMarkupReplacement>>::new();
    for (story, replacement) in raw {
        if owners
            .get(&replacement.value)
            .is_some_and(|field_ids| field_ids.len() > 1)
        {
            continue;
        }
        grouped.entry(story).or_default().push(replacement);
    }
    grouped
}

fn selected_filled_medical_markup_by_story(
    template_path: &Path,
    excluded_fields: &BTreeSet<String>,
) -> Result<BTreeMap<String, Vec<TemplateMarkupReplacement>>, String> {
    let stories = extract_docx_story_texts(template_path).map_err(|error| error.to_string())?;
    Ok(selected_filled_medical_markup_for_stories(
        &stories,
        excluded_fields,
    ))
}

fn structural_template_bindings_for_stories(
    stories: &BTreeMap<String, String>,
    domain: &DomainKind,
    role_id: &str,
) -> (usize, BTreeSet<String>) {
    let mut binding_count = 0_usize;
    let mut field_ids = BTreeSet::new();
    for text in stories.values() {
        let bindings = dokkomplekt_core::infer_structural_template_values(
            text,
            Some(domain),
            Some(role_id),
        );
        binding_count += bindings.len();
        field_ids.extend(bindings.into_iter().map(|binding| binding.field_id));
    }
    (binding_count, field_ids)
}

fn structural_template_bindings_by_story(
    template_path: &Path,
    domain: &DomainKind,
    role_id: &str,
) -> Result<(usize, BTreeSet<String>), String> {
    let stories = extract_docx_story_texts(template_path).map_err(|error| error.to_string())?;
    Ok(structural_template_bindings_for_stories(
        &stories, domain, role_id,
    ))
}

fn blank_template_fields_for_stories(
    stories: &BTreeMap<String, String>,
    domain: &DomainKind,
    role_id: &str,
) -> BTreeMap<String, Vec<TemplateLearningMapField>> {
    let mut grouped = BTreeMap::new();
    for (story, text) in stories {
        let fields = dokkomplekt_core::infer_legacy_template_fields(
            text,
            Some(domain),
            Some(role_id),
        )
        .into_iter()
        .map(|candidate| TemplateLearningMapField {
            field_id: candidate.field_id,
            line_index: candidate.line_index,
            blank_line: candidate.blank_line,
            common_prefix: candidate.common_prefix,
            common_suffix: candidate.common_suffix,
        })
        .collect::<Vec<_>>();
        if !fields.is_empty() {
            grouped.insert(story.clone(), fields);
        }
    }
    grouped
}

fn blank_template_fields_by_story(
    template_path: &Path,
    domain: &DomainKind,
    role_id: &str,
) -> Result<BTreeMap<String, Vec<TemplateLearningMapField>>, String> {
    let stories = extract_docx_story_texts(template_path).map_err(|error| error.to_string())?;
    Ok(blank_template_fields_for_stories(&stories, domain, role_id))
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
    let blank_candidates_by_story = if infer_blank_zones {
        blank_template_fields_by_story(input_path, domain, role_id)?
    } else {
        BTreeMap::new()
    };
    let blank_binding_count = blank_candidates_by_story.values().map(Vec::len).sum::<usize>();
    let blank_field_ids = blank_candidates_by_story
        .values()
        .flatten()
        .map(|candidate| candidate.field_id.clone())
        .collect::<BTreeSet<_>>();
    let (structural_binding_count, structural_field_ids) = if domain == &DomainKind::Medical {
        structural_template_bindings_by_story(input_path, domain, role_id)?
    } else {
        (0, BTreeSet::new())
    };
    let mut initial_excluded_fields = analysis
        .placeholders
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    initial_excluded_fields.extend(blank_field_ids.iter().cloned());
    initial_excluded_fields.extend(structural_field_ids.iter().cloned());
    let initial_story_fallback = if domain == &DomainKind::Medical {
        selected_filled_medical_markup_by_story(input_path, &initial_excluded_fields)?
    } else {
        BTreeMap::new()
    };
    let primary_expert_field =
        dokkomplekt_core::domains::medical_semantics::MEDICAL_EXPERT_ANAMNESIS;
    let needs_primary_expert_insertion = domain == &DomainKind::Medical
        && dokkomplekt_core::domains::medical::canonical_medical_role(role_id) == "primary"
        && !analysis
            .placeholders
            .iter()
            .any(|field| field == primary_expert_field);

    if blank_binding_count == 0
        && structural_binding_count == 0
        && initial_story_fallback.is_empty()
        && !needs_primary_expert_insertion
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
    let mut changed = false;

    if blank_binding_count > 0 {
        let blank_output = scratch_root.join(format!("blank-{}.{}", Uuid::new_v4(), extension));
        let report = apply_story_template_learning_map_file(
            &current_input,
            &blank_output,
            &blank_candidates_by_story,
        )
        .map_err(|error| format!("Не удалось разметить однозначные пустые зоны: {error}"))?;
        if !report.skipped_bindings.is_empty() || report.applied_binding_count != blank_binding_count {
            return Err(format!(
                "Не все story-scoped пустые зоны удалось скомпилировать: {}",
                report.skipped_bindings.join(", ")
            ));
        }
        applied_field_ids.extend(report.applied_field_ids);
        current_input = blank_output;
        changed = true;
    }

    let (current_structural_binding_count, _) = if domain == &DomainKind::Medical {
        structural_template_bindings_by_story(&current_input, domain, role_id)?
    } else {
        (0, BTreeSet::new())
    };
    if current_structural_binding_count > 0 {
        let structural_output =
            scratch_root.join(format!("structural-{}.{}", Uuid::new_v4(), extension));
        let report = compile_labeled_template_file(
            &current_input,
            &structural_output,
            domain,
            role_id,
        )
        .map_err(|error| format!("Не удалось скомпилировать структурные якоря: {error}"))?;
        if report.binding_count != current_structural_binding_count {
            return Err(format!(
                "Структурный compiler ожидал {} story-scoped якорей после предыдущего stage, но подтвердил {}.",
                current_structural_binding_count,
                report.binding_count
            ));
        }
        applied_field_ids.extend(report.applied_field_ids);
        current_input = structural_output;
        changed = true;
    }

    // Compatibility fallback is derived from the exact post-structural DOCX,
    // never from a stale flattened snapshot. Each candidate stays inside the
    // Word story that produced it, so body/header/footer text cannot be merged
    // into one impossible replacement target.
    let current_text = extract_docx_text(&current_input)
        .map_err(|error| format!("Не удалось перечитать compiler-stage шаблона: {error}"))?;
    let current_analysis = analyze_template_text_with_domain_hint(&current_text, Some(domain));
    let mut fallback_excluded_fields = current_analysis
        .placeholders
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    fallback_excluded_fields.extend(applied_field_ids.iter().cloned());
    let fallback_by_story = if domain == &DomainKind::Medical {
        selected_filled_medical_markup_by_story(&current_input, &fallback_excluded_fields)?
    } else {
        BTreeMap::new()
    };

    if !fallback_by_story.is_empty() {
        let expected_bindings = fallback_by_story.values().map(Vec::len).sum::<usize>();
        let report = apply_story_template_markup_file(
            &current_input,
            output_path,
            &fallback_by_story,
        )
        .map_err(|error| format!("Не удалось применить story-scoped fallback старого шаблона: {error}"))?;
        if !report.skipped_bindings.is_empty()
            || report.applied_binding_count != expected_bindings
        {
            return Err(format!(
                "Story-scoped fallback не смог безопасно привязать значения: {}",
                report.skipped_bindings.join(", ")
            ));
        }
        applied_field_ids.extend(report.applied_field_ids);
        current_input = output_path.to_path_buf();
        changed = true;
    } else if changed {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::copy(&current_input, output_path)
            .map_err(|error| format!("Не удалось зафиксировать compiler-копию шаблона: {error}"))?;
        current_input = output_path.to_path_buf();
    }

    // Primary inspection has a role-owned expert anamnesis at the end of the
    // document. Historical Dokkomplekt inserted it before the physician/signature
    // block even when the doctor's Word template did not contain that section.
    // Preserve that behaviour in the compiler copy rather than requiring users
    // to add a technical placeholder to their own DOCX.
    if domain == &DomainKind::Medical
        && dokkomplekt_core::domains::medical::canonical_medical_role(role_id) == "primary"
    {
        let expert_field =
            dokkomplekt_core::domains::medical_semantics::MEDICAL_EXPERT_ANAMNESIS;
        let role_text = extract_docx_text(&current_input)
            .map_err(|error| format!("Не удалось проверить primary role-block: {error}"))?;
        let role_analysis = analyze_template_text_with_domain_hint(&role_text, Some(domain));
        if !role_analysis.placeholders.iter().any(|field| field == expert_field) {
            let role_output = scratch_root.join(format!("primary-role-{}.{}", Uuid::new_v4(), extension));
            let inserted = insert_text_paragraph_before_first_matching_file(
                &current_input,
                &role_output,
                &[
                    "Лечащий врач",
                    "Врач-психиатр",
                    "Врач психиатр",
                    "Заведующий отделением",
                    "Зав. отделением",
                    "Зав. отд.",
                ],
                "Экспертный анамнез: {{medical.expert_anamnesis}}",
            )
            .map_err(|error| format!("Primary expert-anamnesis insertion failed: {error}"))?;
            if inserted {
                current_input = role_output;
                changed = true;
                applied_field_ids.push(expert_field.to_string());
            }
        }
    }

    if !changed {
        return Ok(TemplateContractCompilation {
            changed: false,
            path: input_path.to_path_buf(),
            template_text,
            applied_field_ids: Vec::new(),
        });
    }

    applied_field_ids.sort();
    applied_field_ids.dedup();
    let derived_text = extract_docx_text(&current_input)
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
        path: current_input,
        template_text: derived_text,
        applied_field_ids,
    })
}

fn validate_medical_template_output_contract(
    document: &DocumentTemplateSpec,
) -> Result<(), String> {
    // Creating a button from an untouched user-owned Word template is always
    // allowed. Static copies are not yet claiming to implement a semantic render
    // contract; generation will attempt deterministic compilation later and stays
    // fail-closed if the template cannot be made safe.
    if document.is_static_copy {
        return Ok(());
    }
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

fn should_attempt_template_contract_compilation(
    domain: &DomainKind,
    legacy_static: bool,
    infer_blank_zones: bool,
) -> bool {
    if legacy_static {
        return infer_blank_zones;
    }
    matches!(domain, DomainKind::Medical)
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
        if !should_attempt_template_contract_compilation(
            &domain,
            legacy_static,
            infer_blank_zones,
        ) {
            if legacy_static {
                summary.untouched_static_documents += 1;
            }
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
        let compiled = match compile_template_contract_copy(
            &input_path,
            &output_path,
            root,
            &domain,
            &row.analysis.role_id,
            infer_blank_zones && legacy_static,
        ) {
            Ok(compiled) => compiled,
            Err(error) if legacy_static => {
                // Auto-inference is optional. A failed guess must never block the
                // primary first-run action: keep the exact user template as a
                // static button and defer semantic compilation to generation.
                eprintln!(
                    "Необязательная авторазметка шаблона «{}» пропущена: {error}",
                    row.editable_button_label
                );
                summary.untouched_static_documents += 1;
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "Шаблон «{}» не удалось скомпилировать в рабочий semantic-template: {error}",
                    row.editable_button_label
                ));
            }
        };
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
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_story_test_docx(path: &Path, body: &str, header: Option<&str>) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create test directory");
        }
        let file = std::fs::File::create(path).expect("create test DOCX");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document part");
        writer.write_all(body.as_bytes()).expect("document bytes");
        if let Some(header) = header {
            writer
                .start_file("word/header1.xml", options)
                .expect("header part");
            writer.write_all(header.as_bytes()).expect("header bytes");
        }
        writer.finish().expect("finish test DOCX");
    }

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
    fn untouched_static_templates_do_not_require_optional_compilation() {
        assert!(!should_attempt_template_contract_compilation(
            &DomainKind::Medical,
            true,
            false,
        ));
        assert!(should_attempt_template_contract_compilation(
            &DomainKind::Medical,
            true,
            true,
        ));
        assert!(!should_attempt_template_contract_compilation(
            &DomainKind::Generic,
            true,
            false,
        ));
    }

    #[test]
    fn static_medical_button_can_be_registered_before_semantic_markup() {
        let mut document = medical_document();
        document.is_static_copy = true;
        document.placeholders.clear();
        document.required_fields.clear();
        assert!(validate_medical_template_output_contract(&document).is_ok());
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
    fn blank_inference_never_uses_a_label_from_body_with_a_blank_from_header() {
        let body = "Первичный осмотр\nЖалобы:".to_string();
        let header = "________".to_string();
        let flattened = format!("{body}\n{header}");
        let legacy_flattened = dokkomplekt_core::infer_legacy_template_fields(
            &flattened,
            Some(&DomainKind::Medical),
            Some("primary"),
        );
        assert!(legacy_flattened
            .iter()
            .any(|candidate| candidate.field_id == "medical.complaints"));

        let stories = BTreeMap::from([
            ("word/document.xml".to_string(), body),
            ("word/header1.xml".to_string(), header),
        ]);
        let scoped = blank_template_fields_for_stories(
            &stories,
            &DomainKind::Medical,
            "primary",
        );
        assert!(scoped.values().flatten().all(|candidate| {
            candidate.field_id != "medical.complaints"
        }));
    }

    #[test]
    fn structural_expected_count_never_crosses_word_story_boundaries() {
        let stories = BTreeMap::from([
            (
                "word/document.xml".to_string(),
                "Первичный осмотр\nЖалобы: тревога".to_string(),
            ),
            (
                "word/header1.xml".to_string(),
                "служебный колонтитул".to_string(),
            ),
        ]);
        let (count, fields) = structural_template_bindings_for_stories(
            &stories,
            &DomainKind::Medical,
            "primary",
        );
        assert_eq!(count, 1);
        assert_eq!(fields, BTreeSet::from(["medical.complaints".to_string()]));
    }

    #[test]
    fn parser_fallback_never_crosses_from_body_into_header_story() {
        let body = "Первичный осмотр\nЖалобы: тревога и бессонница".to_string();
        let header = "ГБУЗ НО НКЦПЗ".to_string();
        let flattened = format!("{body}\n{header}");
        let legacy_flattened = selected_filled_medical_markup(&flattened, &BTreeSet::new());
        let old_complaints = legacy_flattened
            .iter()
            .find(|replacement| replacement.field_id == "medical.complaints")
            .expect("flattened parser demonstrates the legacy cross-story bug");
        assert!(old_complaints.value.contains("ГБУЗ НО НКЦПЗ"));

        let stories = BTreeMap::from([
            ("word/document.xml".to_string(), body),
            ("word/header1.xml".to_string(), header),
        ]);
        let scoped =
            selected_filled_medical_markup_for_stories(&stories, &BTreeSet::new());
        let complaints = scoped["word/document.xml"]
            .iter()
            .find(|replacement| replacement.field_id == "medical.complaints")
            .expect("body complaints candidate");
        assert_eq!(complaints.value, "тревога и бессонница");
        assert!(scoped
            .values()
            .flatten()
            .all(|replacement| !replacement.value.contains("ГБУЗ НО НКЦПЗ")));
    }

    #[test]
    fn full_contract_compiler_keeps_header_out_of_body_fallback_and_succeeds() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-runtime-story-compiler-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let input = root.join("filled.docx");
        let output = root.join("compiled.docx");
        let scratch = root.join("scratch");
        write_story_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Первичный осмотр</w:t></w:r></w:p><w:p><w:r><w:t>Иванов Иван Иванович проживает: Нижний Новгород, Ленина 1</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#,
            Some(r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ГБУЗ НО НКЦПЗ</w:t></w:r></w:p></w:hdr>"#),
        );

        let compiled = compile_template_contract_copy(
            &input,
            &output,
            &scratch,
            &DomainKind::Medical,
            "primary",
            true,
        )
        .expect("story-scoped compiler must succeed");
        assert!(compiled.changed);
        assert!(compiled
            .applied_field_ids
            .iter()
            .any(|field_id| field_id == "subject.address"));
        let stories = extract_docx_story_texts(&compiled.path).expect("compiled stories");
        assert!(stories["word/document.xml"].contains("{{subject.address}}"));
        assert!(stories["word/header1.xml"].contains("ГБУЗ НО НКЦПЗ"));
        assert!(!stories["word/header1.xml"].contains("{{"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn already_semantic_primary_still_receives_role_owned_expert_anamnesis() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-semantic-primary-role-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let input = root.join("semantic-primary.docx");
        let output = root.join("compiled.docx");
        let scratch = root.join("scratch");
        write_story_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Первичный осмотр</w:t></w:r></w:p>
<w:p><w:r><w:t>{{subject.name}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.case_number}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.admission_date}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.diagnosis}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.treatment}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.workplace}}</w:t></w:r></w:p>
<w:p><w:r><w:t>{{medical.position}}</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечащий врач __________</w:t></w:r></w:p>
<w:p><w:r><w:t>Заведующий отделением __________</w:t></w:r></w:p>
<w:sectPr/></w:body></w:document>"#,
            None,
        );

        let compiled = compile_template_contract_copy(
            &input,
            &output,
            &scratch,
            &DomainKind::Medical,
            "primary",
            true,
        )
        .expect("already-semantic primary must not exit before role-owned insertion");
        assert!(compiled.changed);
        assert!(compiled
            .applied_field_ids
            .iter()
            .any(|field| field == "medical.expert_anamnesis"));
        let stories = extract_docx_story_texts(&compiled.path).expect("compiled stories");
        let body = &stories["word/document.xml"];
        assert!(body.contains("Экспертный анамнез: {{medical.expert_anamnesis}}"));
        assert!(
            body.find("Экспертный анамнез: {{medical.expert_anamnesis}}")
                < body.find("Лечащий врач __________"),
            "role-owned expert anamnesis must precede signatures: {body}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn windows_primary_fixture_compiles_without_semanticizing_signature_blanks() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-windows-primary-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let input = root.join("исходник проверка № 1.docx");
        let output = root.join("compiled.docx");
        let scratch = root.join("scratch");
        write_story_test_docx(
            &input,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Первичный осмотр</w:t></w:r></w:p>
<w:p><w:r><w:t>Ф.И.О.: Иванов Иван Иванович</w:t></w:r></w:p>
<w:p><w:r><w:t>Номер истории болезни: 1111</w:t></w:r></w:p>
<w:p><w:r><w:t>Дата поступления: 20.08.2026</w:t></w:r></w:p>
<w:p><w:r><w:t>Диагноз: F20.0 шаблонная формулировка</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечение: старое лечение</w:t></w:r></w:p>
<w:p><w:r><w:t>Место работы: Старый завод</w:t></w:r></w:p>
<w:p><w:r><w:t>Должность: старый инженер</w:t></w:r></w:p>
<w:p><w:r><w:t>Лечащий врач __________</w:t></w:r></w:p>
<w:p><w:r><w:t>Заведующий отделением __________</w:t></w:r></w:p>
<w:sectPr/></w:body></w:document>"#,
            Some(r#"<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>ГБУЗ НО «НКЦПЗ» диспансер №2</w:t></w:r></w:p></w:hdr>"#),
        );

        let compiled = compile_template_contract_copy(
            &input,
            &output,
            &scratch,
            &DomainKind::Medical,
            "primary",
            true,
        )
        .expect("the installed Windows primary fixture must compile");
        assert!(!compiled
            .applied_field_ids
            .iter()
            .any(|field| field == "medical.attending_doctor" || field == "medical.department_head"));
        let stories = extract_docx_story_texts(&compiled.path).expect("compiled stories");
        let body = &stories["word/document.xml"];
        assert!(body.contains("Лечащий врач __________"));
        assert!(body.contains("Заведующий отделением __________"));
        assert!(body.contains("Экспертный анамнез: {{medical.expert_anamnesis}}"));
        assert!(
            body.find("Экспертный анамнез: {{medical.expert_anamnesis}}")
                < body.find("Лечащий врач __________"),
            "primary expert anamnesis must be inserted immediately before signatures: {body}"
        );
        assert!(compiled
            .applied_field_ids
            .iter()
            .any(|field| field == "medical.expert_anamnesis"));
        let analysis = analyze_template_text_with_domain_hint(body, Some(&DomainKind::Medical));
        let mut document = DocumentTemplateSpec {
            id: "primary-runtime".into(),
            button_label: "первичный".into(),
            template_path: compiled.path.display().to_string(),
            category: DomainKind::Medical,
            role_id: "primary".into(),
            required_fields: Vec::new(),
            placeholders: analysis.placeholders,
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        validate_medical_template_output_contract(&document)
            .expect("compiled primary role contract must be complete");
        apply_compiled_contract_to_document(&mut document, body)
            .expect("compiled primary contract must persist safely");
        assert!(stories["word/header1.xml"].contains("НКЦПЗ"));
        assert!(!stories["word/header1.xml"].contains("Экспертный анамнез"));
        let _ = std::fs::remove_dir_all(root);
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
