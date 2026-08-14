from pathlib import Path

path = Path("src-tauri/src/subsystems/document_commands.rs")
text = path.read_text(encoding="utf-8")

helper_anchor = '''#[derive(Debug, Deserialize)]
struct RenderDocxRequest {
'''
helper = '''fn ensure_rendered_document_complete(
    document: &DocumentTemplateSpec,
    template_text: &str,
    semantic_case: &SemanticCase,
    rendered_path: &Path,
) -> Result<(), String> {
    let missing_required = document
        .required_fields
        .iter()
        .filter(|field_id| !semantic_case.has(field_id))
        .cloned()
        .collect::<Vec<_>>();
    let rendered_text = extract_docx_text(rendered_path).map_err(|error| {
        format!(
            "Не удалось проверить полноту созданного документа «{}»: {error}",
            document.button_label
        )
    })?;
    let requirements = dokkomplekt_core::required_blocks_for(document, template_text);
    let unmet_blocks = dokkomplekt_core::unmet_blocks(
        &requirements,
        semantic_case,
        &rendered_text,
    );
    if missing_required.is_empty() && unmet_blocks.is_empty() {
        return Ok(());
    }

    let mut reasons = Vec::new();
    if !missing_required.is_empty() {
        reasons.push(format!(
            "не заполнены обязательные поля: {}",
            missing_required.join(", ")
        ));
    }
    if !unmet_blocks.is_empty() {
        reasons.push(format!(
            "в готовом документе не подтверждены обязательные блоки: {}",
            unmet_blocks.join(", ")
        ));
    }
    Err(format!(
        "Документ «{}» не опубликован: {}.",
        document.button_label,
        reasons.join("; ")
    ))
}

#[derive(Debug, Deserialize)]
struct RenderDocxRequest {
'''
if helper_anchor not in text:
    raise SystemExit("render helper anchor not found")
text = text.replace(helper_anchor, helper, 1)

single_hydration = '''        &base_case,
        &[template_text],
        true,
'''
if single_hydration not in text:
    raise SystemExit("single hydration anchor not found")
text = text.replace(
    single_hydration,
    '''        &base_case,
        std::slice::from_ref(&template_text),
        true,
''',
    1,
)

single_validation_anchor = '''    let mut result = match render_result {
        Ok(result) => result,
        Err(error) => {
            rollback_counter_reservations(&app, &hydrated.counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error.to_string());
        }
    };
    if let Err(error) = template_snapshot.ensure_current() {
'''
single_validation = '''    let mut result = match render_result {
        Ok(result) => result,
        Err(error) => {
            rollback_counter_reservations(&app, &hydrated.counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error.to_string());
        }
    };
    if let Err(error) = ensure_rendered_document_complete(
        &doc,
        &template_text,
        &hydrated.case,
        &reservation.path,
    ) {
        let _ = std::fs::remove_file(&reservation.path);
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    if let Err(error) = template_snapshot.ensure_current() {
'''
if single_validation_anchor not in text:
    raise SystemExit("single validation anchor not found")
text = text.replace(single_validation_anchor, single_validation, 1)

batch_hydration = '''                &base_case,
                &[template_text],
                true,
'''
if batch_hydration not in text:
    raise SystemExit("batch hydration anchor not found")
text = text.replace(
    batch_hydration,
    '''                &base_case,
                std::slice::from_ref(&template_text),
                true,
''',
    1,
)

batch_render_anchor = '''            if let Err(error) = render_docx_with_assets(
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
'''
batch_render = '''            if let Err(error) = render_docx_with_assets(
                &app,
                template_snapshot.path(),
                &reservation.path,
                &hydrated.case,
                req.strict,
                permit.watermark.as_deref(),
            ) {
                return Err(format!("Не создан «{}»: {error}", document.button_label));
            }
            if let Err(error) = ensure_rendered_document_complete(
                document,
                &template_text,
                &hydrated.case,
                &reservation.path,
            ) {
                let _ = std::fs::remove_file(&reservation.path);
                return Err(error);
            }
            paths.push(reservation.commit()?);
'''
if batch_render_anchor not in text:
    raise SystemExit("batch render anchor not found")
text = text.replace(batch_render_anchor, batch_render, 1)

path.write_text(text, encoding="utf-8")
