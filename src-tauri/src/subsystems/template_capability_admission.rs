fn admit_template_capabilities_for_publication(
    app: &tauri::AppHandle,
    snapshot: &mut template_snapshot::TemplateSnapshot,
    document_id: &str,
    button_label: &str,
    learning_validation_id: Option<&str>,
) -> Result<(), String> {
    let mut manifest = inspect_docx_capabilities_file(snapshot.path()).map_err(|error| {
        format!("Не удалось проверить capabilities шаблона «{button_label}»: {error}")
    })?;
    let custom_xml_only = manifest.blocking_issues.len() == 1
        && manifest.blocking_issues[0] == "custom_xml_requires_explicit_sanitization_policy";
    let mut sanitization = None;

    // Learning validation is an exact-byte proof. Never silently replace those bytes.
    // For ordinary imported templates we may derive a publication copy only when
    // Custom XML is the sole blocker; comments, revisions, dataBinding and active
    // content remain fail-closed and require an explicit user-edited source.
    if !manifest.publishable() && custom_xml_only && learning_validation_id.is_none() {
        let proof = snapshot.sanitize_hidden_custom_xml_for_publication(app)?;
        manifest = inspect_docx_capabilities_file(snapshot.publication_path()).map_err(|error| {
            format!(
                "Не удалось повторно проверить очищенную публикационную копию шаблона «{button_label}»: {error}"
            )
        })?;
        sanitization = Some(proof);
    }

    if !manifest.publishable() {
        let blockers = manifest.publication_blocking_issues();
        return Err(format!(
            "Шаблон «{button_label}» пока нельзя опубликовать: обязательные проверки или ограничения не выполнены: {}. Для page-sensitive layout требуется подтверждённый runtime layout check; скрытые/активные конструкции необходимо удалить либо явно санитизировать.",
            blockers.join(", ")
        ));
    }

    append_audit_event(
        app,
        "template_capability_admission_passed",
        snapshot.sha256(),
        &serde_json::json!({
            "document_id": document_id,
            "button_label": button_label,
            "source_template_sha256": snapshot.sha256(),
            "publication_template_sha256": snapshot.publication_sha256(),
            "manifest": manifest,
            "hidden_custom_xml_sanitization": sanitization,
        }),
    )?;
    Ok(())
}
