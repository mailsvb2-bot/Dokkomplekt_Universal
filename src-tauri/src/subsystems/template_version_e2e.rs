use serde_json::json;

fn run_template_version_installer_e2e(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let Some(request_raw) = std::env::var_os("DOKKOMPLEKT_E2E_TEMPLATE_VERSION_REQUEST") else {
        return Ok(false);
    };
    if std::env::var("DOKKOMPLEKT_RUN_INSTALLER_E2E").ok().as_deref() != Some("1") {
        return Err(
            "Template-version E2E fixture requires DOKKOMPLEKT_RUN_INSTALLER_E2E=1".into(),
        );
    }

    let request_path = PathBuf::from(request_raw);
    if !request_path.is_absolute() {
        return Err("Template-version E2E request path must be absolute.".into());
    }
    let evidence_path = std::env::var_os("DOKKOMPLEKT_E2E_EVIDENCE_PATH")
        .map(PathBuf::from)
        .ok_or_else(|| "Template-version E2E fixture requires DOKKOMPLEKT_E2E_EVIDENCE_PATH".to_string())?;
    if !evidence_path.is_absolute() {
        return Err("Template-version E2E evidence path must be absolute.".into());
    }

    let request_bytes = std::fs::read(&request_path)
        .map_err(|error| format!("Failed to read template-version E2E request: {error}"))?;
    let request: serde_json::Value = serde_json::from_slice(&request_bytes)
        .map_err(|error| format!("Invalid template-version E2E request JSON: {error}"))?;
    let button_label = request
        .get("button_label")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "template-version fixture requires button_label".to_string())?;
    let template_path = request
        .get("template_path")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "template-version fixture requires template_path".to_string())?;

    let document_id = {
        let pack = state.pack.lock().map_err(|_| "state lock failed".to_string())?;
        pack.documents
            .iter()
            .find(|document| document.button_label == button_label)
            .map(|document| document.id.clone())
            .ok_or_else(|| {
                format!("template-version fixture document not found for button: {button_label}")
            })?
    };

    let pack = update_document_template(
        UpdateDocumentTemplateRequest {
            document_id: document_id.clone(),
            template_path: template_path.to_string(),
            acknowledge_regressions: true,
            learning_validation_id: None,
        },
        app.state::<AppState>(),
        app.clone(),
    )?;
    let versions = repository_for(&default_state_db_path(app)?)?
        .list_template_versions(&document_id)
        .map_err(|error| error.to_string())?;
    if versions.len() < 2 {
        return Err("Template-version E2E fixture did not create a superseded/current pair.".into());
    }

    let payload = json!({
        "schema": "dokkomplekt.template-version-fixture.v1",
        "button_label": button_label,
        "document_id": document_id,
        "document_count": pack.documents.len(),
        "versions": versions,
    });
    if let Some(parent) = evidence_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    atomic_write_file(
        &evidence_path,
        &serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod template_version_e2e_tests {
    #[test]
    fn installer_fixture_environment_is_explicitly_named_and_opt_in() {
        let source = include_str!("template_version_e2e.rs");
        assert!(source.contains("DOKKOMPLEKT_RUN_INSTALLER_E2E"));
        assert!(source.contains("DOKKOMPLEKT_E2E_TEMPLATE_VERSION_REQUEST"));
        assert!(source.contains("DOKKOMPLEKT_E2E_EVIDENCE_PATH"));
    }
}
