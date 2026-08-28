// Real-source intake boundary shared by interactive desktop and automation.
// Bundle semantics stay in dokkomplekt-core; this module only resolves the
// persisted learned decision and owns source/case replacement commands.

fn resolve_document_bundle_for_case(
    app: &tauri::AppHandle,
    source_text: &str,
    case: &SemanticCase,
    pack: &DocumentPack,
    learning_domain: Option<DomainKind>,
    specialist_confirmed_ids: &[String],
) -> Result<(DocumentRoutingRecommendation, BundleDecision), String> {
    let routing = recommend_document_bundle(source_text, case, pack);
    let domain = learning_domain
        .or_else(|| case.active_domains.first().cloned())
        .filter(|value| *value != DomainKind::Generic)
        .unwrap_or_else(|| routing.domain.clone());
    let corpus_entries = repository_for(&default_state_db_path(app)?)?
        .list_corpus_entries(10_000)
        .map_err(|error| error.to_string())?;
    let key = KitRuleKey {
        domain,
        cluster_id: routing.cluster_id.clone(),
        pack_id: (!pack.pack_id.trim().is_empty()).then(|| pack.pack_id.clone()),
    };
    let learned = decision_for_key(&corpus_entries, &key, KitPromotionPolicy::default());
    let decision = decide_document_bundle(
        pack,
        &routing,
        learned.as_ref(),
        specialist_confirmed_ids,
    );
    Ok((routing, decision))
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
    bundle_decision: BundleDecision,
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

/// A newly loaded source starts a new document-set case. Values and blocks from the
/// previous person/contract/patient must never leak into the next set. Reusable
/// profile blocks are rehydrated from the clause-block store at render time, so the
/// parsed source itself is the only block source carried into the new active case.
fn replace_case_from_new_source(target: &mut SemanticCase, parsed: SemanticCase) {
    *target = parsed;
}

type SourceSessionGuards<'a> = (
    std::sync::MutexGuard<'a, Option<universal_intake::RetainedUploadedSource>>,
    std::sync::MutexGuard<'a, Option<SourceProvenance>>,
);

fn lock_source_session_state(state: &AppState) -> Result<SourceSessionGuards<'_>, String> {
    // Acquire both fallible in-memory locks before the durable state transaction.
    // A poisoned lock must never make an intake command report failure after the
    // new case has already been committed to SQLite. Keep one global lock order
    // (retained source -> provenance) to avoid cross-command deadlocks.
    let retained = state
        .retained_uploaded_source
        .lock()
        .map_err(|_| "uploaded source state lock failed")?;
    let provenance = state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")?;
    Ok((retained, provenance))
}

#[tauri::command]
fn reset_case(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<SemanticCase, String> {
    let (mut retained, mut provenance) = lock_source_session_state(&state)?;
    let result = transact_default_state(&app, &state, |snapshot| {
        snapshot.semantic_case = SemanticCase::default();
        Ok((snapshot.semantic_case.clone(), true))
    })?;
    retained.take();
    provenance.take();
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
    let (mut retained, mut source_provenance) = lock_source_session_state(&state)?;
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let (routing, bundle_decision) = resolve_document_bundle_for_case(
            &app,
            &req.source_text,
            &semantic_case,
            &snapshot.pack,
            None,
            &[],
        )?;
        Ok((
            ParseSourceResponse {
                semantic_case,
                report,
                routing,
                bundle_decision,
            },
            true,
        ))
    })?;
    retained.take();
    *source_provenance = Some(provenance);
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ParseSourceFileRequest {
    file_name: String,
    bytes_base64: String,
    default_year: i32,
}

