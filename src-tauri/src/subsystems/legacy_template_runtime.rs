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

fn infer_static_template_rows(
    app: &tauri::AppHandle,
    rows: &[TemplateConfirmationRow],
) -> Result<(
    Vec<TemplateConfirmationRow>,
    Option<LegacyTemplateInferenceWorkspace>,
    LegacyTemplateInferenceSummary,
), String> {
    let mut updated_rows = rows.to_vec();
    let mut workspace: Option<LegacyTemplateInferenceWorkspace> = None;
    let mut summary = LegacyTemplateInferenceSummary::default();

    for row in &mut updated_rows {
        if !row.is_static_copy && !row.analysis.is_static {
            continue;
        }
        let input_path = resolve_user_path(app, &row.template_path)?;
        let template_text = extract_docx_text(&input_path).map_err(|error| {
            format!(
                "Не удалось прочитать старый шаблон «{}» перед безопасной разметкой: {error}",
                row.editable_button_label
            )
        })?;
        let domain = row
            .domain_override
            .clone()
            .unwrap_or_else(|| dokkomplekt_core::best_domain(&row.analysis));
        let candidates = dokkomplekt_core::infer_legacy_template_fields(
            &template_text,
            Some(&domain),
            Some(&row.analysis.role_id),
        );
        if candidates.is_empty() {
            summary.untouched_static_documents += 1;
            continue;
        }

        if workspace.is_none() {
            let root = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("template-inference-work")
                .join(Uuid::new_v4().to_string());
            std::fs::create_dir_all(&root).map_err(|error| {
                format!("Не удалось создать рабочую папку разметки шаблона: {error}")
            })?;
            workspace = Some(LegacyTemplateInferenceWorkspace { root });
        }
        let root = &workspace
            .as_ref()
            .ok_or_else(|| "legacy inference workspace was not initialized".to_string())?
            .root;
        let extension = input_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("docx")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "docx" | "docm") {
            return Err(format!(
                "Авторазметка старого шаблона поддерживает только DOCX/DOCM: {}",
                input_path.display()
            ));
        }
        let safe_document_id = sanitize_path_component(&row.document_id);
        let output_path = root.join(format!(
            "{}-inferred-{}.{}",
            if safe_document_id.is_empty() {
                "template"
            } else {
                safe_document_id.as_str()
            },
            Uuid::new_v4(),
            extension
        ));
        let fields = candidates
            .iter()
            .map(|candidate| TemplateLearningMapField {
                field_id: candidate.field_id.clone(),
                line_index: candidate.line_index,
                blank_line: candidate.blank_line.clone(),
                common_prefix: candidate.common_prefix.clone(),
                common_suffix: candidate.common_suffix.clone(),
            })
            .collect::<Vec<_>>();
        let report = apply_template_learning_map_file(&input_path, &output_path, &fields)
            .map_err(|error| {
                format!(
                    "Не удалось безопасно разметить старый шаблон «{}»: {error}",
                    row.editable_button_label
                )
            })?;
        if report.applied_field_ids.is_empty() {
            return Err(format!(
                "Шаблон «{}» содержал однозначные зоны, но ни одна не была размечена; исходный файл оставлен без изменений.",
                row.editable_button_label
            ));
        }

        let derived_text = extract_docx_text(&output_path).map_err(|error| {
            format!(
                "Не удалось проверить размеченную копию «{}»: {error}",
                row.editable_button_label
            )
        })?;
        let derived_analysis = analyze_template_text_with_domain_hint(
            &derived_text,
            row.domain_override.as_ref(),
        );
        if derived_analysis.is_static {
            return Err(format!(
                "Проверка размеченной копии «{}» не обнаружила placeholder-поля; публикация остановлена.",
                row.editable_button_label
            ));
        }
        for field_id in &report.applied_field_ids {
            if !derived_analysis.placeholders.iter().any(|value| value == field_id) {
                return Err(format!(
                    "Проверка размеченной копии «{}» не подтвердила поле {field_id}; публикация остановлена.",
                    row.editable_button_label
                ));
            }
        }

        row.template_path = output_path.display().to_string();
        row.detected_title = derived_analysis.title.clone();
        row.suggested_button_label = derived_analysis.suggested_button_label.clone();
        row.role_id = derived_analysis.role_id.clone();
        row.is_static_copy = false;
        row.analysis = derived_analysis;
        summary.upgraded_documents += 1;
        summary.inferred_fields += report.applied_field_ids.len();
    }

    Ok((updated_rows, workspace, summary))
}
