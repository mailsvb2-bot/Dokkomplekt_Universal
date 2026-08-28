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
