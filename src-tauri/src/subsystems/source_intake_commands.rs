// Real-source intake boundary shared by interactive desktop and automation.
// User-confirmed bundle memory is a local encrypted preference, not telemetry.
// It intentionally lives in the existing app_state owner and is keyed by the
// same structural KitRuleKey used by the routing/learning engine.
const SPECIALIST_KIT_RULES_STATE_KEY: &str = "specialist_kit_rules.v1";

fn bundle_confirmation_attention_note(
    routing: &DocumentRoutingRecommendation,
) -> &'static str {
    if routing.cluster_id == "unclassified" {
        "\nОткройте Доккомплект и подтвердите состав одной кнопкой. Тип источника пока не имеет устойчивой структурной сигнатуры, поэтому выбор применится к этому делу и не будет автоматически переноситься на другие источники.\n"
    } else {
        "\nОткройте Доккомплект и подтвердите состав одной кнопкой. После подтверждения выбор будет сохранён локально для этого структурного типа дела. Обезличенный обучающий корпус заполняется только если пользователь отдельно включил это в настройках приватности.\n"
    }
}

fn known_pack_document_ids(pack: &DocumentPack) -> BTreeSet<String> {
    pack.documents
        .iter()
        .map(|document| document.id.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn specialist_kit_rule_key(
    domain: DomainKind,
    routing: &DocumentRoutingRecommendation,
    pack: &DocumentPack,
) -> Option<KitRuleKey> {
    if routing.cluster_id.trim().is_empty() || routing.cluster_id == "unclassified" {
        return None;
    }
    Some(KitRuleKey {
        domain,
        cluster_id: routing.cluster_id.clone(),
        pack_id: (!pack.pack_id.trim().is_empty()).then(|| pack.pack_id.clone()),
    })
}

fn load_specialist_kit_decision(
    app: &tauri::AppHandle,
    key: &KitRuleKey,
    pack: &DocumentPack,
) -> Result<Option<KitLearningDecision>, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let mut rules = repo
        .load_state_value::<Vec<dokkomplekt_core::SpecialistKitRule>>(SPECIALIST_KIT_RULES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let known = known_pack_document_ids(pack);
    let decision = dokkomplekt_core::decision_for_specialist_rule(&rules, key, &known);
    if decision.is_none() && rules.iter().any(|rule| &rule.key == key) {
        // The pack changed and at least one remembered document disappeared.
        // Delete only the stale exact rule and require a fresh confirmation;
        // never silently shrink a previously approved bundle.
        rules.retain(|rule| &rule.key != key);
        repo.save_state_value(SPECIALIST_KIT_RULES_STATE_KEY, &rules)
            .map_err(|error| error.to_string())?;
    }
    Ok(decision)
}

fn updated_specialist_kit_rules(
    repo: &LocalRepository,
    key: &KitRuleKey,
    pack: &DocumentPack,
    confirmed_document_ids: &[String],
) -> Result<Vec<dokkomplekt_core::SpecialistKitRule>, String> {
    let mut rules = repo
        .load_state_value::<Vec<dokkomplekt_core::SpecialistKitRule>>(SPECIALIST_KIT_RULES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let known = known_pack_document_ids(pack);
    if !dokkomplekt_core::upsert_specialist_rule(
        &mut rules, key.clone(), confirmed_document_ids, &known,
    ) {
        return Err("Не удалось сохранить подтверждённый комплект: в нём нет существующих документов.".into());
    }
    Ok(rules)
}

fn persist_specialist_kit_rule(
    app: &tauri::AppHandle,
    key: &KitRuleKey,
    pack: &DocumentPack,
    confirmed_document_ids: &[String],
) -> Result<(), String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let rules = updated_specialist_kit_rules(&repo, key, pack, confirmed_document_ids)?;
    repo.save_state_value(SPECIALIST_KIT_RULES_STATE_KEY, &rules)
        .map_err(|error| error.to_string())
}

fn claim_bundle_exception_confirmation(
    app: &tauri::AppHandle,
    state: &AppState,
    exception_id: &str,
    requested_document_ids: &[String],
) -> Result<(Vec<String>, CaseRunRecord, CreatedDocumentsIntakeRequest), String> {
    if exception_id.trim().is_empty() {
        return Err("Не указан идентификатор исключения.".into());
    }
    let _confirmation_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let known_ids = known_pack_document_ids(&pack);
    let mut seen = BTreeSet::new();
    let selected = requested_document_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| known_ids.contains(value))
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("Не выбран ни один существующий документ комплекта.".into());
    }
    let repo = repository_for(&default_state_db_path(app)?)?;
    let exception = repo
        .list_exceptions(false)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.exception_id == exception_id)
        .ok_or_else(|| "Открытое исключение не найдено.".to_string())?;
    if exception.category != "bundle_decision" {
        return Err("Подтверждение состава доступно только для исключения Bundle Decision Engine.".into());
    }
    let details: serde_json::Value = serde_json::from_str(&exception.details_json)
        .map_err(|error| format!("Сохранённые данные подтверждения повреждены: {error}"))?;
    let cluster_id = details
        .get("cluster_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "В подтверждении отсутствует структурный тип дела.".to_string())?;
    let rule_update = if cluster_id != "unclassified" {
        let domain = details
            .get("domain")
            .cloned()
            .ok_or_else(|| "В подтверждении отсутствует профессиональная область.".to_string())
            .and_then(|value| serde_json::from_value::<DomainKind>(value)
                .map_err(|error| format!("Профессиональная область подтверждения повреждена: {error}")))?;
        let key = KitRuleKey {
            domain,
            cluster_id: cluster_id.to_string(),
            pack_id: (!pack.pack_id.trim().is_empty()).then(|| pack.pack_id.clone()),
        };
        Some(updated_specialist_kit_rules(&repo, &key, &pack, &selected)?)
    } else {
        None
    };
    let record = repo
        .list_case_runs(500)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|case| case.source_path == exception.source_path && case.status == "attention")
        .ok_or_else(|| "Не найдено остановленное дело для этого источника.".to_string())?;
    if !Path::new(&record.source_path).exists() {
        return Err("Исходный файл больше не существует в рабочей папке.".into());
    }
    let mut intake: CreatedDocumentsIntakeRequest = serde_json::from_str(&record.request_json)
        .map_err(|error| format!("Сохранённый план дела повреждён: {error}"))?;
    intake.confirmed_document_ids = selected.clone();
    let resolution = format!("Специалист подтвердил комплект: {}", selected.join(", "));
    let resolved = match rule_update.as_ref() {
        Some(rules) => repo
            .resolve_exception_and_save_state_value(
                exception_id, &resolution, SPECIALIST_KIT_RULES_STATE_KEY, rules,
            )
            .map_err(|error| error.to_string())?,
        None => repo.resolve_exception(exception_id, &resolution)
            .map_err(|error| error.to_string())?,
    };
    if !resolved {
        return Err("Исключение уже закрыто другим процессом.".into());
    }
    Ok((selected, record, intake))
}

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
    let key = specialist_kit_rule_key(domain.clone(), &routing, pack);
    if !specialist_confirmed_ids.is_empty() {
        if let Some(key) = key.as_ref() {
            persist_specialist_kit_rule(app, key, pack, specialist_confirmed_ids)?;
        }
    }

    let specialist_rule = match key.as_ref() {
        Some(key) => load_specialist_kit_decision(app, key, pack)?,
        None => None,
    };
    let learned = if specialist_rule.is_some() {
        specialist_rule
    } else if let Some(key) = key.as_ref() {
        let corpus_entries = repository_for(&default_state_db_path(app)?)?
            .list_corpus_entries(10_000)
            .map_err(|error| error.to_string())?;
        decision_for_key(&corpus_entries, key, KitPromotionPolicy::default())
    } else {
        None
    };
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
struct PickComponentBundleRequest {
    #[serde(default)]
    initial_path: Option<String>,
}

