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
    bundle_decision: BundleDecision,
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
