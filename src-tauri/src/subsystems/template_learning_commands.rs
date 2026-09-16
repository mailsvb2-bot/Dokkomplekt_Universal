
#[derive(Debug, Deserialize)]
struct PickLearningFilesRequest {
    kind: String,
    #[serde(default)]
    initial_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct PickedLearningFile {
    file_name: String,
    staged_path: String,
    content_sha256: String,
}

#[derive(Debug, Serialize)]
struct PickLearningFilesResponse {
    files: Vec<PickedLearningFile>,
}

/// Use the same operating-system picker boundary as the rest of the packaged app,
/// then copy the chosen inputs into the existing protected learning workspace.
/// No WebView file-input bytes are trusted for Template Learning on desktop.
#[tauri::command]
async fn pick_learning_files(
    req: PickLearningFilesRequest,
    app: tauri::AppHandle,
) -> Result<PickLearningFilesResponse, String> {
    let kind = req.kind.trim().to_string();
    let picker_kind = kind.clone();
    let selected_paths = tauri::async_runtime::spawn_blocking(move || match picker_kind.as_str() {
        "blank" | "correct_output" => pick_template_files_blocking(req.initial_path),
        "source" => pick_source_file_blocking(req.initial_path)
            .map(|selected| selected.into_iter().collect::<Vec<_>>()),
        _ => Err(format!("Неизвестная роль файла обучения: {picker_kind}")),
    })
    .await
    .map_err(|error| format!("Не удалось открыть системный выбор файлов обучения: {error}"))??;

    if selected_paths.is_empty() {
        return Ok(PickLearningFilesResponse { files: Vec::new() });
    }
    if kind == "blank" && selected_paths.len() != 1 {
        return Err("Для обучения выберите ровно один пустой DOCX/DOCM-шаблон.".into());
    }

    let _learning_guard = lock_learning_workspace()?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-learning-inputs");
    let session_root = universal_intake::create_retained_workspace_session(&root)?;
    let mut files = Vec::with_capacity(selected_paths.len());
    for selected_path in selected_paths {
        let canonical = selected_path.canonicalize().map_err(|error| {
            format!("Не удалось открыть выбранный файл обучения «{}»: {error}", selected_path.display())
        })?;
        let metadata = std::fs::metadata(&canonical).map_err(|error| {
            format!("Не удалось прочитать выбранный файл обучения «{}»: {error}", canonical.display())
        })?;
        if !metadata.is_file() {
            return Err(format!("Выбранный путь не является файлом: {}", canonical.display()));
        }
        let file_name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Имя выбранного файла обучения не поддерживается системой.".to_string())?
            .to_string();

        if kind == "blank" || kind == "correct_output" {
            if metadata.len() > MAX_PICKED_TEMPLATE_BYTES {
                return Err("DOCX/DOCM для обучения слишком большой: максимум 50 МБ.".into());
            }
            let extension = canonical
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if extension != "docx" && extension != "docm" {
                return Err(format!("Для {kind} поддерживаются только DOCX и DOCM: {}", canonical.display()));
            }
            validate_safe_template_file(&canonical).map_err(|error| {
                format!("Файл обучения «{}» содержит активное содержимое или внешние связи и заблокирован: {error}", canonical.display())
            })?;
        } else if metadata.len() > universal_intake::MAX_SOURCE_FILE_BYTES {
            return Err(format!(
                "Source-файл слишком большой: максимум {} МБ.",
                universal_intake::MAX_SOURCE_FILE_BYTES / (1024 * 1024)
            ));
        }

        let extension = canonical
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        let (_, _, content_sha256) = file_content_signature(&canonical)?;
        let target = session_root.join(format!("{}{}", Uuid::new_v4(), extension));
        std::fs::copy(&canonical, &target).map_err(|error| {
            format!("Не удалось сохранить защищённую копию файла обучения «{file_name}»: {error}")
        })?;
        files.push(PickedLearningFile {
            file_name,
            staged_path: target.display().to_string(),
            content_sha256,
        });
    }
    Ok(PickLearningFilesResponse { files })
}

#[derive(Debug, Deserialize)]
struct ImportLearningExampleFileRequest {
    file_name: String,
    bytes_base64: String,
}

#[derive(Debug, Serialize)]
struct ImportLearningExampleFileResponse {
    source_path: String,
    source_kind: String,
    extracted_text: String,
    warnings: Vec<String>,
}

/// Persist and validate a user-supplied learning example. Unlike template import,
/// this accepts every format supported by the universal intake pipeline, so source
/// examples can be PDF, images, spreadsheets, e-mail or archives. The original
/// upload is retained only in the local app-data learning workspace.
#[tauri::command]
fn import_learning_example_file(
    req: ImportLearningExampleFileRequest,
    app: tauri::AppHandle,
) -> Result<ImportLearningExampleFileResponse, String> {
    let bytes = universal_intake::decode_uploaded_payload(&req.file_name, &req.bytes_base64)?;
    let _learning_guard = lock_learning_workspace()?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-learning-inputs");
    let session_root = universal_intake::create_retained_workspace_session(&root)?;
    let safe_name = sanitize_path_component(
        Path::new(&req.file_name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("example"),
    );
    if safe_name.is_empty() {
        return Err("Имя учебного примера некорректно.".into());
    }
    let target = session_root.join(safe_name);
    std::fs::write(&target, &bytes)
        .map_err(|error| format!("Не удалось сохранить учебный пример: {error}"))?;
    let work = session_root.join("normalized-work");
    let normalized = match universal_intake::normalize_path(&target, &work, 0) {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&session_root);
            return Err(error);
        }
    };
    append_audit_event(
        &app,
        "template_learning_example_imported",
        &format!("{:x}", Sha256::digest(&bytes)),
        &serde_json::json!({
            "file_name": req.file_name,
            "source_kind": normalized.source_kind,
            "byte_count": bytes.len(),
            "document_text_not_logged": true,
        }),
    )?;
    Ok(ImportLearningExampleFileResponse {
        source_path: target.display().to_string(),
        source_kind: normalized.source_kind,
        extracted_text: normalized.text,
        warnings: normalized.warnings,
    })
}