#[tauri::command]
async fn pick_component_bundle(
    req: PickComponentBundleRequest,
) -> Result<Option<PickedSourceFileResponse>, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        pick_component_bundle_file_blocking(req.initial_path)
    })
    .await
    .map_err(|error| format!("Не удалось открыть выбор офлайн-комплекта: {error}"))??;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let absolute = if selected.is_absolute() {
        selected
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(selected)
    };
    let metadata = std::fs::symlink_metadata(&absolute).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Офлайн-комплект должен быть обычным ZIP-файлом, а не ссылкой".into());
    }
    if absolute.extension().and_then(|value| value.to_str()).map(|value| value.eq_ignore_ascii_case("zip")) != Some(true) {
        return Err("Офлайн-комплект Dokkomplekt должен быть ZIP-файлом".into());
    }
    let file_name = absolute
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Имя офлайн-комплекта не поддерживается системой".to_string())?
        .to_string();
    Ok(Some(PickedSourceFileResponse {
        file_name,
        selected_path: absolute.display().to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct ImportComponentBundleRequest {
    selected_path: String,
}

#[tauri::command]
async fn import_component_bundle(
    app: tauri::AppHandle,
    req: ImportComponentBundleRequest,
) -> Result<component_manager::OfflineComponentImportResult, String> {
    let path = PathBuf::from(req.selected_path);
    tauri::async_runtime::spawn_blocking(move || {
        component_manager::import_offline_component_bundle(&app, &path)
    })
    .await
    .map_err(|error| format!("Импорт офлайн-комплекта завершился ошибкой: {error}"))?
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
