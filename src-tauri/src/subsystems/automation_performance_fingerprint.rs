fn automation_plan_fingerprint(
    app: &tauri::AppHandle,
    pack: &DocumentPack,
    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,
    req: &CreatedDocumentsIntakeRequest,
) -> Result<String, String> {
    let mut documents = pack.documents.clone();
    documents.sort_by(|left, right| left.id.cmp(&right.id));
    let mut templates = Vec::with_capacity(documents.len());
    for document in documents {
        let snapshot = template_snapshots.get(&document.id).ok_or_else(|| {
            format!("Не найден snapshot шаблона «{}».", document.button_label)
        })?;
        templates.push(serde_json::json!({
            "document": document,
            "template_sha256": snapshot.sha256(),
        }));
    }
    let model_config = load_semantic_model_config(app)?;
    let semantic_runtime_files = ["llama_cpp", "semantic_model"]
        .into_iter()
        .filter_map(|tool| {
            let path = universal_intake::resolve_tool(tool);
            path.is_file().then(|| {
                file_content_signature(&path)
                    .map(|(_, _, sha256)| serde_json::json!({"tool": tool, "sha256": sha256}))
                    .unwrap_or_else(|_| serde_json::json!({"tool": tool, "sha256": "unreadable"}))
            })
        })
        .collect::<Vec<_>>();
    let app_data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let calendar_path = reference_data_update::cached_package_path(&app_data_dir);
    let calendar_fingerprint = if calendar_path.is_file() {
        file_content_signature(&calendar_path)
            .map(|(_, _, sha256)| sha256)
            .unwrap_or_else(|_| "cached-calendar-unreadable".into())
    } else {
        format!("bundled-calendar:{}", env!("CARGO_PKG_VERSION"))
    };
    let payload = serde_json::json!({
        "schema": 3,
        "template_admission_contract": "validated-automatic-v1",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "templates": templates,
        "folder_parts": req.folder_parts.clone(),
        "default_year": req.default_year,
        "sick_leave_enabled": req.sick_leave_enabled,
        "semantic_model": model_config,
        "semantic_runtime_files": semantic_runtime_files,
        "calendar": calendar_fingerprint,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
