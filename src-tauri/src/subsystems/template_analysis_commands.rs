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
    extracted_text: String,
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
        extracted_text: template_text.to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct PrepareTemplatesRequest {
    candidates: Vec<TemplateCandidate>,
}

#[tauri::command]
fn prepare_template_setup(
    req: PrepareTemplatesRequest,
    state: State<'_, AppState>,
) -> Result<Vec<TemplateConfirmationRow>, String> {
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    Ok(prepare_template_confirmations_with_existing_pack(
        &req.candidates,
        Some(&pack),
    ))
}