#[derive(Debug, Deserialize)]
struct PickSourceFileRequest {
    #[serde(default)]
    initial_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct PickedSourceFileResponse {
    file_name: String,
    selected_path: String,
}

#[derive(Debug, Deserialize)]
struct ParseSourcePathRequest {
    selected_path: String,
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
    bundle_decision: BundleDecision,
}

#[tauri::command]
fn parse_source_file(
    req: ParseSourceFileRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseSourceFileResponse, String> {
    let bytes = universal_intake::decode_uploaded_payload(&req.file_name, &req.bytes_base64)?;
    parse_source_file_bytes(req.file_name, bytes, req.default_year, state, app)
}

#[tauri::command]
async fn pick_source_file(
    req: PickSourceFileRequest,
) -> Result<Option<PickedSourceFileResponse>, String> {
    let selected_path = tauri::async_runtime::spawn_blocking(move || {
        pick_source_file_blocking(req.initial_path)
    })
    .await
    .map_err(|error| format!("Не удалось открыть выбор исходного документа: {error}"))??;
    let Some(selected_path) = selected_path else {
        return Ok(None);
    };
    let (canonical, file_name) = validate_source_path(&selected_path)?;
    Ok(Some(PickedSourceFileResponse {
        file_name,
        selected_path: canonical.display().to_string(),
    }))
}

#[tauri::command]
fn parse_source_path(
    req: ParseSourcePathRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseSourceFileResponse, String> {
    let (canonical, file_name) = validate_source_path(Path::new(&req.selected_path))?;
    let bytes = std::fs::read(&canonical).map_err(|error| {
        format!(
            "Не удалось прочитать выбранный исходник «{}»: {error}",
            canonical.display()
        )
    })?;
    parse_source_file_bytes(file_name, bytes, req.default_year, state, app)
}

fn validate_source_path(path: &Path) -> Result<(PathBuf, String), String> {
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "Не удалось открыть выбранный исходник «{}»: {error}",
            path.display()
        )
    })?;
    let metadata = std::fs::metadata(&canonical).map_err(|error| {
        format!(
            "Не удалось прочитать выбранный исходник «{}»: {error}",
            canonical.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Выбранный путь не является файлом: {}",
            canonical.display()
        ));
    }
    if metadata.len() > universal_intake::MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "Исходный файл слишком большой: максимум {} МБ.",
            universal_intake::MAX_SOURCE_FILE_BYTES / (1024 * 1024)
        ));
    }
    let file_name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Имя выбранного исходника не поддерживается системой.".to_string())?
        .to_string();
    Ok((canonical, file_name))
}

fn parse_source_file_bytes(
    file_name: String,
    mut bytes: Vec<u8>,
    default_year: i32,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ParseSourceFileResponse, String> {
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("intake-work");
    let mut upload_session =
        universal_intake::normalize_uploaded_bytes(&file_name, &bytes, &workspace)?;
    let normalized = upload_session.take_source()?;
    let provenance = SourceProvenance::from_bytes(&file_name, &bytes);
    let retained_source = universal_intake::RetainedUploadedSource::new(&file_name, &bytes)?;
    bytes.fill(0);
    let source_path = retained_source.virtual_path();
    let source_text = normalized.text;
    let source_kind = normalized.source_kind;
    let layout_items = normalized.layout_items;
    let (mut parsed, mut report) = parse_source_text(&source_text, default_year);
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
    let (mut retained_slot, mut provenance_slot) = lock_source_session_state(&state)?;
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let (routing, bundle_decision) = resolve_document_bundle_for_case(
            &app,
            &source_text,
            &semantic_case,
            &snapshot.pack,
            None,
            &[],
        )?;
        Ok((
            ParseSourceFileResponse {
                source_text,
                source_path,
                source_kind,
                layout_items,
                semantic_case,
                report,
                routing,
                bundle_decision,
            },
            true,
        ))
    })?;
    drop(upload_session);
    *retained_slot = Some(retained_source);
    *provenance_slot = Some(provenance);
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
    bundle_decision: BundleDecision,
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
    let (mut retained, mut source_provenance) = lock_source_session_state(&state)?;
    let response = transact_default_state(&app, &state, |snapshot| {
        replace_case_from_new_source(&mut snapshot.semantic_case, parsed);
        let semantic_case = snapshot.semantic_case.clone();
        let (routing, bundle_decision) = resolve_document_bundle_for_case(
            &app,
            &fetched.source_text,
            &semantic_case,
            &snapshot.pack,
            None,
            &[],
        )?;
        Ok((
            ParseWebSourceResponse {
                source_text: fetched.source_text,
                final_url: fetched.final_url,
                content_type: fetched.content_type,
                semantic_case,
                report,
                routing,
                bundle_decision,
            },
            true,
        ))
    })?;
    retained.take();
    *source_provenance = Some(provenance);
    Ok(response)
}

#[cfg(test)]
mod source_intake_block_retention_tests {
    use super::replace_case_from_new_source;
    use dokkomplekt_core::SemanticCase;

    #[test]
    fn new_source_drops_every_block_from_previous_case() {
        let mut current = SemanticCase::default();
        current
            .blocks
            .insert("professional.medical.diary.regular.f200".into(), String::new());
        current
            .blocks
            .insert("source.kind".into(), "old-docx".into());
        current
            .blocks
            .insert("medical.diary.final_text".into(), "old-patient-local".into());
        let mut parsed = SemanticCase::default();
        parsed
            .blocks
            .insert("source.kind".into(), "new-docx".into());

        replace_case_from_new_source(&mut current, parsed);

        assert_eq!(current.blocks.len(), 1);
        assert_eq!(
            current.blocks.get("source.kind").map(String::as_str),
            Some("new-docx")
        );
        assert!(!current
            .blocks
            .contains_key("professional.medical.diary.regular.f200"));
        assert!(!current.blocks.contains_key("medical.diary.final_text"));
    }
}