#[derive(Debug, Deserialize)]
struct LearnTemplateFromExamplesRequest {
    blank_template_path: String,
    completed_example_paths: Vec<String>,
    #[serde(default)]
    source_example_paths: Vec<String>,
    default_year: i32,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Debug, Serialize)]
struct TemplateLearningCommandResponse {
    #[serde(flatten)]
    report: TemplateLearningReport,
    validation_id: Option<String>,
    candidate_mapping_sha256: Option<String>,
}

fn learning_path_sha256(app: &tauri::AppHandle, value: &str) -> Result<String, String> {
    let path = resolve_user_path(app, value)?;
    file_content_signature(&path).map(|(_, _, sha256)| sha256)
}

fn canonical_learning_map_fields(report: &TemplateLearningReport) -> Vec<TemplateLearningMapField> {
    report
        .fields
        .iter()
        .filter(|field| {
            field
                .source_matches
                .iter()
                .any(|value| !value.trim().is_empty())
        })
        .map(|field| TemplateLearningMapField {
            field_id: field.field_id.clone(),
            line_index: field.line_index,
            blank_line: field.blank_line.clone(),
            common_prefix: field.common_prefix.clone(),
            common_suffix: field.common_suffix.clone(),
        })
        .collect()
}

fn learning_map_sha256(fields: &[TemplateLearningMapField]) -> Result<String, String> {
    let mut canonical = fields.to_vec();
    canonical.sort_by(|left, right| {
        left.field_id
            .cmp(&right.field_id)
            .then_with(|| left.line_index.cmp(&right.line_index))
            .then_with(|| left.blank_line.cmp(&right.blank_line))
            .then_with(|| left.common_prefix.cmp(&right.common_prefix))
            .then_with(|| left.common_suffix.cmp(&right.common_suffix))
    });
    let payload = serde_json::to_vec(&canonical).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn confirmed_learning_map_is_subset_of_evidence(
    confirmed: &[TemplateLearningMapField],
    evidence_json: &str,
) -> Result<(), String> {
    let evidence: serde_json::Value =
        serde_json::from_str(evidence_json).map_err(|error| error.to_string())?;
    let validated_fields = serde_json::from_value::<Vec<TemplateLearningMapField>>(
        evidence
            .get("validated_fields")
            .cloned()
            .ok_or_else(|| "Learning evidence does not contain validated_fields.".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let validated = validated_fields.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    for field in confirmed {
        if !seen.insert(field.field_id.as_str()) {
            return Err(format!(
                "Поле {} повторяется в подтверждённой карте.",
                field.field_id
            ));
        }
        if !validated.iter().any(|candidate| candidate == field) {
            return Err(format!(
                "Поле {} не принадлежит карте, прошедшей независимую проверку.",
                field.field_id
            ));
        }
    }
    Ok(())
}

fn read_learning_text(app: &tauri::AppHandle, value: &str) -> Result<String, String> {
    let path = resolve_user_path(app, value)?;
    let learning_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-learning-inputs");
    let _ = universal_intake::refresh_retained_workspace_session(&learning_root, &path)?;
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "docx" | "docm") {
        return extract_docx_text(&path).map_err(|error| error.to_string());
    }
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-learning-work");
    universal_intake::normalize_path(&path, &workspace, 0).map(|source| source.text)
}

#[tauri::command]
fn learn_template_from_examples_command(
    req: LearnTemplateFromExamplesRequest,
    app: tauri::AppHandle,
) -> Result<TemplateLearningCommandResponse, String> {
    let pair_count = req.completed_example_paths.len();
    if !(4..=10).contains(&pair_count) {
        return Err("Для доказательного обучения нужны 4–10 пар Source → Correct Output: минимум 3 обучающие и 1 контрольная.".into());
    }
    if req.source_example_paths.len() != pair_count {
        return Err("Количество Source и Correct Output должно совпадать; неполные пары не участвуют в обучении.".into());
    }
    if req
        .completed_example_paths
        .iter()
        .any(|path| path.trim().is_empty())
        || req
            .source_example_paths
            .iter()
            .any(|path| path.trim().is_empty())
    {
        return Err("Пустой путь в Source → Correct Output недопустим.".into());
    }
    let _learning_guard = lock_learning_workspace()?;
    let blank_template_text = read_learning_text(&app, &req.blank_template_path)?;
    let completed_examples = req
        .completed_example_paths
        .iter()
        .map(|path| read_learning_text(&app, path))
        .collect::<Result<Vec<_>, _>>()?;
    let source_examples = req
        .source_example_paths
        .iter()
        .map(|path| read_learning_text(&app, path))
        .collect::<Result<Vec<_>, _>>()?;
    let report = dokkomplekt_core::learn_template_from_examples(&TemplateLearningInput {
        blank_template_text,
        completed_examples,
        source_examples,
        default_year: req.default_year,
        locale: req.locale.clone().unwrap_or_else(|| "ru-RU".into()),
    });

    let (validation_id, candidate_mapping_sha256) = if report.validation.publishable
        && report.validation.verdict == dokkomplekt_core::TemplateLearningValidationState::Passed
        && report.validation.passed
    {
        let validated_fields = canonical_learning_map_fields(&report);
        if validated_fields.is_empty() {
            return Err(
                "Контрольная проверка не оставила ни одного source-evidenced поля для публикации."
                    .into(),
            );
        }
        let blank_template_sha256 = learning_path_sha256(&app, &req.blank_template_path)?;
        let source_sha256s = req
            .source_example_paths
            .iter()
            .map(|path| learning_path_sha256(&app, path))
            .collect::<Result<Vec<_>, _>>()?;
        let correct_output_sha256s = req
            .completed_example_paths
            .iter()
            .map(|path| learning_path_sha256(&app, path))
            .collect::<Result<Vec<_>, _>>()?;
        let mapping_sha256 = learning_map_sha256(&validated_fields)?;
        let evidence_json = serde_json::to_string(&serde_json::json!({
            "schema": "dokkomplekt.template-learning-validation.v2",
            "blank_template_sha256": &blank_template_sha256,
            "candidate_mapping_sha256": &mapping_sha256,
            "source_sha256s": source_sha256s,
            "correct_output_sha256s": correct_output_sha256s,
            "holdout_pair_index": report.validation.holdout_pair_index,
            "validation": &report.validation,
            "validated_fields": &validated_fields,
            "source_profile_field_ids": validated_fields.iter().map(|field| field.field_id.as_str()).collect::<Vec<_>>(),
            "locale": req.locale.as_deref().unwrap_or("ru-RU"),
            "default_year": req.default_year,
        }))
        .map_err(|error| error.to_string())?;
        let repo = repository_for(&default_state_db_path(&app)?)?;
        let validation = repo
            .register_template_learning_validation(
                &blank_template_sha256,
                &mapping_sha256,
                &evidence_json,
            )
            .map_err(|error| error.to_string())?;
        (Some(validation.validation_id), Some(mapping_sha256))
    } else {
        (None, None)
    };

    append_audit_event(
        &app,
        "template_examples_analyzed",
        validation_id.as_deref().unwrap_or(""),
        &serde_json::json!({
            "blank_template_path": req.blank_template_path,
            "completed_example_count": pair_count,
            "source_example_count": req.source_example_paths.len(),
            "field_count": report.fields.len(),
            "confidence": report.confidence,
            "requires_confirmation": report.requires_confirmation,
            "validation_verdict": report.validation.verdict,
            "publishable": report.validation.publishable,
            "validation_id": &validation_id,
            "candidate_mapping_sha256": &candidate_mapping_sha256,
            "missing_source_field_count": report.validation.missing_source_field_ids.len(),
            "mismatched_field_count": report.validation.mismatched_field_ids.len(),
        }),
    )?;
    Ok(TemplateLearningCommandResponse {
        report,
        validation_id,
        candidate_mapping_sha256,
    })
}

#[derive(Debug, Deserialize)]
struct ApplyTemplateLearningMapRequest {
    input_path: String,
    output_path: String,
    validation_id: String,
    confirmed_fields: Vec<TemplateLearningMapField>,
}

#[tauri::command]
fn apply_template_learning_map(
    req: ApplyTemplateLearningMapRequest,
    app: tauri::AppHandle,
) -> Result<TemplateLearningMapReport, String> {
    if req.confirmed_fields.is_empty() {
        return Err("Подтвердите хотя бы одно найденное поле.".into());
    }
    let validation_id = req.validation_id.trim();
    if validation_id.is_empty() {
        return Err("Карта не имеет независимого validation proof; повторите обучение на 4–10 полных парах.".into());
    }
    let input_path = resolve_user_path(&app, &req.input_path)?;
    let output_path = resolve_user_path(&app, &req.output_path)?;
    if input_path == output_path {
        return Err("Обученная карта применяется только к новой копии; исходный шаблон не перезаписывается.".into());
    }
    let (_, _, input_sha256) = file_content_signature(&input_path)?;
    let mut repo = repository_for(&default_state_db_path(&app)?)?;
    let validation = repo
        .template_learning_validation_by_id(validation_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Validation proof не найден в локальном version store.".to_string())?;
    if validation.status != "validated" {
        return Err(format!(
            "Validation proof нельзя применить из состояния {}.",
            validation.status
        ));
    }
    if validation.blank_template_sha256 != input_sha256 {
        return Err("Validation proof относится к другой версии пустого шаблона.".into());
    }
    let evidence: serde_json::Value = serde_json::from_str(&validation.evidence_json)
        .map_err(|error| format!("Learning evidence повреждён: {error}"))?;
    let validated_fields = serde_json::from_value::<Vec<TemplateLearningMapField>>(
        evidence
            .get("validated_fields")
            .cloned()
            .ok_or_else(|| "Learning evidence не содержит validated_fields.".to_string())?,
    )
    .map_err(|error| format!("Learning evidence содержит некорректную карту: {error}"))?;
    let persisted_candidate_hash = learning_map_sha256(&validated_fields)?;
    if persisted_candidate_hash != validation.candidate_mapping_sha256 {
        return Err("Learning evidence не совпадает с зарегистрированным hash карты.".into());
    }
    confirmed_learning_map_is_subset_of_evidence(&req.confirmed_fields, &validation.evidence_json)?;
    let applied_mapping_sha256 = learning_map_sha256(&req.confirmed_fields)?;

    let report = apply_template_learning_map_file(&input_path, &output_path, &req.confirmed_fields)
        .map_err(|error| error.to_string())?;
    if !report.skipped_field_ids.is_empty()
        || report.applied_field_ids.len() != req.confirmed_fields.len()
    {
        let _ = std::fs::remove_file(&output_path);
        return Err(format!(
            "Проверенная карта применилась не полностью: вставлено {}, пропущено {}. Кандидат удалён и не может быть опубликован.",
            report.applied_field_ids.len(),
            report.skipped_field_ids.len()
        ));
    }
    let (_, _, output_sha256) = file_content_signature(&output_path)?;
    repo.bind_template_learning_validation_output(
        validation_id,
        &input_sha256,
        &applied_mapping_sha256,
        &output_sha256,
    )
    .map_err(|error| error.to_string())?;
    append_audit_event(
        &app,
        "template_learning_map_applied",
        &output_sha256,
        &serde_json::json!({
            "validation_id": validation_id,
            "input_sha256": input_sha256,
            "output_sha256": output_sha256,
            "applied_mapping_sha256": applied_mapping_sha256,
            "applied_field_ids": &report.applied_field_ids,
            "skipped_field_ids": &report.skipped_field_ids,
            "explicit_confirmation": true,
        }),
    )?;
    Ok(report)
}
