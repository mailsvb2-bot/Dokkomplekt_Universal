/// Immutable template material used by one entire mail-merge operation.
///
/// Text extraction and DOCX rendering both read the same snapshot. The live
/// template is consulted again only at publication and commercial-commit boundaries.
struct MailMergeTemplateSnapshot {
    button_label: String,
    snapshot: template_snapshot::TemplateSnapshot,
    text: String,
}

fn capture_mail_merge_template_snapshot(
    app: &tauri::AppHandle,
    button_label: &str,
    configured_path: &str,
) -> Result<MailMergeTemplateSnapshot, String> {
    let snapshot =
        template_snapshot::TemplateSnapshot::capture(app, configured_path, button_label)?;
    let text = extract_docx_text(snapshot.path()).map_err(|error| {
        format!(
            "Не удалось прочитать стабилизированный шаблон «{button_label}»: {error}"
        )
    })?;
    Ok(MailMergeTemplateSnapshot {
        button_label: button_label.to_string(),
        snapshot,
        text,
    })
}

fn ensure_mail_merge_templates_current(
    templates: &[MailMergeTemplateSnapshot],
) -> Result<(), String> {
    for template in templates {
        template.snapshot.ensure_current()?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RenderMailMergeRequest {
    document_ids: Vec<String>,
    delimited_text: String,
    output_root: String,
    strict: bool,
}
#[derive(Debug, Serialize)]
struct RenderMailMergeResponse {
    output_folder: String,
    row_count: usize,
    created_files: Vec<String>,
    warnings: Vec<String>,
}
#[tauri::command]
fn render_mail_merge(
    req: RenderMailMergeRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RenderMailMergeResponse, String> {
    require_strict_document_publication(req.strict)?;
    let table = parse_delimited_table(&req.delimited_text)?;
    if table.rows.is_empty() {
        return Err("В таблице нет строк данных.".into());
    }
    let mut ids = req
        .document_ids
        .into_iter()
        .filter(|x| !x.trim().is_empty())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err("Не выбраны документы.".into());
    }
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let documents = ids
        .iter()
        .map(|id| {
            pack.documents
                .iter()
                .find(|d| &d.id == id)
                .cloned()
                .ok_or_else(|| format!("Документ не найден: {id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let template_inputs = documents
        .iter()
        .map(|document| {
            capture_mail_merge_template_snapshot(
                &app,
                &document.button_label,
                &document.template_path,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = template_inputs
        .len()
        .checked_mul(table.rows.len())
        .ok_or("Слишком большой пакет")?;
    let requested_documents = count.try_into().map_err(|_| "Слишком большой пакет")?;
    let root = resolve_user_visible_absolute_path(&req.output_root, "Папка готовых документов")?;
    ensure_output_root_path(&root)?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    cleanup_stale_stage_directories(&root, Duration::from_secs(24 * 60 * 60))?;
    let stage = root.join(format!(".mail-merge-stage-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let permit = match reserve_generation_access(&app, &state, requested_documents) {
        Ok(permit) => permit,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    let base = match state.semantic_case.lock() {
        Ok(case) => case.clone(),
        Err(_) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_generation_access(&app, &state, &permit);
            return Err("state lock failed".into());
        }
    };
    let mut counter_reservations = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
        let mut files = Vec::new();
        for row_index in 0..table.rows.len() {
            let row_dir = stage.join(format!("{:04}", row_index + 1));
            std::fs::create_dir_all(&row_dir).map_err(|e| e.to_string())?;
            let row_case = case_for_mail_merge_row(&base, &table, row_index)?;
            for (template_index, template) in template_inputs.iter().enumerate() {
                let document = documents
                    .get(template_index)
                    .ok_or_else(|| "Внутренняя ошибка соответствия mail-merge шаблона документу.".to_string())?;
                let template_path = template.snapshot.path();
                let hydrated = hydrate_case_with_persistent_template_data(
                    &app,
                    &row_case,
                    std::slice::from_ref(&template.text),
                    true,
                )?;
                counter_reservations.extend(hydrated.counter_reservations);
                let render_case = dokkomplekt_core::domains::case_for_document_render(
                    &hydrated.case,
                    &document.category,
                    &document.role_id,
                );
                let ext = template_path
                    .extension()
                    .and_then(|x| x.to_str())
                    .filter(|x| x.eq_ignore_ascii_case("docm"))
                    .unwrap_or("docx");
                let out = row_dir.join(format!(
                    "{}.{}",
                    sanitize_path_component(&template.button_label),
                    ext
                ));
                let proof = render_docx_with_assets(
                    &app,
                    template_path,
                    &out,
                    &render_case,
                    req.strict,
                    permit.watermark.as_deref(),
                )
                .map_err(|e| {
                    format!(
                        "Строка {} / {}: {e}",
                        row_index + 1,
                        template.button_label
                    )
                })?;
                ensure_rendered_document_complete(
                    document,
                    &template.text,
                    &render_case,
                    &proof.visible_text,
                    &out,
                )
                .map_err(|error| {
                    format!(
                        "Строка {} / {}: {error}",
                        row_index + 1,
                        template.button_label
                    )
                })?;
                files.push(out);
            }
        }
        Ok(files)
    })();
    let files = match rendered {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(e);
        }
    };
    if let Err(error) = ensure_mail_merge_templates_current(&template_inputs) {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let desired = root.join(format!(
        "Пакетная генерация {}",
        OffsetDateTime::now_utc().date()
    ));
    if let Err(error) =
        generation_publication::prepare_publication(&app, &permit, &stage, &counter_reservations, None)
    {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let published = match publish_stage_to_unique_directory(&stage, &desired) {
        Ok(v) => v,
        Err(e) => {
            let journal_cleanup =
                generation_publication::abort_prepared_publication(&app, &permit);
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            if journal_cleanup.is_ok() {
                rollback_generation_access(&app, &state, &permit);
                return Err(e);
            }
            return Err(format!(
                "{e}; pre-publication квитанцию удалить не удалось, поэтому резервация лимита сохранена для безопасного восстановления: {}",
                journal_cleanup.err().unwrap_or_else(|| "unknown journal cleanup error".into())
            ));
        }
    };
    let mut warnings = match generation_publication::confirm_publication(
        &app,
        &permit,
        &published,
    ) {
        Ok(warnings) => warnings,
        Err(error) => {
            return Err(recover_unverified_batch_publication(
                &app,
                &permit,
                &published,
                None,
                error,
                false,
            ));
        }
    };
    if let Err(error) = ensure_mail_merge_templates_current(&template_inputs) {
        warnings.push(format!(
            "Пакет уже опубликован, но один из шаблонов изменился сразу после границы публикации: {error}"
        ));
        let _ = append_audit_event(
            &app,
            "published_mail_merge_templates_changed_after_boundary",
            "",
            &serde_json::json!({ "error": error }),
        );
    }
    warnings.extend(generation_publication::finalize_published_generation(
        &app, &permit, false,
    ));
    let created_files = files
        .iter()
        .filter_map(|p| p.strip_prefix(&stage).ok())
        .map(|r| published.join(r).display().to_string())
        .collect();
    Ok(RenderMailMergeResponse {
        output_folder: published.display().to_string(),
        row_count: table.rows.len(),
        created_files,
        warnings,
    })
}
