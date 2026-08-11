#[derive(Debug, Serialize)]
struct FirstRunStateResponse {
    pack: DocumentPack,
    has_user_buttons: bool,
    message: String,
}

#[tauri::command]
fn first_run_state(state: State<'_, AppState>) -> Result<FirstRunStateResponse, String> {
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let has_user_buttons = !pack.documents.is_empty();
    let message = if state.persistence_blocked.load(Ordering::SeqCst) {
        let reason = state
            .persistence_error
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| "неизвестная ошибка базы состояния".into());
        format!(
            "Восстановление состояния заблокировано для защиты данных: {reason}. Загрузите исправную резервную базу; текущие данные не будут перезаписаны."
        )
    } else if has_user_buttons {
        "Рабочий комплект загружен. Можно положить первичный документ в папку автоматизации.".into()
    } else {
        "Первоначальная настройка: выберите процесс, загрузите пустой шаблон и 3–10 заполненных примеров, проверьте предложенную карту и включите автоматизацию.".into()
    };
    Ok(FirstRunStateResponse {
        has_user_buttons,
        pack,
        message,
    })
}

#[derive(Debug, Deserialize)]
struct AnalyzeTemplateRequest {
    template_text: String,
    document_id: String,
    template_path: String,
    button_label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeTemplateFileRequest {
    template_path: String,
    document_id: String,
    button_label: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnalyzeTemplateResponse {
    document: DocumentTemplateSpec,
    analysis_json: serde_json::Value,
    core_pipeline_json: serde_json::Value,
}

#[tauri::command]
fn analyze_template(req: AnalyzeTemplateRequest) -> Result<AnalyzeTemplateResponse, String> {
    analyze_template_from_text(
        &req.template_text,
        &req.document_id,
        &req.template_path,
        req.button_label.as_deref(),
    )
}

#[tauri::command]
fn analyze_template_file(
    req: AnalyzeTemplateFileRequest,
) -> Result<AnalyzeTemplateResponse, String> {
    let path = PathBuf::from(&req.template_path);
    let text = extract_docx_text(&path).map_err(|e| e.to_string())?;
    analyze_template_from_text(
        &text,
        &req.document_id,
        &req.template_path,
        req.button_label.as_deref(),
    )
}

/// Analysis is deliberately pure. A template enters the user's pack only after
/// explicit confirmation; cancelling the dialog can no longer leave a ghost button.
fn analyze_template_from_text(
    template_text: &str,
    document_id: &str,
    template_path: &str,
    button_label: Option<&str>,
) -> Result<AnalyzeTemplateResponse, String> {
    let analysis = analyze_template_text(template_text);
    if !analysis.unknown_placeholders.is_empty() {
        return Err(format!(
            "invalid placeholder ids: {:?}",
            analysis.unknown_placeholders
        ));
    }
    let core_pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: dokkomplekt_core::core::SourceDocument {
            id: "ui-template-source".into(),
            text: String::new(),
            metadata: Default::default(),
        },
        target_template: dokkomplekt_core::core::TargetTemplate {
            id: document_id.into(),
            path: template_path.into(),
            text: template_text.into(),
        },
        domain_hint: None,
        flags: UniversalPipelineFlags::default(),
    });
    let spec = dokkomplekt_core::create_button_from_template_text(
        template_text,
        document_id,
        template_path,
        button_label,
    );
    Ok(AnalyzeTemplateResponse {
        document: spec,
        analysis_json: serde_json::to_value(analysis).map_err(|e| e.to_string())?,
        core_pipeline_json: serde_json::to_value(core_pipeline).map_err(|e| e.to_string())?,
    })
}

#[derive(Debug, Deserialize)]
struct PrepareTemplatesRequest {
    candidates: Vec<TemplateCandidate>,
}

#[tauri::command]
fn prepare_template_setup(
    req: PrepareTemplatesRequest,
) -> Result<Vec<TemplateConfirmationRow>, String> {
    Ok(prepare_template_confirmations(&req.candidates))
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
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-learning-inputs");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let safe_name = sanitize_path_component(
        Path::new(&req.file_name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("example"),
    );
    if safe_name.is_empty() {
        return Err("Имя учебного примера некорректно.".into());
    }
    let target = root.join(format!("{}-{safe_name}", Uuid::new_v4()));
    std::fs::write(&target, &bytes)
        .map_err(|error| format!("Не удалось сохранить учебный пример: {error}"))?;
    let work = root.join("normalized-work");
    let normalized = match universal_intake::normalize_path(&target, &work, 0) {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&target);
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

fn read_learning_text(app: &tauri::AppHandle, value: &str) -> Result<String, String> {
    let path = resolve_user_path(app, value)?;
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
) -> Result<TemplateLearningReport, String> {
    if req.completed_example_paths.len() < 3 {
        return Err("Добавьте минимум три заполненных примера (поддерживается 3–10).".into());
    }
    let blank_template_text = read_learning_text(&app, &req.blank_template_path)?;
    let completed_examples = req
        .completed_example_paths
        .iter()
        .take(10)
        .map(|path| read_learning_text(&app, path))
        .collect::<Result<Vec<_>, _>>()?;
    let source_examples = req
        .source_example_paths
        .iter()
        .take(10)
        .map(|path| read_learning_text(&app, path))
        .collect::<Result<Vec<_>, _>>()?;
    let report = dokkomplekt_core::learn_template_from_examples(&TemplateLearningInput {
        blank_template_text,
        completed_examples,
        source_examples,
        default_year: req.default_year,
        locale: req.locale.unwrap_or_else(|| "ru-RU".into()),
    });
    append_audit_event(
        &app,
        "template_examples_analyzed",
        "",
        &serde_json::json!({
            "blank_template_path": req.blank_template_path,
            "completed_example_count": req.completed_example_paths.len().min(10),
            "source_example_count": req.source_example_paths.len().min(10),
            "field_count": report.fields.len(),
            "confidence": report.confidence,
            "requires_confirmation": report.requires_confirmation,
        }),
    )?;
    Ok(report)
}

#[derive(Debug, Deserialize)]
struct ApplyTemplateLearningMapRequest {
    input_path: String,
    output_path: String,
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
    let input_path = resolve_user_path(&app, &req.input_path)?;
    let output_path = resolve_user_path(&app, &req.output_path)?;
    if input_path == output_path {
        return Err("Обученная карта применяется только к новой копии; исходный шаблон не перезаписывается.".into());
    }
    let report = apply_template_learning_map_file(
        &input_path,
        &output_path,
        &req.confirmed_fields,
    )
    .map_err(|error| error.to_string())?;
    append_audit_event(
        &app,
        "template_learning_map_applied",
        &format!("{:x}", Sha256::digest(output_path.display().to_string().as_bytes())),
        &serde_json::json!({
            "input_path": input_path.display().to_string(),
            "output_path": output_path.display().to_string(),
            "applied_field_ids": &report.applied_field_ids,
            "skipped_field_ids": &report.skipped_field_ids,
            "explicit_confirmation": true,
        }),
    )?;
    Ok(report)
}

fn publish_pack_with_template_versions<F>(
    app: &tauri::AppHandle,
    state: &AppState,
    drafts: &[TemplateVersionDraft],
    mutate: F,
) -> Result<(DocumentPack, Vec<TemplateVersionRecord>), String>
where
    F: FnOnce(&mut DocumentPack) -> Result<(), String>,
{
    ensure_persistence_available(state)?;
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let license = state
        .license_document
        .lock()
        .map_err(|_| "license state lock failed")?
        .clone();
    let mut pack_guard = state.pack.lock().map_err(|_| "state lock failed")?;
    let mut candidate = pack_guard.clone();
    mutate(&mut candidate)?;
    let path = default_state_db_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut repo = repository_for(&path)?;
    let versions = repo
        .save_desktop_snapshot_with_template_versions(DesktopSnapshotPublication {
            case_id: "current",
            pack_id: "default",
            case: &case,
            pack: &candidate,
            state_key: "license_document",
            state_value: &license,
            versions: drafts,
        })
        .map_err(|error| error.to_string())?;
    *pack_guard = candidate.clone();
    Ok((candidate, versions))
}

fn verify_published_template_version_file(
    path: &Path,
    record: &TemplateVersionRecord,
) -> Result<(), String> {
    let (_, _, actual_sha256) = file_content_signature(path)?;
    if actual_sha256 != record.template_sha256 {
        return Err(format!(
            "Опубликованная версия шаблона {} повреждена или изменена: ожидался SHA-256 {}, получен {}.",
            record.version_number, record.template_sha256, actual_sha256
        ));
    }
    Ok(())
}

fn bind_document_to_published_template(
    document: &mut DocumentTemplateSpec,
    record: &TemplateVersionRecord,
) -> bool {
    if document.template_path == record.template_path {
        return false;
    }
    document.template_path = record.template_path.clone();
    true
}

fn bind_loaded_pack_to_published_template_versions(
    app: &tauri::AppHandle,
    repo: &LocalRepository,
    pack: &mut DocumentPack,
) -> Result<usize, String> {
    let mut rebound = 0usize;
    for document in &mut pack.documents {
        let Some(record) = repo
            .list_template_versions(&document.id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|version| version.status == "published")
        else {
            continue;
        };
        let archived_path = resolve_user_path(app, &record.template_path)?;
        verify_published_template_version_file(&archived_path, &record)?;
        rebound += usize::from(bind_document_to_published_template(document, &record));
    }
    Ok(rebound)
}

#[derive(Debug, Deserialize)]
struct RegisterLearnedTemplateRequest {
    document_id: String,
    button_label: String,
    template_path: String,
}

#[tauri::command]
fn register_learned_template(
    req: RegisterLearnedTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let document_id = req.document_id.trim();
    let button_label = req.button_label.trim();
    if document_id.is_empty()
        || !document_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("Укажите безопасный идентификатор документа.".into());
    }
    if button_label.is_empty() {
        return Err("Укажите название кнопки.".into());
    }
    let template_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.template_path,
        button_label,
    )?;
    let text = extract_docx_text(template_snapshot.path()).map_err(|error| error.to_string())?;
    let live_template_path = template_snapshot.live_path().display().to_string();
    let mut document = dokkomplekt_core::create_button_from_template_text(
        &text,
        document_id,
        &live_template_path,
        Some(button_label),
    );
    if document.is_static_copy || document.placeholders.is_empty() {
        return Err("Обученная копия не содержит подтверждённых {{field.id}} и не может стать рабочей кнопкой.".into());
    }
    document.popup_fields = normalize_popup_fields(&document.popup_fields);
    let template_sha256 = template_snapshot.sha256().to_string();
    let draft = prepare_template_version_draft(
        &app,
        document_id,
        template_snapshot.path(),
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    document.template_path = draft.template_path.clone();
    template_snapshot.ensure_current()?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        pack.documents.retain(|item| item.id != document_id);
        if pack
            .documents
            .iter()
            .any(|item| item.button_label.eq_ignore_ascii_case(button_label))
        {
            return Err("Кнопка с таким названием уже существует.".into());
        }
        pack.documents.push(document);
        pack.documents
            .sort_by(|left, right| left.button_label.cmp(&right.button_label));
        Ok(())
    })?;
    append_audit_event(
        &app,
        "learned_template_registered",
        &template_sha256,
        &serde_json::json!({
            "document_id": document_id,
            "button_label": button_label,
            "template_path": template_snapshot.live_path().display().to_string(),
            "explicit_confirmation": true,
        }),
    )?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
struct ConfirmTemplatesRequest {
    rows: Vec<TemplateConfirmationRow>,
}

#[tauri::command]
fn confirm_template_setup(
    req: ConfirmTemplatesRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    if req.rows.is_empty() {
        return Err("Выберите хотя бы один шаблон Word.".into());
    }
    if req
        .rows
        .iter()
        .any(|row| row.editable_button_label.trim().is_empty())
    {
        return Err("У каждого шаблона должно быть название кнопки.".into());
    }
    let mut incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
    let template_snapshots = req
        .rows
        .iter()
        .map(|row| {
            template_snapshot::TemplateSnapshot::capture(
                &app,
                &row.template_path,
                &row.editable_button_label,
            )
            .map(|snapshot| (row.document_id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut drafts = Vec::with_capacity(req.rows.len());
    for row in &req.rows {
        let snapshot = template_snapshots
            .get(&row.document_id)
            .ok_or_else(|| format!("Не найден snapshot шаблона {}.", row.document_id))?;
        drafts.push(prepare_template_version_draft(
            &app,
            &row.document_id,
            snapshot.path(),
            snapshot.sha256(),
            "Первичная публикация пользовательского шаблона.",
        )?);
    }
    template_snapshot::ensure_all_current(&template_snapshots)?;
    for draft in &drafts {
        let document = incoming
            .documents
            .iter_mut()
            .find(|document| document.id == draft.document_id)
            .ok_or_else(|| format!("Не найден документ {} для привязки опубликованной версии.", draft.document_id))?;
        document.template_path = draft.template_path.clone();
    }
    let (result, _) = publish_pack_with_template_versions(&app, &state, &drafts, |pack| {
        merge_document_pack(pack, incoming);
        Ok(())
    })?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
struct RenameDocumentButtonRequest {
    document_id: String,
    button_label: String,
}

#[tauri::command]
fn rename_document_button(
    req: RenameDocumentButtonRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    transact_default_state(&app, &state, |snapshot| {
        rename_button_in_pack(&mut snapshot.pack, &req.document_id, &req.button_label)
            .map_err(|error| error.to_string())?;
        Ok((snapshot.pack.clone(), true))
    })
}

#[derive(Debug, Deserialize)]
struct RemoveDocumentButtonRequest {
    document_id: String,
}

#[tauri::command]
fn remove_document_button(
    req: RemoveDocumentButtonRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    transact_default_state(&app, &state, |snapshot| {
        remove_button_from_pack(&mut snapshot.pack, &req.document_id)
            .map_err(|error| error.to_string())?;
        Ok((snapshot.pack.clone(), true))
    })
}

#[derive(Debug, Deserialize)]
struct UpdateDocumentPopupFieldsRequest {
    document_id: String,
    popup_fields: Vec<PopupFieldConfig>,
}

#[tauri::command]
fn update_document_popup_fields(
    req: UpdateDocumentPopupFieldsRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    validate_popup_fields(&req.popup_fields)?;
    transact_default_state(&app, &state, |snapshot| {
        let document = snapshot
            .pack
            .documents
            .iter_mut()
            .find(|document| document.id == req.document_id)
            .ok_or_else(|| "document not found".to_string())?;
        let old_popup_required = document
            .popup_fields
            .iter()
            .filter(|field| field.required)
            .map(|field| field.field_id.clone())
            .collect::<BTreeSet<_>>();
        document.popup_fields = normalize_popup_fields(&req.popup_fields);
        document.popup_configured = true;
        let placeholder_fields = document
            .placeholders
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut required = document
            .required_fields
            .iter()
            .filter(|field| {
                !old_popup_required.contains(*field) || placeholder_fields.contains(*field)
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        for field in &document.popup_fields {
            if field.required {
                required.insert(field.field_id.clone());
            }
        }
        document.required_fields = required.into_iter().collect();
        Ok((snapshot.pack.clone(), true))
    })
}

#[derive(Debug, Deserialize)]
struct SetFieldRequest {
    field_id: String,
    value: String,
}

#[tauri::command]
fn set_field(
    req: SetFieldRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SemanticCase, String> {
    transact_default_state(&app, &state, |snapshot| {
        validate_field_value(&req.field_id, &req.value)?;
        let mut candidate = snapshot.semantic_case.clone();
        set_user_value(&mut candidate, req.field_id, req.value);
        if let Some((_, error)) = validate_case_relations(&candidate).into_iter().next() {
            return Err(error);
        }
        snapshot.semantic_case = candidate;
        Ok((snapshot.semantic_case.clone(), true))
    })
}

#[derive(Debug, Deserialize)]
struct ParseSourceRequest {
    source_text: String,
    default_year: i32,
}

#[derive(Debug, Serialize)]
struct ParseSourceResponse {
    semantic_case: SemanticCase,
    report: ParsedSourceReport,
    routing: DocumentRoutingRecommendation,
}

fn merge_parsed_case(target: &mut SemanticCase, parsed: SemanticCase) -> Result<(), String> {
    let mut candidate = target.clone();
    for value in parsed.values.values().cloned() {
        if let Some(conflict) = detect_field_conflict(&candidate, &value) {
            return Err(conflict.message);
        }
        dokkomplekt_core::merge_value(&mut candidate, value);
    }
    *target = candidate;
    Ok(())
}

/// A newly loaded source starts a new document-set case. Values from the previous
/// person/contract/patient must never leak into the next set. Reusable clause blocks
/// remain available because they are specialist-owned configuration, not case data.
fn replace_case_from_new_source(target: &mut SemanticCase, mut parsed: SemanticCase) {
    let mut reusable_blocks = target.blocks.clone();
    reusable_blocks.retain(|key, _| !key.starts_with("source."));
    reusable_blocks.extend(parsed.blocks);
    parsed.blocks = reusable_blocks;
    *target = parsed;
}

#[tauri::command]
fn reset_case(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<SemanticCase, String> {
    let result = transact_default_state(&app, &state, |snapshot| {
        let mut blocks = snapshot.semantic_case.blocks.clone();
        blocks.retain(|key, _| !key.starts_with("source."));
        snapshot.semantic_case = SemanticCase::default();
        snapshot.semantic_case.blocks = blocks;
        Ok((snapshot.semantic_case.clone(), true))
    })?;
    state
        .retained_uploaded_source
        .lock()
        .map_err(|_| "uploaded source state lock failed")?
        .take();
    state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")?
        .take();
    Ok(result)
}

#[tauri::command]
fn parse_source(
    req: ParseSourceRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseSourceResponse, String> {
    let provenance = SourceProvenance::from_bytes("вставленный текст", req.source_text.as_bytes());
    let (mut parsed, mut report) = parse_source_text(&req.source_text, req.default_year);
    let learned = apply_learned_scanner_rules(&app, &req.source_text, &mut parsed)?;
    if !learned.is_empty() {
        report.warnings.push(format!(
            "Сканер сам применил запомнённых полей: {}.",
            learned.len()
        ));
    }
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let routing = recommend_document_bundle(&req.source_text, &semantic_case, &snapshot.pack);
        Ok((
            ParseSourceResponse {
                semantic_case,
                report,
                routing,
            },
            true,
        ))
    })?;
    state
        .retained_uploaded_source
        .lock()
        .map_err(|_| "uploaded source state lock failed")?
        .take();
    *state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")? = Some(provenance);
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ParseSourceFileRequest {
    file_name: String,
    bytes_base64: String,
    default_year: i32,
}

#[derive(Debug, Serialize)]
struct ParseSourceFileResponse {
    source_text: String,
    source_path: String,
    source_kind: String,
    layout_items: Vec<universal_intake::NormalizedLayoutItem>,
    semantic_case: SemanticCase,
    report: ParsedSourceReport,
    routing: DocumentRoutingRecommendation,
}

#[tauri::command]
fn parse_source_file(
    req: ParseSourceFileRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseSourceFileResponse, String> {
    let mut bytes = universal_intake::decode_uploaded_payload(&req.file_name, &req.bytes_base64)?;
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("intake-work");
    let mut upload_session =
        universal_intake::normalize_uploaded_bytes(&req.file_name, &bytes, &workspace)?;
    let normalized = upload_session.take_source()?;
    let provenance = SourceProvenance::from_bytes(&req.file_name, &bytes);
    let retained_source = universal_intake::RetainedUploadedSource::new(&req.file_name, &bytes)?;
    bytes.fill(0);
    let source_path = retained_source.virtual_path();
    let source_text = normalized.text;
    let source_kind = normalized.source_kind;
    let layout_items = normalized.layout_items;
    let (mut parsed, mut report) = parse_source_text(&source_text, req.default_year);
    report.warnings.extend(normalized.warnings);
    universal_intake::apply_layout_to_case(&source_kind, &layout_items, &mut parsed);
    let learned = apply_learned_scanner_rules(&app, &source_text, &mut parsed)?;
    universal_intake::attach_layout_evidence(&layout_items, &mut parsed);
    if !learned.is_empty() {
        report.warnings.push(format!(
            "Сканер сам применил запомнённых полей: {}.",
            learned.len()
        ));
    }
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let routing = recommend_document_bundle(&source_text, &semantic_case, &snapshot.pack);
        Ok((
            ParseSourceFileResponse {
                source_text,
                source_path,
                source_kind,
                layout_items,
                semantic_case,
                report,
                routing,
            },
            true,
        ))
    })?;
    drop(upload_session);
    *state
        .retained_uploaded_source
        .lock()
        .map_err(|_| "uploaded source state lock failed")? = Some(retained_source);
    *state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")? = Some(provenance);
    Ok(response)
}

#[tauri::command]
fn get_intake_capabilities() -> Vec<universal_intake::IntakeCapability> {
    universal_intake::capabilities()
}

#[tauri::command]
fn get_sidecar_status() -> Vec<universal_intake::SidecarToolStatus> {
    universal_intake::sidecar_tool_statuses()
}

#[tauri::command]
fn get_component_statuses() -> Vec<component_manager::ComponentStatus> {
    component_manager::component_statuses()
}

#[tauri::command]
fn refresh_component_catalog(
    app: tauri::AppHandle,
) -> Result<Vec<component_manager::ComponentStatus>, String> {
    component_manager::refresh_component_catalog(&app)
}

#[tauri::command]
async fn install_component(
    app: tauri::AppHandle,
    id: String,
) -> Result<component_manager::ComponentStatus, String> {
    component_manager::install_component(app, id).await
}

#[tauri::command]
fn remove_component(id: String) -> Result<component_manager::ComponentStatus, String> {
    component_manager::remove_component(&id)
}

#[derive(Debug, Deserialize)]
struct ParseWebSourceRequest {
    url: String,
    default_year: i32,
}

#[derive(Debug, Serialize)]
struct ParseWebSourceResponse {
    source_text: String,
    final_url: String,
    content_type: String,
    semantic_case: SemanticCase,
    report: ParsedSourceReport,
    routing: DocumentRoutingRecommendation,
}

#[tauri::command]
fn parse_web_source(
    req: ParseWebSourceRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseWebSourceResponse, String> {
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("intake-work");
    let fetched = universal_intake::fetch_web_source(req.url.trim(), &workspace)?;
    let provenance = SourceProvenance::from_sha256(&fetched.final_url, &fetched.source_sha256)?;
    let (mut parsed, mut report) = parse_source_text(&fetched.source_text, req.default_year);
    report.warnings.extend(fetched.warnings);
    let learned = apply_learned_scanner_rules(&app, &fetched.source_text, &mut parsed)?;
    if !learned.is_empty() {
        report.warnings.push(format!(
            "Сканер применил запомнённых полей: {}.",
            learned.len()
        ));
    }
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let routing = recommend_document_bundle(&fetched.source_text, &semantic_case, &snapshot.pack);
        Ok((
            ParseWebSourceResponse {
                source_text: fetched.source_text,
                final_url: fetched.final_url,
                content_type: fetched.content_type,
                semantic_case,
                report,
                routing,
            },
            true,
        ))
    })?;
    state
        .retained_uploaded_source
        .lock()
        .map_err(|_| "uploaded source state lock failed")?
        .take();
    *state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")? = Some(provenance);
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct DocumentTemplateTextRequest {
    document_id: String,
}

#[derive(Debug, Serialize)]
struct DocumentTemplateTextResponse {
    template_text: String,
}

#[tauri::command]
fn get_document_template_text(
    req: DocumentTemplateTextRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentTemplateTextResponse, String> {
    let template_path = {
        let pack = state.pack.lock().map_err(|_| "state lock failed")?;
        pack.documents
            .iter()
            .find(|doc| doc.id == req.document_id)
            .map(|doc| doc.template_path.clone())
            .ok_or_else(|| "document not found".to_string())?
    };
    let path = resolve_user_path(&app, &template_path)?;
    let template_text = extract_docx_text(&path).map_err(|e| e.to_string())?;
    Ok(DocumentTemplateTextResponse { template_text })
}

#[derive(Debug, Deserialize)]
struct WorkflowPlanRequest {
    document_id: String,
    sick_leave_enabled: bool,
}

#[tauri::command]
fn get_workflow_plan(
    req: WorkflowPlanRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let doc = {
        let pack = state.pack.lock().map_err(|_| "state lock failed")?;
        pack.documents
            .iter()
            .find(|d| d.id == req.document_id)
            .cloned()
            .ok_or_else(|| "document not found".to_string())?
    };
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?;
    let plan = plan_workflow(
        &doc,
        &case,
        &WorkflowFlags {
            sick_leave_enabled: req.sick_leave_enabled,
        },
    );
    serde_json::to_value(plan).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct WorkflowPlanBatchRequest {
    document_ids: Vec<String>,
    sick_leave_enabled: bool,
}

#[tauri::command]
fn get_workflow_plan_batch(
    req: WorkflowPlanBatchRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let documents = {
        let pack = state.pack.lock().map_err(|_| "state lock failed")?;
        let requested = req
            .document_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        let found = pack
            .documents
            .iter()
            .filter(|document| requested.contains(document.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if found.len() != requested.len() {
            return Err("Один или несколько документов комплекта не найдены".into());
        }
        found
    };
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?;
    let plan = plan_workflow_batch(
        &documents,
        &case,
        &WorkflowFlags {
            sick_leave_enabled: req.sick_leave_enabled,
        },
    );
    serde_json::to_value(plan).map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct PopupApplyRequest {
    document_id: String,
    sick_leave_enabled: bool,
    answers: Vec<PopupAnswer>,
}

#[tauri::command]
fn apply_popup(
    req: PopupApplyRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PopupApplyResult, String> {
    transact_default_state(&app, &state, |snapshot| {
        let doc = snapshot
            .pack
            .documents
            .iter()
            .find(|document| document.id == req.document_id)
            .cloned()
            .ok_or_else(|| "document not found".to_string())?;
        let plan = build_merged_popup_plan(
            &doc,
            &snapshot.semantic_case,
            &WorkflowFlags {
                sick_leave_enabled: req.sick_leave_enabled,
            },
        );
        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);
        if result.accepted {
            snapshot.semantic_case = result.semantic_case.clone();
        }
        let changed = result.accepted;
        Ok((result, changed))
    })
}

#[derive(Debug, Deserialize)]
struct PopupApplyBatchRequest {
    document_ids: Vec<String>,
    sick_leave_enabled: bool,
    answers: Vec<PopupAnswer>,
}

#[tauri::command]
fn apply_popup_batch(
    req: PopupApplyBatchRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<PopupApplyResult, String> {
    transact_default_state(&app, &state, |snapshot| {
        let requested = req
            .document_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        let documents = snapshot
            .pack
            .documents
            .iter()
            .filter(|document| requested.contains(document.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if documents.len() != requested.len() {
            return Err("Один или несколько документов комплекта не найдены".into());
        }
        let plan = plan_workflow_batch(
            &documents,
            &snapshot.semantic_case,
            &WorkflowFlags {
                sick_leave_enabled: req.sick_leave_enabled,
            },
        );
        let result = apply_popup_answers(&snapshot.semantic_case, &plan, &req.answers);
        if result.accepted {
            snapshot.semantic_case = result.semantic_case.clone();
        }
        let changed = result.accepted;
        Ok((result, changed))
    })
}

#[derive(Debug, Deserialize)]
struct RenderPreviewRequest {
    template_text: String,
    strict: bool,
}

#[tauri::command]
fn render_preview(
    req: RenderPreviewRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let base = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let hydrated = hydrate_case_with_persistent_template_data(
        &app,
        &base,
        std::slice::from_ref(&req.template_text),
        false,
    )?;
    let result = render_text_template(&req.template_text, &hydrated.case, req.strict);
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct RenderDocxRequest {
    document_id: String,
    output_path: String,
    strict: bool,
}

#[tauri::command]
fn render_docx(
    req: RenderDocxRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let doc = {
        let pack = state.pack.lock().map_err(|_| "state lock failed")?;
        pack.documents
            .iter()
            .find(|d| d.id == req.document_id)
            .cloned()
            .ok_or_else(|| "document not found".to_string())?
    };
    let base_case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let template_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &doc.template_path,
        &doc.button_label,
    )?;
    let template_text = extract_docx_text(template_snapshot.path()).map_err(|e| e.to_string())?;
    // Both paths are anchored: an installed app must not depend on the process CWD.
    let desired_output = resolve_user_path(&app, &req.output_path)?;
    let reservation = UniqueFileReservation::acquire(&desired_output)?;
    let permit = reserve_generation_access(&app, &state, 1)?;
    let hydrated = match hydrate_case_with_persistent_template_data(
        &app,
        &base_case,
        &[template_text],
        true,
    ) {
        Ok(value) => value,
        Err(error) => {
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    let render_result = render_docx_with_assets(
        &app,
        template_snapshot.path(),
        &reservation.path,
        &hydrated.case,
        req.strict,
        permit.watermark.as_deref(),
    );
    let result = match render_result {
        Ok(result) => result,
        Err(error) => {
            rollback_counter_reservations(&app, &hydrated.counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error.to_string());
        }
    };
    if let Err(error) = template_snapshot.ensure_current() {
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let output_path = match reservation.commit() {
        Ok(path) => path,
        Err(error) => {
            rollback_counter_reservations(&app, &hydrated.counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if let Err(error) = template_snapshot.ensure_current() {
        let _ = std::fs::remove_file(&output_path);
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_file(&output_path);
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    // Report the real absolute location back to the user (the core RenderResult
    // deliberately knows nothing about the filesystem).
    let mut value = serde_json::to_value(result).map_err(|e| e.to_string())?;
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "output_path".into(),
            serde_json::Value::String(output_path.display().to_string()),
        );
    }
    Ok(value)
}

#[derive(Debug, Deserialize)]
struct RenderDocxBatchRequest {
    document_ids: Vec<String>,
    output_root: String,
    folder_parts: Vec<FolderNamePart>,
    strict: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CreatedDocumentOutputDto {
    document_id: String,
    label: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct RenderDocxBatchResponse {
    output_folder: String,
    created_files: Vec<String>,
    created_documents: Vec<CreatedDocumentOutputDto>,
}

#[tauri::command]
fn render_docx_batch(
    req: RenderDocxBatchRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RenderDocxBatchResponse, String> {
    let mut requested_ids = req
        .document_ids
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let mut seen_ids = BTreeSet::new();
    requested_ids.retain(|document_id| seen_ids.insert(document_id.clone()));
    if requested_ids.is_empty() {
        return Err("Не выбран ни один документ для комплекта.".into());
    }

    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let documents = requested_ids
        .iter()
        .map(|document_id| {
            pack.documents
                .iter()
                .find(|document| &document.id == document_id)
                .cloned()
                .ok_or_else(|| format!("Документ не найден: {document_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let template_snapshots = documents
        .iter()
        .map(|document| {
            template_snapshot::TemplateSnapshot::capture(
                &app,
                &document.template_path,
                &document.button_label,
            )
            .map(|snapshot| (document.id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let base_case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let output_root = resolve_user_path(&app, &req.output_root)?;
    std::fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&output_root, Duration::from_secs(24 * 60 * 60))?;
    let labels = documents
        .iter()
        .map(|document| document.button_label.clone())
        .collect::<Vec<_>>();
    let output_plan = plan_output_paths(&output_root, &base_case, &req.folder_parts, &labels);
    let desired_output_folder = output_plan.patient_folder;
    let permit =
        reserve_generation_access(&app, &state, documents.len().try_into().unwrap_or(u32::MAX))?;
    let privacy = load_privacy_preferences(&app)?;
    let stage = output_root.join(format!(
        ".dokkomplekt-manual-stage-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    if let Err(error) = std::fs::create_dir_all(&stage) {
        rollback_generation_access(&app, &state, &permit);
        return Err(error.to_string());
    }

    let mut counter_reservations = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        let mut report_case = base_case.clone();
        for document in &documents {
            let template_snapshot = template_snapshots
                .get(&document.id)
                .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", document.button_label))?;
            let template_text = extract_docx_text(template_snapshot.path()).map_err(|e| e.to_string())?;
            let hydrated = hydrate_case_with_persistent_template_data(
                &app,
                &base_case,
                &[template_text],
                true,
            )?;
            for (field_id, value) in &hydrated.case.values {
                report_case.values.insert(field_id.clone(), value.clone());
            }
            counter_reservations.extend(hydrated.counter_reservations);
            let extension = template_snapshot
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .filter(|value| value.eq_ignore_ascii_case("docm"))
                .unwrap_or("docx");
            let desired_name = stage.join(format!(
                "{}.{}",
                sanitize_path_component(&document.button_label),
                extension
            ));
            let reservation = UniqueFileReservation::acquire(&desired_name)?;
            if let Err(error) = render_docx_with_assets(
                &app,
                template_snapshot.path(),
                &reservation.path,
                &hydrated.case,
                req.strict,
                permit.watermark.as_deref(),
            ) {
                return Err(format!("Не создан «{}»: {error}", document.button_label));
            }
            paths.push(reservation.commit()?);
        }
        let generated_names = paths
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let used_field_ids = documents
            .iter()
            .flat_map(|document| document.placeholders.iter().cloned())
            .collect::<BTreeSet<_>>();
        if privacy.write_trust_report {
            let provenance = state
                .source_provenance
                .lock()
                .map_err(|_| "source provenance state lock failed")?
                .clone()
                .ok_or_else(|| {
                    "Для проверяемого отчёта сначала загрузите файл, вставьте текст или получите HTTPS-источник.".to_string()
                })?;
            write_trust_report(
                &stage,
                &report_case,
                TrustReportContext {
                    source_name: &provenance.source_name,
                    source_sha256: &provenance.source_sha256,
                    generated_names: &generated_names,
                    used_field_ids: &used_field_ids,
                    include_values: privacy.include_values_in_trust_report,
                    source_warnings: &[],
                },
            )?;
        }
        Ok(paths)
    })();

    let staged_paths = match rendered {
        Ok(paths) => paths,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let output_folder = match publish_stage_to_unique_directory(&stage, &desired_output_folder) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
        let _ = std::fs::remove_dir_all(&output_folder);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_dir_all(&output_folder);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let created_files = staged_paths
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| output_folder.join(name).display().to_string())
        .collect::<Vec<_>>();
    let created_documents = documents
        .iter()
        .zip(created_files.iter())
        .map(|(document, path)| CreatedDocumentOutputDto {
            document_id: document.id.clone(),
            label: document.button_label.clone(),
            path: path.clone(),
        })
        .collect();
    Ok(RenderDocxBatchResponse {
        output_folder: output_folder.display().to_string(),
        created_files,
        created_documents,
    })
}

#[derive(Debug, Deserialize)]
struct ScannerRequest {
    marks: Vec<ScannerMark>,
}

#[tauri::command]
fn apply_scanner(
    req: ScannerRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let report = transact_default_state(&app, &state, |snapshot| {
        let report = apply_scanner_marks(&mut snapshot.semantic_case, &req.marks);
        Ok((report, true))
    })?;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct StartWordScannerRequest {
    path: String,
    mode: String,
    #[serde(default)]
    make_working_copy: bool,
}

#[derive(Debug, Serialize)]
struct WordScannerSessionResponse {
    session_id: String,
    mode: String,
    original_path: String,
    opened_path: String,
    working_copy: bool,
    word_was_running: bool,
    automation_available: bool,
    message: String,
}

#[tauri::command]
fn start_word_scanner(
    req: StartWordScannerRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<WordScannerSessionResponse, String> {
    if !matches!(req.mode.as_str(), "source" | "template") {
        return Err("Неизвестный режим сканера.".into());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (&req.path, req.make_working_copy, &state, &app);
        Err("Автоматическое открытие, чтение выделения и закрытие Word доступно только в Windows. Используйте встроенное выделение текста.".into())
    }
    #[cfg(target_os = "windows")]
    {
        let mut sensitive_source_session = None;
        let original = if req.path.starts_with("dokkomplekt-upload://current/") {
            let workspace = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("word-scanner-work");
            let materialized = {
                let retained = state
                    .retained_uploaded_source
                    .lock()
                    .map_err(|_| "uploaded source state lock failed")?;
                retained
                    .as_ref()
                    .ok_or_else(|| "Загруженный источник уже очищен. Выберите файл заново.".to_string())?
                    .materialize(&workspace)?
            };
            let path = materialized.original_path()?;
            sensitive_source_session = Some(materialized);
            path
        } else {
            resolve_user_path(&app, &req.path)?
        };
        if !original.is_file() {
            return Err(format!(
                "Документ для сканера не найден: {}",
                original.display()
            ));
        }
        let extension = original
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(extension.to_ascii_lowercase().as_str(), "docx" | "docm") {
            return Err("Сканер мышью открывает только DOCX и DOCM.".into());
        }
        validate_safe_template_file(&original).map_err(|error| {
            format!(
                "Word-сканер заблокировал активное или внешнее содержимое документа: {error}"
            )
        })?;
        let previous = state
            .word_scanner
            .lock()
            .map_err(|_| "scanner lock failed")?
            .take();
        if let Some(previous) = previous {
            let _ = close_word_document(&previous.opened_path, previous.word_was_running, false);
            if previous.working_copy {
                let _ = std::fs::remove_file(previous.opened_path);
            }
        }
        state
            .word_scanner_source_session
            .lock()
            .map_err(|_| "scanner source lock failed")?
            .take();
        let materialized_source = sensitive_source_session.is_some();
        let opened = if req.make_working_copy && !materialized_source {
            let copy = scanner_copy_path(&app, &original)?;
            std::fs::copy(&original, &copy)
                .map_err(|error| format!("Не удалось создать безопасную копию шаблона: {error}"))?;
            copy
        } else {
            original.clone()
        };
        let word_was_running = word_process_running();
        if let Err(error) =
            shell_execute_path(&opened, "open").and_then(|_| activate_word_document(&opened, 20))
        {
            let _ = close_word_document(&opened, word_was_running, false);
            if req.make_working_copy || materialized_source {
                let _ = std::fs::remove_file(&opened);
            }
            return Err(format!(
                "Не удалось автоматически открыть документ в Word: {error}"
            ));
        }
        let session_id = Uuid::new_v4().to_string();
        let response = WordScannerSessionResponse {
            session_id: session_id.clone(),
            mode: req.mode.clone(),
            original_path: req.path.clone(),
            opened_path: opened.display().to_string(),
            working_copy: req.make_working_copy || materialized_source,
            word_was_running,
            automation_available: true,
            message: "Word открыт. Выделите значение мышью или поставьте курсор внутрь слова, затем вернитесь в Доккомплект.".into(),
        };
        *state
            .word_scanner
            .lock()
            .map_err(|_| "scanner lock failed")? = Some(WordScannerSessionState {
            session_id,
            mode: req.mode,
            opened_path: opened,
            working_copy: req.make_working_copy || materialized_source,
            word_was_running,
            last_capture: None,
        });
        *state
            .word_scanner_source_session
            .lock()
            .map_err(|_| "scanner source lock failed")? = sensitive_source_session;
        Ok(response)
    }
}

#[derive(Debug, Deserialize)]
struct ActivateWordScannerRequest {
    session_id: String,
}

#[tauri::command]
fn activate_word_scanner(
    req: ActivateWordScannerRequest,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let session = state
        .word_scanner
        .lock()
        .map_err(|_| "scanner lock failed")?
        .clone()
        .ok_or_else(|| "Сеанс сканера не запущен. Откройте документ заново.".to_string())?;
    if session.session_id != req.session_id {
        return Err("Сеанс сканера устарел. Откройте документ заново.".into());
    }
    activate_word_document(&session.opened_path, 8)?;
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct CaptureWordScannerRequest {
    session_id: String,
    #[serde(default)]
    close_after_capture: bool,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Serialize, Deserialize)]
struct PowerShellWordCapture {
    selected_text: String,
    context_text: String,
    before_text: String,
    after_text: String,
    selection_start: i64,
    selection_end: i64,
    expanded_from_cursor: bool,
    document_path: String,
}

#[derive(Debug, Serialize)]
struct WordScannerCaptureResponse {
    session_id: String,
    selected_text: String,
    context_text: String,
    before_text: String,
    after_text: String,
    selection_start: i64,
    selection_end: i64,
    expanded_from_cursor: bool,
    document_path: String,
    document_closed: bool,
}

#[tauri::command]
fn capture_word_scanner(
    req: CaptureWordScannerRequest,
    state: State<'_, AppState>,
) -> Result<WordScannerCaptureResponse, String> {
    let session = state
        .word_scanner
        .lock()
        .map_err(|_| "scanner lock failed")?
        .clone()
        .ok_or_else(|| {
            "Сеанс сканера не запущен. Нажмите «Открыть документ в Word».".to_string()
        })?;
    if session.session_id != req.session_id {
        return Err("Сеанс сканера устарел. Откройте документ заново.".into());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = req.close_after_capture;
        Err("Чтение выделения Word доступно только в Windows.".into())
    }
    #[cfg(target_os = "windows")]
    {
        let expected = powershell_quote(&session.opened_path.display().to_string());
        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$expected = [IO.Path]::GetFullPath('{expected}')
$word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application')
$target = $null
for ($i = 1; $i -le $word.Documents.Count; $i++) {{
  $candidate = $word.Documents.Item($i)
  if ([String]::Equals([IO.Path]::GetFullPath([string]$candidate.FullName), $expected, [StringComparison]::OrdinalIgnoreCase)) {{ $target = $candidate; break }}
}}
if ($null -eq $target) {{ throw 'Документ сканера не найден среди открытых документов Word.' }}
$target.Activate()
$selection = $word.Selection
$expanded = $false
if ($selection.Start -eq $selection.End) {{ $null = $selection.Expand(2); $expanded = $true }}
$text = [string]$selection.Text
if ([String]::IsNullOrWhiteSpace($text)) {{ throw 'Значение не выделено. Выделите его мышкой или поставьте курсор внутрь слова.' }}
$paragraph = $selection.Paragraphs.Item(1).Range
$context = [string]$paragraph.Text
$relativeStart = [Math]::Max(0, $selection.Start - $paragraph.Start)
$relativeEnd = [Math]::Max($relativeStart, $selection.End - $paragraph.Start)
$beforeStart = [Math]::Max(0, $relativeStart - 120)
$beforeLength = [Math]::Max(0, $relativeStart - $beforeStart)
$afterStart = [Math]::Min($context.Length, $relativeEnd)
$afterLength = [Math]::Min(120, [Math]::Max(0, $context.Length - $afterStart))
$result = [ordered]@{{
 selected_text = $text
 context_text = $context
 before_text = $(if ($beforeLength -gt 0) {{ $context.Substring($beforeStart, $beforeLength) }} else {{ '' }})
 after_text = $(if ($afterLength -gt 0) {{ $context.Substring($afterStart, $afterLength) }} else {{ '' }})
 selection_start = [long]$selection.Start
 selection_end = [long]$selection.End
 expanded_from_cursor = [bool]$expanded
 document_path = [string]$target.FullName
}}
$result | ConvertTo-Json -Compress
"#
        );
        let raw = run_hidden_powershell(&script)?;
        let last_line = raw.lines().last().unwrap_or(raw.as_str());
        let capture: PowerShellWordCapture = serde_json::from_str(last_line)
            .map_err(|error| format!("Word вернул непонятный ответ сканеру: {error}"))?;
        let cleaned = WordScannerCaptureInternal {
            selected_text: clean_word_selection(&capture.selected_text),
            context_text: clean_word_selection(&capture.context_text),
            before_text: clean_word_selection(&capture.before_text),
            after_text: clean_word_selection(&capture.after_text),
            selection_start: capture.selection_start,
            selection_end: capture.selection_end,
            expanded_from_cursor: capture.expanded_from_cursor,
        };
        if cleaned.selected_text.is_empty() {
            return Err("Word не вернул выделенный текст. Выделите значение и повторите.".into());
        }
        let mut document_closed = false;
        if req.close_after_capture {
            close_word_document(&session.opened_path, session.word_was_running, false)?;
            document_closed = true;
            *state
                .word_scanner
                .lock()
                .map_err(|_| "scanner lock failed")? = None;
            state
                .word_scanner_source_session
                .lock()
                .map_err(|_| "scanner source lock failed")?
                .take();
        } else {
            let mut guard = state
                .word_scanner
                .lock()
                .map_err(|_| "scanner lock failed")?;
            let current = guard
                .as_mut()
                .ok_or_else(|| "Сеанс сканера завершён".to_string())?;
            current.last_capture = Some(cleaned.clone());
        }
        Ok(WordScannerCaptureResponse {
            session_id: session.session_id,
            selected_text: cleaned.selected_text,
            context_text: cleaned.context_text,
            before_text: cleaned.before_text,
            after_text: cleaned.after_text,
            selection_start: cleaned.selection_start,
            selection_end: cleaned.selection_end,
            expanded_from_cursor: cleaned.expanded_from_cursor,
            document_path: capture.document_path,
            document_closed,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ApplyWordScannerSelectionRequest {
    session_id: String,
    field_id: String,
    action: String,
}

#[derive(Debug, Serialize)]
struct WordScannerApplyResponse {
    session_id: String,
    output_path: String,
    selected_text: String,
    placeholder: String,
    extracted_text: String,
    document_closed: bool,
}

#[tauri::command]
fn apply_word_scanner_selection(
    req: ApplyWordScannerSelectionRequest,
    state: State<'_, AppState>,
) -> Result<WordScannerApplyResponse, String> {
    if !is_valid_field_id(req.field_id.trim()) {
        return Err("Поле содержит недопустимые символы. Используйте вид contract.number или custom.nomer_zayavki.".into());
    }
    if !matches!(req.action.as_str(), "replace" | "insert_after") {
        return Err("Неизвестное действие сканера шаблона.".into());
    }
    let session = state
        .word_scanner
        .lock()
        .map_err(|_| "scanner lock failed")?
        .clone()
        .ok_or_else(|| "Сеанс сканера не запущен.".to_string())?;
    if session.session_id != req.session_id || session.mode != "template" {
        return Err("Сеанс разметки шаблона устарел. Откройте шаблон заново.".into());
    }
    let capture = session
        .last_capture
        .clone()
        .ok_or_else(|| "Сначала выделите значение в Word и нажмите «Я выделил».".to_string())?;
    #[cfg(not(target_os = "windows"))]
    {
        let _ = &capture;
        Err("Изменение выделения Word доступно только в Windows.".into())
    }
    #[cfg(target_os = "windows")]
    {
        let expected_path = powershell_quote(&session.opened_path.display().to_string());
        let expected_text = powershell_quote(&capture.selected_text);
        let placeholder = format!("{{{{{}}}}}", req.field_id.trim());
        let placeholder_quoted = powershell_quote(&placeholder);
        let insert_after = req.action == "insert_after";
        let quit_flag = if session.word_was_running {
            "$false"
        } else {
            "$true"
        };
        let start = capture.selection_start;
        let end = capture.selection_end;
        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$expectedPath = [IO.Path]::GetFullPath('{expected_path}')
$expectedText = '{expected_text}'
$placeholder = '{placeholder_quoted}'
$word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application')
$target = $null
for ($i = 1; $i -le $word.Documents.Count; $i++) {{
  $candidate = $word.Documents.Item($i)
  if ([String]::Equals([IO.Path]::GetFullPath([string]$candidate.FullName), $expectedPath, [StringComparison]::OrdinalIgnoreCase)) {{ $target = $candidate; break }}
}}
if ($null -eq $target) {{ throw 'Шаблон сканера уже закрыт. Откройте его заново.' }}
$target.Activate()
$selection = $word.Selection
$current = [regex]::Replace(([string]$selection.Text).Replace("`r", ' ').Replace("`n", ' ').Replace([char]7, ' '), '\s+', ' ').Trim()
if ($selection.Start -ne {start} -or $selection.End -ne {end} -or -not [String]::Equals($current, $expectedText, [StringComparison]::Ordinal)) {{
  throw 'Выделение в Word изменилось. Ничего не заменено: выделите значение заново.'
}}
if ({str_insert}) {{
  $selection.Collapse(0)
  $selection.InsertAfter($placeholder)
}} else {{
  $selection.Text = $placeholder
}}
$target.Save()
$target.Close(0)
if ({quit_flag} -and $word.Documents.Count -eq 0) {{ $word.Quit() }}
'{{"saved":true}}'
"#,
            str_insert = if insert_after { "$true" } else { "$false" }
        );
        run_hidden_powershell(&script)?;
        let extracted_text =
            extract_docx_text(&session.opened_path).map_err(|error| error.to_string())?;
        *state
            .word_scanner
            .lock()
            .map_err(|_| "scanner lock failed")? = None;
        state
            .word_scanner_source_session
            .lock()
            .map_err(|_| "scanner source lock failed")?
            .take();
        Ok(WordScannerApplyResponse {
            session_id: session.session_id,
            output_path: session.opened_path.display().to_string(),
            selected_text: capture.selected_text,
            placeholder,
            extracted_text,
            document_closed: true,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CloseWordScannerRequest {
    session_id: String,
    #[serde(default)]
    discard_working_copy: bool,
}

#[tauri::command]
fn close_word_scanner(
    req: CloseWordScannerRequest,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let session = state
        .word_scanner
        .lock()
        .map_err(|_| "scanner lock failed")?
        .take();
    let Some(session) = session else {
        return Ok(true);
    };
    if session.session_id != req.session_id {
        *state
            .word_scanner
            .lock()
            .map_err(|_| "scanner lock failed")? = Some(session);
        return Err("Сеанс сканера устарел.".into());
    }
    if let Err(error) = close_word_document(&session.opened_path, session.word_was_running, false) {
        *state
            .word_scanner
            .lock()
            .map_err(|_| "scanner lock failed")? = Some(session);
        return Err(error);
    }
    if req.discard_working_copy && session.working_copy {
        let _ = std::fs::remove_file(session.opened_path);
    }
    state
        .word_scanner_source_session
        .lock()
        .map_err(|_| "scanner source lock failed")?
        .take();
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct SaveLearnedScannerRuleRequest {
    field_id: String,
    title: String,
    selected_text: String,
    context_text: String,
    before_text: String,
    after_text: String,
    input_kind: String,
    #[serde(default)]
    source_text: Option<String>,
}

#[tauri::command]
fn save_learned_scanner_rule(
    req: SaveLearnedScannerRuleRequest,
    app: tauri::AppHandle,
) -> Result<Vec<LearnedScannerRule>, String> {
    let field_id = req.field_id.trim();
    if !is_valid_field_id(field_id) {
        return Err("Нельзя запомнить поле с недопустимым идентификатором.".into());
    }
    let selected = clean_word_selection(&req.selected_text);
    if selected.is_empty() {
        return Err("Нельзя запомнить пустое значение.".into());
    }
    let label_hint = infer_scanner_label(&req.context_text, &selected);
    let audit_label_hint = label_hint.clone();
    let layout_fingerprint = req
        .source_text
        .as_deref()
        .map(source_layout_fingerprint);
    let _rules_guard = lock_learned_scanner_rules()?;
    let mut rules = load_learned_scanner_rules(&app)?;
    rules.retain(|rule| {
        !(rule.field_id == field_id
            && rule.label_hint.eq_ignore_ascii_case(&label_hint)
            && rule.layout_fingerprint == layout_fingerprint)
    });
    rules.push(LearnedScannerRule {
        rule_id: Uuid::new_v4().to_string(),
        field_id: field_id.to_string(),
        title: req.title.trim().to_string(),
        label_hint,
        before_text: clean_word_selection(&req.before_text),
        after_text: clean_word_selection(&req.after_text),
        sample_value: selected,
        input_kind: req.input_kind.trim().to_string(),
        created_at: OffsetDateTime::now_utc().unix_timestamp().to_string(),
        layout_fingerprint,
        successful_applications: 0,
        last_applied_at: None,
        learning_status: "shadow".into(),
        shadow_observations: 1,
        shadow_agreements: 1,
        shadow_conflicts: 0,
        promoted_at: None,
    });
    rules.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.field_id.cmp(&right.field_id))
    });
    persist_learned_scanner_rules(&app, &rules)?;
    let _ = append_audit_event(
        &app,
        "scanner_rule_learned",
        field_id,
        &serde_json::json!({
            "field_id": field_id,
            "label_hint": audit_label_hint,
            "scope": "exact_layout_when_fingerprint_is_available",
            "learning_status": "shadow",
            "promotion_policy": "8/8 grounded agreements; reject after repeated conflicts",
            "layout_scoped": req.source_text.as_deref().is_some_and(|text| !text.trim().is_empty()),
        }),
    );
    Ok(rules)
}

#[tauri::command]
fn list_learned_scanner_rules(app: tauri::AppHandle) -> Result<Vec<LearnedScannerRule>, String> {
    let _rules_guard = lock_learned_scanner_rules()?;
    load_learned_scanner_rules(&app)
}

#[derive(Debug, Deserialize)]
struct DeleteLearnedScannerRuleRequest {
    rule_id: String,
}

#[tauri::command]
fn delete_learned_scanner_rule(
    req: DeleteLearnedScannerRuleRequest,
    app: tauri::AppHandle,
) -> Result<Vec<LearnedScannerRule>, String> {
    let _rules_guard = lock_learned_scanner_rules()?;
    let mut rules = load_learned_scanner_rules(&app)?;
    rules.retain(|rule| rule.rule_id != req.rule_id);
    persist_learned_scanner_rules(&app, &rules)?;
    Ok(rules)
}

#[derive(Debug, Deserialize)]
struct UpdateDocumentTemplateRequest {
    document_id: String,
    template_path: String,
    #[serde(default)]
    acknowledge_regressions: bool,
}

#[derive(Debug, Deserialize)]
struct CheckTemplateRegressionRequest {
    document_id: String,
    candidate_template_path: String,
}

fn compare_candidate_to_published_template(
    app: &tauri::AppHandle,
    document_id: &str,
    candidate_path: &Path,
) -> Result<Option<TemplateRegressionReport>, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let Some(previous) = repo
        .list_template_versions(document_id.trim())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|version| version.status == "published")
    else {
        return Ok(None);
    };
    let previous_path = resolve_user_path(app, &previous.template_path)?;
    verify_published_template_version_file(&previous_path, &previous)?;
    compare_docx_structures(&previous_path, candidate_path)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn check_template_regression(
    req: CheckTemplateRegressionRequest,
    app: tauri::AppHandle,
) -> Result<Option<TemplateRegressionReport>, String> {
    let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.candidate_template_path,
        "кандидат новой версии шаблона",
    )?;
    let result = compare_candidate_to_published_template(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
    )?;
    candidate_snapshot.ensure_current()?;
    Ok(result)
}

#[tauri::command]
fn update_document_template(
    req: UpdateDocumentTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.template_path,
        "новая версия шаблона",
    )?;
    let regression_report = compare_candidate_to_published_template(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
    )?;
    if !req.acknowledge_regressions {
        if let Some(report) = regression_report.as_ref().filter(|report| report.critical) {
            return Err(format!(
                "Обновление заблокировано Template Regression Gate: {}",
                report
                    .issues
                    .iter()
                    .filter(|issue| matches!(&issue.severity, dokkomplekt_docx::TemplateRegressionSeverity::Critical))
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    let text = extract_docx_text(candidate_snapshot.path()).map_err(|error| error.to_string())?;
    let mut updated = dokkomplekt_core::create_button_from_template_text(
        &text,
        &req.document_id,
        &candidate_snapshot.path().display().to_string(),
        None,
    );
    if updated.is_static_copy {
        return Err("Размеченная копия не содержит ни одного поля {{field.id}}.".into());
    }
    let template_sha256 = candidate_snapshot.sha256().to_string();
    let draft = prepare_template_version_draft(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
        &template_sha256,
        "Шаблон опубликован после проверенной разметки.",
    )?;
    updated.template_path = draft.template_path.clone();
    candidate_snapshot.ensure_current()?;
    let (result, versions) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        let existing = pack
            .documents
            .iter_mut()
            .find(|document| document.id == req.document_id)
            .ok_or_else(|| "Документ для обновления не найден.".to_string())?;
        updated.button_label = existing.button_label.clone();
        updated.category = existing.category.clone();
        updated.role_id = existing.role_id.clone();
        updated.popup_fields = existing.popup_fields.clone();
        updated.popup_configured = existing.popup_configured;
        updated
            .required_fields
            .extend(existing.required_fields.iter().cloned());
        updated.required_fields.extend(
            existing
                .popup_fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.field_id.clone()),
        );
        updated.required_fields.sort();
        updated.required_fields.dedup();
        *existing = updated;
        Ok(())
    })?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| "Атомарная публикация не вернула версию шаблона.".to_string())?;
    let _ = append_audit_event(
        &app,
        "template_version_published",
        &template_sha256,
        &serde_json::json!({
            "document_id": req.document_id,
            "version_id": version.version_id,
            "version_number": version.version_number,
            "regression_report": &regression_report,
            "regressions_acknowledged": req.acknowledge_regressions,
        }),
    );
    Ok(result)
}

fn archive_template_version_source(
    app: &tauri::AppHandle,
    document_id: &str,
    source: &Path,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("docx")
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "docx" | "docm") {
        return Err("Версионирование шаблонов поддерживает только DOCX/DOCM.".into());
    }
    let archive_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("template-versions")
        .join(sanitize_path_component(document_id));
    std::fs::create_dir_all(&archive_dir)
        .map_err(|error| format!("Не удалось создать архив шаблонов: {error}"))?;
    let destination = archive_dir.join(format!("{expected_sha256}.{extension}"));
    if destination.is_file() {
        let (_, _, actual) = file_content_signature(&destination)?;
        if actual == expected_sha256 {
            return Ok(destination);
        }
        return Err("Архивная копия шаблона имеет неожиданный SHA-256; публикация заблокирована.".into());
    }
    let temporary = archive_dir.join(format!(
        ".{expected_sha256}.tmp-{}",
        Uuid::new_v4()
    ));
    std::fs::copy(source, &temporary)
        .map_err(|error| format!("Не удалось создать архивную копию шаблона: {error}"))?;
    let copied_sha256 = file_content_signature(&temporary)?.2;
    if copied_sha256 != expected_sha256 {
        let _ = std::fs::remove_file(&temporary);
        return Err("Архивная копия шаблона не совпала с исходником по SHA-256.".into());
    }
    match std::fs::rename(&temporary, &destination) {
        Ok(()) => Ok(destination),
        Err(error) if destination.is_file() => {
            let _ = std::fs::remove_file(&temporary);
            let (_, _, actual) = file_content_signature(&destination)?;
            if actual == expected_sha256 {
                Ok(destination)
            } else {
                Err(format!("Конфликт публикации архивной версии шаблона: {error}"))
            }
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!("Не удалось опубликовать архивную версию шаблона: {error}"))
        }
    }
}

fn prepare_template_version_draft(
    app: &tauri::AppHandle,
    document_id: &str,
    source: &Path,
    template_sha256: &str,
    note: &str,
) -> Result<TemplateVersionDraft, String> {
    let archived_path =
        archive_template_version_source(app, document_id, source, template_sha256)?;
    Ok(TemplateVersionDraft {
        document_id: document_id.to_string(),
        template_path: archived_path.display().to_string(),
        template_sha256: template_sha256.to_string(),
        note: note.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct ListTemplateVersionsRequest {
    document_id: String,
}

#[tauri::command]
fn list_template_versions(
    req: ListTemplateVersionsRequest,
    app: tauri::AppHandle,
) -> Result<Vec<TemplateVersionRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_template_versions(req.document_id.trim())
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct RollbackTemplateVersionRequest {
    version_id: String,
}

#[tauri::command]
fn rollback_template_version(
    req: RollbackTemplateVersionRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let record = repository_for(&default_state_db_path(&app)?)?
        .template_version_by_id(req.version_id.trim())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Версия шаблона не найдена.".to_string())?;
    let path = resolve_user_path(&app, &record.template_path)?;
    verify_published_template_version_file(&path, &record)?;
    let text = extract_docx_text(&path).map_err(|error| error.to_string())?;
    let mut restored = dokkomplekt_core::create_button_from_template_text(
        &text,
        &record.document_id,
        &path.display().to_string(),
        None,
    );
    if restored.is_static_copy {
        return Err("Архивная версия больше не содержит размеченных полей.".into());
    }
    let rollback_note = format!("Rollback к версии {}.", record.version_number);
    let draft = prepare_template_version_draft(
        &app,
        &record.document_id,
        &path,
        &record.template_sha256,
        &rollback_note,
    )?;
    let (result, versions) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        let existing = pack
            .documents
            .iter_mut()
            .find(|document| document.id.as_str() == record.document_id.as_str())
            .ok_or_else(|| "Документ версии отсутствует в текущем комплекте.".to_string())?;
        restored.button_label = existing.button_label.clone();
        restored.category = existing.category.clone();
        restored.role_id = existing.role_id.clone();
        restored.popup_fields = existing.popup_fields.clone();
        restored.popup_configured = existing.popup_configured;
        restored
            .required_fields
            .extend(existing.required_fields.iter().cloned());
        restored.required_fields.sort();
        restored.required_fields.dedup();
        *existing = restored;
        Ok(())
    })?;
    let published = versions
        .into_iter()
        .next()
        .ok_or_else(|| "Атомарный rollback не вернул опубликованную версию.".to_string())?;
    append_audit_event(
        &app,
        "template_version_rollback",
        &record.template_sha256,
        &serde_json::json!({
            "document_id": record.document_id,
            "from_version_id": record.version_id,
            "published_version_id": published.version_id,
        }),
    )?;
    Ok(result)
}

#[cfg(test)]
mod published_template_binding_tests {
    use super::*;

    fn record(path: &Path, sha256: &str) -> TemplateVersionRecord {
        TemplateVersionRecord {
            version_id: "version-1".into(),
            document_id: "invoice".into(),
            version_number: 1,
            template_path: path.display().to_string(),
            template_sha256: sha256.into(),
            note: "test".into(),
            status: "published".into(),
            created_at: "2026-08-08T00:00:00Z".into(),
        }
    }

    fn document(path: &str) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            template_path: path.into(),
            category: DomainKind::Generic,
            role_id: "generic".into(),
            required_fields: Vec::new(),
            placeholders: vec!["invoice.number".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn active_document_binding_replaces_mutable_live_path_with_published_archive() {
        let archive = PathBuf::from("C:/app-data/template-versions/invoice/hash.docx");
        let version = record(&archive, &"a".repeat(64));
        let mut document = document("C:/Users/user/Documents/invoice.docx");
        assert!(bind_document_to_published_template(&mut document, &version));
        assert_eq!(document.template_path, version.template_path);
        assert!(!bind_document_to_published_template(&mut document, &version));
    }

    #[test]
    fn published_template_sha_verification_rejects_archive_mutation() {
        let root = std::env::temp_dir().join(format!(
            "dkk-published-template-binding-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("template.docx");
        std::fs::write(&archive, b"published-template-v1").unwrap();
        let sha256 = hex::encode(Sha256::digest(b"published-template-v1"));
        let version = record(&archive, &sha256);
        verify_published_template_version_file(&archive, &version).unwrap();
        std::fs::write(&archive, b"tampered-template-v2").unwrap();
        assert!(verify_published_template_version_file(&archive, &version).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[derive(Debug, Deserialize)]
struct DiaryPlanRequest {
    admission_date: Option<String>,
    discharge_date: Option<String>,
    default_year: i32,
}

#[tauri::command]
fn get_diary_plan(req: DiaryPlanRequest) -> Result<serde_json::Value, String> {
    let result = build_diary_plan(
        req.admission_date.as_deref(),
        req.discharge_date.as_deref(),
        req.default_year,
    )
    .map_err(|e| format!("{:?}", e))?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Domain-neutral repeated-record planner used by journals, shift reports,
/// inspections, lessons, observations and other recurring professional records.
#[tauri::command]
fn get_record_series_plan(req: SeriesPlanRequest) -> Result<serde_json::Value, String> {
    let result = build_series_plan(&req).map_err(|error| error.to_string())?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[tauri::command]
fn icd10_suggest(query: String) -> Result<serde_json::Value, String> {
    serde_json::to_value(suggest_icd10(&query)).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct OutputPlanRequest {
    root_folder: String,
    folder_parts: Vec<FolderNamePart>,
    button_labels: Vec<String>,
}

#[tauri::command]
fn get_output_plan(
    req: OutputPlanRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?;
    let plan = plan_output_paths(
        Path::new(&req.root_folder),
        &case,
        &req.folder_parts,
        &req.button_labels,
    );
    serde_json::to_value(plan).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct IntakeRouteRequest {
    app_already_running: bool,
    user_requested_ui: bool,
}

#[tauri::command]
fn route_intake(
    req: IntakeRouteRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    drop(state.intake_dedup.lock().map_err(|_| "state lock failed")?);
    serde_json::to_value(route_intake_event(
        req.app_already_running,
        req.user_requested_ui,
    ))
    .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct SaveStateRequest {
    db_path: String,
}

#[tauri::command]
fn save_state(
    req: SaveStateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db_path = resolve_user_path(&app, &req.db_path)?;
    persist_state_to(&db_path, &state)
}

#[derive(Debug, Deserialize)]
struct LoadStateRequest {
    db_path: String,
}

fn load_state_from(
    app: &tauri::AppHandle,
    db_path: &Path,
    state: &AppState,
    load_commercial_state: bool,
) -> Result<(), String> {
    let repo = repository_for(db_path)?;
    repo.quick_integrity_check().map_err(|error| error.to_string())?;

    // Decode and validate every row before touching the live in-memory state.
    // A damaged late row can therefore never leave a mixed old/new snapshot.
    let loaded_case = repo.load_case("current").map_err(|error| error.to_string())?;
    let loaded_pack = repo.load_pack("default").map_err(|error| error.to_string())?;
    let loaded_license = if load_commercial_state {
        repo.load_state_value::<Option<LicenseDocument>>("license_document")
            .map_err(|error| error.to_string())?
    } else {
        None
    };
    if let Some(Some(document)) = loaded_license.as_ref() {
        verify_license_document_now(document, &trusted_license_key()?)
            .map_err(|error| format!("Сохранённая лицензия недействительна: {error}"))?;
    }
    let loaded_pack = if let Some(mut pack) = loaded_pack {
        let rebound = bind_loaded_pack_to_published_template_versions(app, &repo, &mut pack)?;
        if rebound > 0 && load_commercial_state {
            repo.save_pack(&pack).map_err(|error| error.to_string())?;
        }
        Some(pack)
    } else {
        None
    };

    let mut case_guard = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?;
    let mut pack_guard = state.pack.lock().map_err(|_| "state lock failed")?;
    let mut license_guard = if load_commercial_state {
        Some(
            state
                .license_document
                .lock()
                .map_err(|_| "license state lock failed")?,
        )
    } else {
        None
    };
    if let Some(case) = loaded_case {
        *case_guard = case;
    }
    if let Some(pack) = loaded_pack {
        *pack_guard = pack;
    }
    if let (Some(guard), Some(license)) = (license_guard.as_mut(), loaded_license) {
        **guard = license;
    }
    drop(license_guard);
    drop(pack_guard);
    drop(case_guard);

    *state.db_path.lock().map_err(|_| "state lock failed")? = Some(db_path.to_path_buf());
    state.persistence_blocked.store(false, Ordering::SeqCst);
    if let Ok(mut slot) = state.persistence_error.lock() {
        *slot = None;
    }
    Ok(())
}

#[tauri::command]
fn load_state(
    req: LoadStateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FirstRunStateResponse, String> {
    let db_path = resolve_user_path(&app, &req.db_path)?;
    load_state_from(&app, &db_path, &state, false)?;
    first_run_state(state)
}

#[derive(Debug, Deserialize)]
struct ProductAccessRequest {
    code: Option<String>,
}

#[tauri::command]
fn validate_product_access(
    req: ProductAccessRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    if req
        .code
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return Err(
            "Локальные VIP-коды отключены: права выдаются только подписанной Ed25519-лицензией."
                .into(),
        );
    }
    serde_json::to_value(inspect_desktop_access(&app, &state, 0)?)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct RustLicenseVerifyRequest {
    license_text: String,
    /// Debug-only override for integration tests; ignored/rejected in release.
    #[serde(default)]
    public_key_b64: Option<String>,
}

#[tauri::command]
fn verify_rust_license_text(
    req: RustLicenseVerifyRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    // Ed25519 verification is delegated to the ported license-core crate. Beyond a
    // valid strict signature, the license must also be inside its validity window —
    // a validly-signed but expired (or not-yet-valid) license is rejected.
    //
    // The trust anchor is the compiled-in TRUSTED_LICENSE_PUBKEY_B64. A key supplied
    // by the caller is honored only in debug builds (integration tests); in release
    // it is rejected outright, so the UI can never swap the anchor.
    let key_b64 = match req
        .public_key_b64
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => TRUSTED_LICENSE_PUBKEY_B64,
        Some(overridden) => {
            if cfg!(debug_assertions) {
                overridden
            } else {
                return Err(
                    "Переопределение доверенного ключа лицензии запрещено в релизной сборке."
                        .into(),
                );
            }
        }
    };
    let document: dokkomplekt_license_core::LicenseDocument =
        serde_json::from_str(&req.license_text).map_err(|e| e.to_string())?;
    let public_key = dokkomplekt_license_core::PublicKeyBytes::from_base64(key_b64)
        .map_err(|e| e.to_string())?;
    verify_license_document_now(&document, &public_key).map_err(|e| e.to_string())?;
    transact_default_state(&app, &state, |snapshot| {
        snapshot.license_document = Some(document);
        Ok((true, true))
    })
}

include!("watcher_commands.rs");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreatedDocumentsIntakeRequest {
    source_path: String,
    output_root: String,
    #[serde(default)]
    folder_parts: Vec<FolderNamePart>,
    default_year: i32,
    #[serde(default)]
    sick_leave_enabled: bool,
    #[serde(default)]
    model_output: Option<String>,
    #[serde(default)]
    confirmed_fields: Vec<String>,
    #[serde(default)]
    confirmed_document_ids: Vec<String>,
    #[serde(default)]
    force_reissue: bool,
    #[serde(default)]
    preserve_source_after_success: bool,
    #[serde(default)]
    resume_from_case_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CreatedDocumentsIntakeResponse {
    status: String,
    patient_folder: Option<String>,
    created_files: Vec<String>,
    created_documents: Vec<CreatedDocumentOutputDto>,
    missing: Vec<String>,
    attention_file: Option<String>,
    print_triage: Option<PrintTriageReport>,
    message: String,
}

/// Zero-touch «Созданные документы» run: one dropped primary document -> the whole
/// configured set into a fresh output folder, or a safe attention note when data
/// is missing. Decision logic lives in dokkomplekt_core; this command only does IO.
#[tauri::command]
fn run_created_documents_intake(
    req: CreatedDocumentsIntakeRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let source_path = req.source_path.clone();
    match perform_created_documents_intake(&state, &app, req) {
        Ok(response) => serde_json::to_value(response).map_err(|e| e.to_string()),
        Err(error) => {
            increment_metric(&app, "failed_sources", 1);
            let details = serde_json::json!({ "error": &error });
            let _ = create_automation_exception(
                &app,
                "processing_error",
                &source_path,
                "Источник не обработан.",
                &details,
            );
            let _ = append_audit_event(&app, "intake_failed", "", &details);
            Err(error)
        }
    }
}
