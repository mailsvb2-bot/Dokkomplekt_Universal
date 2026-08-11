/// The zero-touch orchestrator shared by the UI command and the background
/// watcher thread: one dropped primary DOCX -> the whole configured set into a
/// fresh output folder, or a safe attention note. Decision logic lives in
/// dokkomplekt_core; this function only does IO.
fn file_content_signature(path: &Path) -> Result<(u64, u128, String), String> {
    use std::io::Read as _;
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok((
        metadata.len(),
        modified_unix_ms,
        hex::encode(hasher.finalize()),
    ))
}

fn finalize_processed_source(
    source: &Path,
    source_sha256: &str,
    privacy: &PrivacyPreferences,
    preserve_source_after_success: bool,
) -> Result<serde_json::Value, String> {
    if !universal_intake::current_source_matches(source, source_sha256)? {
        return Ok(serde_json::json!({
            "action": "source_changed_or_missing_after_publication_preserved",
            "expected_source_sha256": source_sha256,
        }));
    }
    let marker = workspace_hygiene::processed_marker_path(source);
    if preserve_source_after_success {
        for candidate in workspace_hygiene::processed_marker_candidates(source) {
            let _ = std::fs::remove_file(candidate);
        }
        return Ok(serde_json::json!({
            "action": "source_preserved_for_reissue",
            "source_sha256": source_sha256,
            "marker_removed": true,
        }));
    }
    if privacy.archive_processed_sources {
        let archived = workspace_hygiene::archive_processed_source(
            source,
            source_sha256,
            &privacy.retention_policy(),
        )?;
        return serde_json::to_value(archived).map_err(|error| error.to_string());
    }

    if privacy.copy_source_to_output {
        match workspace_hygiene::delete_processed_source_if_matches(source, source_sha256) {
            Ok(()) => {
                let _ = std::fs::remove_file(&marker);
                Ok(serde_json::json!({
                    "action": "source_deleted_after_copy",
                    "marker_removed": true,
                }))
            }
            Err(error) => {
                std::fs::write(
                    &marker,
                    format!(
                        "sha256={source_sha256}\nstatus=published_source_delete_delayed\nerror={error}\n"
                    ),
                )
                .map_err(|marker_error| {
                    format!(
                        "Комплект создан, но исходник не удалён ({error}) и маркер не записан ({marker_error})."
                    )
                })?;
                Ok(serde_json::json!({
                    "action": "source_delete_delayed",
                    "marker": marker.display().to_string(),
                    "error": error,
                }))
            }
        }
    } else {
        // A completed case is already stored by SHA-256 in the encrypted SQLite
        // case history. Do not leave an adjacent marker next to a deliberately
        // retained source: it would permanently clutter the working folder.
        for candidate in workspace_hygiene::processed_marker_candidates(source) {
            let _ = std::fs::remove_file(candidate);
        }
        Ok(serde_json::json!({
            "action": "source_retained_and_tracked_in_case_history",
            "source_sha256": source_sha256,
            "marker_removed": true,
        }))
    }
}


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
        "schema": 2,
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

fn processing_job_key(source_sha256: &str, processing_fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"dokkomplekt-processing-job-v2\0");
    hasher.update(source_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(processing_fingerprint.as_bytes());
    hex::encode(hasher.finalize())
}


fn local_completion_receipt(app_data: &Path, processing_job_sha256: &str) -> PathBuf {
    app_data
        .join("intake-completion-receipts")
        .join(format!("{processing_job_sha256}.done"))
}

fn local_completion_receipt_matches(
    app_data: &Path,
    processing_job_sha256: &str,
    source_sha256: &str,
    processing_fingerprint: &str,
) -> bool {
    let Ok(body) = std::fs::read_to_string(local_completion_receipt(
        app_data,
        processing_job_sha256,
    )) else {
        return false;
    };
    let required = [
        format!("processing_job_sha256={processing_job_sha256}"),
        format!("source_sha256={source_sha256}"),
        format!("processing_fingerprint={processing_fingerprint}"),
    ];
    required
        .iter()
        .all(|expected| body.lines().any(|line| line.trim() == expected))
}

fn plan_bound_emergency_completion_exists(
    source: &Path,
    processing_job_sha256: &str,
) -> bool {
    workspace_hygiene::processed_marker_candidates(source)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|body| {
            body.lines().any(|line| {
                line.trim() == format!("processing_job_sha256={processing_job_sha256}")
            }) && body
                .lines()
                .any(|line| line.trim() == "status=published_completion_ledgers_failed")
        })
}

fn mark_local_completion(
    app_data: &Path,
    processing_job_sha256: &str,
    source_sha256: &str,
    processing_fingerprint: &str,
) -> Result<PathBuf, String> {
    let final_path = local_completion_receipt(app_data, processing_job_sha256);
    let payload = format!(
        "schema=1\nprocessing_job_sha256={processing_job_sha256}\nsource_sha256={source_sha256}\nprocessing_fingerprint={processing_fingerprint}\ncompleted_unix={}\nhost={}\n",
        unix_now_seconds(),
        processing_lock_host_id(),
    );
    atomic_write_file(&final_path, payload.as_bytes()).map_err(|error| {
        format!("Не удалось записать локальную квитанцию завершённого дела: {error}")
    })?;
    Ok(final_path)
}

fn perform_created_documents_intake(
    state: &AppState,
    app: &tauri::AppHandle,
    req: CreatedDocumentsIntakeRequest,
) -> Result<CreatedDocumentsIntakeResponse, String> {
    let intake_started = std::time::Instant::now();
    let source = resolve_user_path(app, &req.source_path)?;
    let privacy = load_privacy_preferences(app)?;
    let app_data = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let workspace = app_data.join("intake-work");
    let source_snapshot = universal_intake::capture_stable_source(&source, &workspace)?;
    let source_size = source_snapshot.size_bytes();
    let source_modified_ms = source_snapshot.modified_unix_ms();
    let source_sha256 = source_snapshot.sha256().to_string();
    let processed_markers = workspace_hygiene::processed_marker_candidates(&source);
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let template_snapshots = pack
        .documents
        .iter()
        .map(|document| {
            template_snapshot::TemplateSnapshot::capture(
                app,
                &document.template_path,
                &document.button_label,
            )
            .map(|snapshot| (document.id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let processing_fingerprint =
        automation_plan_fingerprint(app, &pack, &template_snapshots, &req)?;
    let processing_job_sha256 = processing_job_key(&source_sha256, &processing_fingerprint);
    let completed_in_history = repository_for(&default_state_db_path(app)?)?
        .completed_case_exists_for_source_and_plan(&source_sha256, &processing_fingerprint)
        .map_err(|error| error.to_string())?;
    let completed_in_shared_queue =
        shared_completion_receipt(&source, &processing_job_sha256).is_file();
    let completed_in_local_receipts = local_completion_receipt_matches(
        &app_data,
        &processing_job_sha256,
        &source_sha256,
        &processing_fingerprint,
    );
    let completed_in_emergency_marker =
        plan_bound_emergency_completion_exists(&source, &processing_job_sha256);
    // Legacy adjacent markers remain non-authoritative. Only the explicit
    // plan-bound emergency publication marker can suppress a duplicate retry.
    let processed_is_current = !req.force_reissue
        && (completed_in_history
            || completed_in_shared_queue
            || completed_in_local_receipts
            || completed_in_emergency_marker);
    if processed_is_current {
        return Ok(CreatedDocumentsIntakeResponse {
            status: "ignored".into(),
            patient_folder: None,
            created_files: Vec::new(),
            created_documents: Vec::new(),
            missing: Vec::new(),
            attention_file: None,
            print_triage: None,
            message: "Эта версия источника уже была обработана; повторный запуск предотвращён."
                .into(),
        });
    }
    for marker in &processed_markers {
        if marker.exists() {
            let _ = std::fs::remove_file(marker);
        }
    }
    let mut central_queue_lease = match central_queue::CentralQueueLease::acquire_from_env(
        &processing_job_sha256,
        req.force_reissue,
    )? {
        central_queue::QueueAcquireResult::Disabled => None,
        central_queue::QueueAcquireResult::Acquired(lease) => Some(lease),
        central_queue::QueueAcquireResult::Busy => {
            return Ok(CreatedDocumentsIntakeResponse {
                status: "ignored".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: None,
                print_triage: None,
                message: "Источник уже обрабатывается другим компьютером центральной очереди.".into(),
            });
        }
        central_queue::QueueAcquireResult::Completed => {
            return Ok(CreatedDocumentsIntakeResponse {
                status: "ignored".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: None,
                print_triage: None,
                message: "Эта версия источника уже завершена центральной очередью.".into(),
            });
        }
    };
    let processing_guard = if central_queue_lease.is_some() {
        None
    } else {
        match ProcessingGuard::acquire(&source, &processing_job_sha256)? {
            Some(guard) => Some(guard),
            None => {
                return Ok(CreatedDocumentsIntakeResponse {
                    status: "ignored".into(),
                    patient_folder: None,
                    created_files: Vec::new(),
                    created_documents: Vec::new(),
                    missing: Vec::new(),
                    attention_file: None,
                    print_triage: None,
            message: "Источник уже обрабатывается другим экземпляром программы.".into(),
                });
            }
        }
    };

    // Process-once: ignore temp office files, unsupported packages and duplicate FS events.
    // A specialist-confirmed retry bypasses only the short-lived event deduplicator.
    // Explicit reissue also bypasses completed-history/event dedup, while the
    // atomic processing lock remains in force and the previous output is retained.
    if req.confirmed_fields.is_empty() && req.confirmed_document_ids.is_empty() && !req.force_reissue {
        let mut dedup = state.intake_dedup.lock().map_err(|_| "state lock failed")?;
        let event = dokkomplekt_core::IntakeEvent {
            path: source.clone(),
            size_bytes: source_size,
            modified_unix_ms: source_modified_ms,
            content_sha256: Some(source_sha256.clone()),
        };
        if dedup.decide_event(&event, std::time::SystemTime::now()) != IntakeDecision::Accept {
            return Ok(CreatedDocumentsIntakeResponse {
                status: "ignored".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: None,
                print_triage: None,
                message: "Событие пропущено (дубликат, временный или неподдерживаемый файл)."
                    .into(),
            });
        }
    }

    let mut case_run = CaseRunTracker::start(app, &source_sha256, &processing_fingerprint, &source, &req)?;
    case_run.transition("normalizing")?;
    if let Some(lease) = central_queue_lease.as_mut() {
        lease.renew()?;
    }

    // Each dropped source is an independent case. Every accepted format is first
    // normalized from the immutable private snapshot, never from a live file that
    // Word, a scanner or a sync client may still be replacing underneath us.
    let normalized = universal_intake::normalize_path(source_snapshot.path(), &workspace, 0)?;
    case_run.transition("recognizing")?;
    if let Some(lease) = central_queue_lease.as_mut() {
        lease.renew()?;
    }
    let source_text = normalized.text;
    let source_kind = normalized.source_kind;
    let layout_items = normalized.layout_items;
    let (mut case, mut source_report) = parse_source_text(&source_text, req.default_year);
    source_report.warnings.extend(normalized.warnings);

    let compound_fragments = universal_intake::compound_source_fragments(
        &source_kind,
        &source_text,
        &layout_items,
    );
    if compound_fragments.len() > 1 {
        let parsed_fragments = compound_fragments
            .iter()
            .map(|fragment| {
                let (mut semantic_case, _) = parse_source_text(&fragment.text, req.default_year);
                universal_intake::apply_layout_to_case(
                    &source_kind,
                    &fragment.layout_items,
                    &mut semantic_case,
                );
                universal_intake::attach_layout_evidence(
                    &fragment.layout_items,
                    &mut semantic_case,
                );
                CaseFragment {
                    source_reference: fragment.source_reference.clone(),
                    text: fragment.text.clone(),
                    semantic_case,
                }
            })
            .collect::<Vec<_>>();
        let segmentation = segment_case_fragments(&parsed_fragments);
        append_audit_event(
            app,
            "case_segmentation_evaluated",
            &source_sha256,
            &serde_json::to_value(&segmentation).map_err(|error| error.to_string())?,
        )?;
        if !segmentation.zero_touch_allowed {
            let stem = source
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("source");
            let report_path = source.with_file_name(attention_file_name(stem));
            let mut attention = String::from(
                "КОМПЛЕКТ НЕ СОЗДАН: источник содержит несколько дел или неоднозначные вложения\n\n",
            );
            for reason in &segmentation.reasons {
                attention.push_str(&format!("- {reason}\n"));
            }
            for segment in &segmentation.segments {
                attention.push_str(&format!(
                    "\n{}: {}\n",
                    segment.segment_id,
                    segment.source_references.join(", ")
                ));
            }
            attention.push_str(
                "\nРазделите дела либо один раз укажите, какие вложения относятся к каждому делу. Значения из разных людей или организаций не были объединены.\n",
            );
            std::fs::write(
                &report_path,
                note_with_source_fingerprint(
                    &attention,
                    &source_sha256,
                    None,
                    None,
                    std::time::SystemTime::now(),
                ),
            )
            .map_err(|error| error.to_string())?;
            let details = serde_json::json!({
                "segmentation": &segmentation,
                "attention_file": report_path.display().to_string(),
            });
            create_automation_exception(
                app,
                "case_segmentation",
                &source.display().to_string(),
                "Автоматическая генерация остановлена: в контейнере обнаружено несколько независимых дел.",
                &details,
            )?;
            increment_metric(app, "case_segmentation_blocks", 1);
            case_run.finish(
                "attention",
                None,
                &[],
                &[],
                Some("Case Segmentation Engine запретил смешивание независимых дел."),
            )?;
            return Ok(CreatedDocumentsIntakeResponse {
                status: "attention".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: Some(report_path.display().to_string()),
                print_triage: None,
                message: "Обнаружено несколько независимых дел; автоматическое смешивание данных запрещено.".into(),
            });
        }
        if let Some(segment) = segmentation.segments.into_iter().next() {
            case = segment.semantic_case;
            case.blocks.insert(
                "source.segment_count".into(),
                "1".into(),
            );
            case.blocks.insert(
                "source.segment_references".into(),
                segment.source_references.join(" | "),
            );
            source_report.warnings.push(
                "Case Segmentation Engine проверил вложения: все относятся к одному делу."
                    .into(),
            );
        }
    }
    universal_intake::apply_layout_to_case(&source_kind, &layout_items, &mut case);
    let _ = apply_learned_scanner_rules(app, &source_text, &mut case)?;
    universal_intake::attach_layout_evidence(&layout_items, &mut case);
    let model_domain = case
        .active_domains
        .first()
        .cloned()
        .unwrap_or(DomainKind::Generic);
    let deterministic_case_for_corpus = case.clone();
    let mut model_case_for_corpus = SemanticCase::default();
    let configured_model = load_semantic_model_config(app)?;
    let corpus_recording_enabled = configured_model.corpus_recording_enabled;
    if let Some(model_output) = &req.model_output {
        source_report
            .warnings
            .extend(apply_model_output_with_source(
                &mut case,
                model_output,
                &source_text,
                req.default_year,
            ));
        model_case_for_corpus = case.clone();
    } else {
        let model_config = semantic_runtime::effective_config(
            &state.semantic_runtime,
            &configured_model,
        )?;
        if model_config.enabled
            && (model_config.shadow_mode || model_config.auto_apply_zero_touch)
        {
            match LocalSemanticModelTransport::new(&model_config)
                .and_then(|transport| transport.complete_many(&build_extraction_prompt_for_domain_and_language(
                    &source_text,
                    &model_domain,
                    &model_config.preferred_language,
                )))
            {
                Ok(model_outputs) => {
                    let deterministic_case = case.clone();
                    if model_config.shadow_mode {
                        let mut shadow_case = case.clone();
                        let warnings = apply_model_consensus_with_source(
                            &mut shadow_case,
                            &model_outputs,
                            &source_text,
                            req.default_year,
                        );
                        universal_intake::attach_layout_evidence(&layout_items, &mut shadow_case);
                        let rejected = warnings
                            .iter()
                            .filter(|warning| {
                                warning.contains("отклонено")
                                    || warning.contains("не подтверждает")
                                    || warning.contains("self-consistency")
                            })
                            .count() as u64;
                        if rejected > 0 {
                            increment_metric(app, "model_grounding_rejections", rejected);
                        }
                        model_case_for_corpus = shadow_case.clone();
                        let proposed = shadow_case
                            .values
                            .iter()
                            .filter(|(_, value)| value.source == ValueSource::Model)
                            .count() as u64;
                        let agreements = shadow_case
                            .values
                            .iter()
                            .filter(|(field_id, value)| {
                                value.source == ValueSource::Model
                                    && deterministic_case.get(field_id)
                                        == Some(value.value.as_str())
                            })
                            .count() as u64;
                        increment_metric(app, "shadow_model_runs", 1);
                        increment_metric(app, "shadow_model_proposals", proposed);
                        increment_metric(app, "shadow_model_agreements", agreements);
                        append_audit_event(
                            app,
                            "semantic_model_shadow_evaluated",
                            &source_sha256,
                            &serde_json::json!({
                                "proposals": proposed,
                                "agreements_with_deterministic": agreements,
                                "grounding_rejections": rejected,
                            }),
                        )?;
                        source_report.warnings.push(format!(
                            "Shadow-mode SemanticModel: предложено полей {proposed}, совпало с deterministic-парсером {agreements}; результат модели не влиял на комплект."
                        ));
                        source_report.warnings.extend(warnings);
                    } else {
                        let warnings = apply_model_consensus_with_source(
                            &mut case,
                            &model_outputs,
                            &source_text,
                            req.default_year,
                        );
                        let rejected = warnings
                            .iter()
                            .filter(|warning| {
                                warning.contains("отклонено")
                                    || warning.contains("не подтверждает")
                                    || warning.contains("self-consistency")
                            })
                            .count() as u64;
                        if rejected > 0 {
                            increment_metric(app, "model_grounding_rejections", rejected);
                        }
                        model_case_for_corpus = case.clone();
                        source_report.warnings.extend(warnings);
                    }
                }
                Err(error) => source_report.warnings.push(format!(
                    "Локальная SemanticModel недоступна; deterministic/rule-парсер продолжил безопасно: {error}"
                )),
            }
        }
    }

    universal_intake::attach_layout_evidence(&layout_items, &mut case);
    if !model_case_for_corpus.values.is_empty() {
        universal_intake::attach_layout_evidence(&layout_items, &mut model_case_for_corpus);
    }

    if !req.confirmed_fields.is_empty() {
        let mut confirmed = Vec::new();
        for field_id in &req.confirmed_fields {
            if let Some(value) = case.get(field_id).map(str::to_owned) {
                set_user_value(&mut case, field_id, &value);
                confirmed.push(field_id.clone());
            }
        }
        if !confirmed.is_empty() {
            source_report.warnings.push(format!(
                "Специалист пакетно подтвердил значения полей: {}",
                confirmed.join(", ")
            ));
        }
    }
    universal_intake::attach_layout_evidence(&layout_items, &mut case);

    case_run.transition("checking")?;
    if let Some(lease) = central_queue_lease.as_mut() {
        lease.renew()?;
    }
    if pack.documents.is_empty() {
        case_run.finish(
            "attention",
            None,
            &[],
            &[],
            Some("Не настроен ни один пользовательский шаблон."),
        )?;
        return Ok(CreatedDocumentsIntakeResponse {
            status: "setup_needed".into(),
            patient_folder: None,
            created_files: Vec::new(),
            created_documents: Vec::new(),
            missing: Vec::new(),
            attention_file: None,
            print_triage: None,
            message: "Не настроен ни один документ. Сначала создайте кнопки из своих шаблонов."
                .into(),
        });
    }

    let routing_recommendation = recommend_document_bundle(&source_text, &case, &pack);
    let learned_kit_decision = {
        let corpus_entries = repository_for(&default_state_db_path(app)?)?
            .list_corpus_entries(10_000)
            .map_err(|error| error.to_string())?;
        let key = KitRuleKey {
            domain: model_domain.clone(),
            cluster_id: routing_recommendation.cluster_id.clone(),
            pack_id: (!pack.pack_id.trim().is_empty()).then(|| pack.pack_id.clone()),
        };
        decision_for_key(&corpus_entries, &key, KitPromotionPolicy::default())
    };
    let bundle_decision = decide_document_bundle(
        &pack,
        &routing_recommendation,
        learned_kit_decision.as_ref(),
        &req.confirmed_document_ids,
    );
    append_audit_event(
        app,
        "document_bundle_decided",
        &source_sha256,
        &serde_json::json!({
            "routing": &routing_recommendation,
            "learned_decision": &learned_kit_decision,
            "decision": &bundle_decision,
        }),
    )?;

    if bundle_decision.review_required {
        let report_name = attention_file_name(source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("source"));
        let report_path = source.with_file_name(&report_name);
        let question = bundle_decision
            .question
            .clone()
            .unwrap_or_else(|| "Выберите точный состав комплекта.".into());
        let mut attention = format!("КОМПЛЕКТ НЕ СОЗДАН: требуется подтвердить состав\n\n{question}\n");
        if !bundle_decision.document_ids.is_empty() {
            attention.push_str(&format!(
                "\nПредложенные документы: {}\n",
                bundle_decision.document_ids.join(", ")
            ));
        }
        attention.push_str("\nОткройте Доккомплект, подтвердите состав одной кнопкой. После подтверждения он будет записан в корпус обучения для этого типа дела.\n");
        std::fs::write(
            &report_path,
            note_with_source_fingerprint(
                &attention,
                &source_sha256,
                None,
                None,
                std::time::SystemTime::now(),
            ),
        )
        .map_err(|error| error.to_string())?;
        let details = serde_json::json!({
            "question": question,
            "proposed_document_ids": &bundle_decision.document_ids,
            "cluster_id": &routing_recommendation.cluster_id,
            "domain": &routing_recommendation.domain,
            "confidence": bundle_decision.confidence,
            "attention_file": report_path.display().to_string(),
        });
        create_automation_exception(
            app,
            "bundle_decision",
            &source.display().to_string(),
            "Автоматическая генерация остановлена: требуется подтвердить точный состав комплекта.",
            &details,
        )?;
        increment_metric(app, "bundle_reviews_required", 1);
        case_run.finish(
            "attention",
            None,
            &[],
            &[],
            Some("Bundle Decision Engine потребовал подтверждение состава."),
        )?;
        return Ok(CreatedDocumentsIntakeResponse {
            status: "attention".into(),
            patient_folder: None,
            created_files: Vec::new(),
            created_documents: Vec::new(),
            missing: Vec::new(),
            attention_file: Some(report_path.display().to_string()),
            print_triage: None,
            message: question,
        });
    }

    let selected_document_ids = bundle_decision
        .document_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    source_report.warnings.push(format!(
        "Bundle Decision Engine выбрал точный комплект: {} (источник: {:?}, уверенность {:.0}%).",
        bundle_decision.document_ids.join(", "),
        bundle_decision.source,
        bundle_decision.confidence * 100.0
    ));

    let mut configured = Vec::new();
    for doc in &pack.documents {
        if !selected_document_ids.contains(&doc.id) {
            continue;
        }
        let template_snapshot = template_snapshots
            .get(&doc.id)
            .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", doc.button_label))?;
        let template_text = extract_docx_text(template_snapshot.path())
            .map_err(|e| format!("Шаблон «{}» не читается: {e}", doc.button_label))?;
        configured.push(ConfiguredDocument {
            spec: doc.clone(),
            template_text,
        });
    }

    let flags = WorkflowFlags {
        sick_leave_enabled: req.sick_leave_enabled,
    };
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("source")
        .to_string();
    let file_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("source.docx")
        .to_string();

    let configured_template_texts: Vec<String> = configured
        .iter()
        .map(|document| document.template_text.clone())
        .collect();
    let planning_case =
        hydrate_case_with_persistent_template_data(app, &case, &configured_template_texts, false)?;
    // Readiness belongs to the final generation plan, never to templates that
    // routing or a learned kit deliberately excluded.
    let required_for_automation = configured
        .iter()
        .flat_map(|configured_document| {
            configured_document
                .spec
                .required_fields
                .iter()
                .chain(configured_document.spec.placeholders.iter())
                .cloned()
        })
        .collect::<BTreeSet<_>>();
    let quality = evaluate_automation_quality(
        &planning_case.case,
        required_for_automation.iter().map(String::as_str),
    );
    if !quality.ready {
        let missing = quality
            .blockers
            .iter()
            .map(|blocker| blocker.field_id.clone())
            .collect::<Vec<_>>();
        let report_name = attention_file_name(&stem);
        let report_path = source.with_file_name(&report_name);
        let mut attention = String::from("КОМПЛЕКТ НЕ СОЗДАН: значения требуют подтверждения\n\n");
        for blocker in &quality.blockers {
            attention.push_str(&format!(
                "- {}: уверенность {:.1}%, требуется {:.1}% ({:?})\n",
                blocker.field_id,
                blocker.confidence * 100.0,
                blocker.required_confidence * 100.0,
                blocker.risk
            ));
        }
        attention.push_str(
            "\nОткройте Доккомплект, подтвердите указанные значения и повторите генерацию.\n",
        );
        std::fs::write(
            &report_path,
            note_with_source_fingerprint(&attention, &source_sha256, None, None, std::time::SystemTime::now()),
        )
        .map_err(|error| error.to_string())?;
        let details = serde_json::to_value(&quality).map_err(|error| error.to_string())?;
        create_automation_exception(
            app,
            "risk_gate",
            &source.display().to_string(),
            "Автоматическая генерация остановлена: значения требуют подтверждения.",
            &details,
        )?;
        append_audit_event(app, "intake_blocked_risk", &source_sha256, &details)?;
        increment_metric(app, "blocked_sources", 1);
        case_run.finish(
            "attention",
            None,
            &[],
            &missing,
            Some("Risk gate потребовал подтверждение значений."),
        )?;
        return Ok(CreatedDocumentsIntakeResponse {
            status: "attention".into(),
            patient_folder: None,
            created_files: Vec::new(),
            created_documents: Vec::new(),
            missing,
            attention_file: Some(report_path.display().to_string()),
            print_triage: None,
            message: "Автоматическая генерация остановлена risk gate: сомнительные значения не попали в документы и печать.".into(),
        });
    }

    let batch = plan_created_documents_batch(
        &planning_case.case,
        &configured,
        &flags,
        &req.folder_parts,
        &stem,
        &file_name,
    );

    match batch {
        CreatedDocumentsBatch::Attention {
            missing,
            attention_file_name: report_name,
            attention_text,
            ..
        } => {
            let report_path = source.with_file_name(&report_name);
            std::fs::write(
                &report_path,
                note_with_source_fingerprint(&attention_text, &source_sha256, None, None, std::time::SystemTime::now()),
            )
            .map_err(|e| e.to_string())?;
            let details = serde_json::json!({
                "missing_fields": &missing,
                "attention_file": report_path.display().to_string(),
            });
            create_automation_exception(
                app,
                "missing_data",
                &source.display().to_string(),
                "Не хватает обязательных данных для создания комплекта.",
                &details,
            )?;
            append_audit_event(app, "intake_blocked_missing", &source_sha256, &details)?;
            increment_metric(app, "blocked_sources", 1);
            case_run.finish(
                "attention",
                None,
                &[],
                &missing,
                Some("Не хватает обязательных данных для комплекта."),
            )?;
            Ok(CreatedDocumentsIntakeResponse {
                status: "attention".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing,
                attention_file: Some(report_path.display().to_string()),
                print_triage: None,
                message: "Не хватает данных в исходном документе. Ничего не создано, источник не перемещён.".into(),
            })
        }
        CreatedDocumentsBatch::Ready {
            patient_folder_name,
            source_target_name,
            outputs,
        } => {
            case_run.transition("ready")?;
            let output_root = resolve_user_path(app, &req.output_root)?;
            std::fs::create_dir_all(&output_root).map_err(|e| e.to_string())?;
            cleanup_stale_stage_directories(&output_root, Duration::from_secs(24 * 60 * 60))?;
            let desired_patient_dir =
                output_root.join(sanitize_path_component(&patient_folder_name));
            if let Some(lease) = central_queue_lease.as_mut() {
                lease.renew()?;
            }
            let permit = reserve_generation_access(
                app,
                state,
                outputs.len().try_into().unwrap_or(u32::MAX),
            )?;
            case_run.transition("generating")?;
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default();
            let stage =
                output_root.join(format!(".dokkomplekt-stage-{}-{nonce}", std::process::id()));
            if let Err(error) = std::fs::create_dir_all(&stage) {
                rollback_generation_access(app, state, &permit);
                return Err(error.to_string());
            }

            let previous_case_documents = req
                .resume_from_case_id
                .as_deref()
                .map(|case_id| {
                    repository_for(&default_state_db_path(app)?)?
                        .list_case_documents(case_id)
                        .map_err(|error| error.to_string())
                })
                .transpose()?
                .unwrap_or_default();
            let current_case_id = case_run.case_id().to_string();
            let mut document_records = Vec::<(String, String, String, Option<String>, String, u64)>::new();
            let mut reused_documents = 0_u64;
            let mut rerendered_documents = 0_u64;
            let mut counter_reservations = Vec::new();
            let render_result = (|| -> Result<Vec<String>, String> {
                let mut names = Vec::new();
                let mut report_case = planning_case.case.clone();
                for out in &outputs {
                    let doc = pack
                        .documents
                        .iter()
                        .find(|d| d.id == out.document_id)
                        .ok_or_else(|| "document not found".to_string())?;
                    let out_path = stage.join(&out.file_name);
                    let template_snapshot = template_snapshots
                        .get(&doc.id)
                        .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", doc.button_label))?;
                    let template_text = configured
                        .iter()
                        .find(|configured| configured.spec.id == out.document_id)
                        .map(|configured| configured.template_text.clone())
                        .ok_or_else(|| "configured document not found".to_string())?;
                    let fingerprint_case = hydrate_case_with_persistent_template_data(
                        app,
                        &case,
                        std::slice::from_ref(&template_text),
                        false,
                    )?;
                    let input_fingerprint = resume_engine::document_input_fingerprint(
                        &out.document_id,
                        template_snapshot.path(),
                        &template_text,
                        &fingerprint_case.case,
                        permit.watermark.as_deref(),
                    )?;
                    let reusable = resume_engine::template_is_resume_safe(
                        &template_text,
                        &fingerprint_case.case,
                    )
                    .then(|| {
                        resume_engine::reusable_checkpoint(
                            &previous_case_documents,
                            &out.document_id,
                            &input_fingerprint,
                        )
                    })
                    .flatten();
                    let reused_from = if let Some(previous) = reusable {
                        std::fs::copy(&previous.output_path, &out_path).map_err(|error| {
                            format!("Не удалось восстановить «{}» из checkpoint: {error}", doc.button_label)
                        })?;
                        reused_documents = reused_documents.saturating_add(1);
                        Some(previous.case_id.clone())
                    } else {
                        let hydrated = hydrate_case_with_persistent_template_data(
                            app,
                            &case,
                            std::slice::from_ref(&template_text),
                            true,
                        )?;
                        for (field_id, value) in &hydrated.case.values {
                            report_case.values.insert(field_id.clone(), value.clone());
                        }
                        counter_reservations.extend(hydrated.counter_reservations);
                        render_docx_with_assets(
                            app,
                            template_snapshot.path(),
                            &out_path,
                            &hydrated.case,
                            true,
                            permit.watermark.as_deref(),
                        )
                        .map_err(|e| format!("Не создан «{}»: {e}", doc.button_label))?;
                        rerendered_documents = rerendered_documents.saturating_add(1);
                        None
                    };
                    let checkpoint = resume_engine::persist_checkpoint(
                        &out_path,
                        &app_data,
                        &current_case_id,
                        &out.file_name,
                    )?;
                    repository_for(&default_state_db_path(app)?)?
                        .upsert_case_document(&CaseDocumentRecord {
                            case_id: current_case_id.clone(),
                            document_id: out.document_id.clone(),
                            input_fingerprint: input_fingerprint.clone(),
                            output_path: checkpoint.path.display().to_string(),
                            output_sha256: checkpoint.sha256.clone(),
                            output_size_bytes: checkpoint.size_bytes,
                            status: if reused_from.is_some() { "reused" } else { "rendered" }.into(),
                            reused_from_case_id: reused_from.clone(),
                            created_at: String::new(),
                            updated_at: String::new(),
                        })
                        .map_err(|error| error.to_string())?;
                    document_records.push((
                        out.document_id.clone(),
                        out.file_name.clone(),
                        input_fingerprint,
                        reused_from,
                        checkpoint.sha256,
                        checkpoint.size_bytes,
                    ));
                    names.push(out.file_name.clone());
                }
                if privacy.copy_source_to_output {
                    std::fs::copy(source_snapshot.path(), stage.join(&source_target_name))
                        .map_err(|e| format!("Не удалось скопировать snapshot исходника в комплект: {e}"))?;
                }
                if privacy.write_trust_report {
                    write_trust_report(
                        &stage,
                        &report_case,
                        TrustReportContext {
                            source_name: &file_name,
                            source_sha256: &source_sha256,
                            generated_names: &names,
                            used_field_ids: &required_for_automation,
                            include_values: privacy.include_values_in_trust_report,
                            source_warnings: &source_report.warnings,
                        },
                    )?;
                }
                Ok(names)
            })();

            let names = match render_result {
                Ok(names) => names,
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&stage);
                    rollback_counter_reservations(app, &counter_reservations);
                    rollback_generation_access(app, state, &permit);
                    return Err(error);
                }
            };
            if let Err(error) = ensure_generation_inputs_current(
                &source,
                &source_sha256,
                &template_snapshots,
                processing_guard.as_ref(),
            ) {
                let _ = std::fs::remove_dir_all(&stage);
                rollback_counter_reservations(app, &counter_reservations);
                rollback_generation_access(app, state, &permit);
                let _ = case_run.finish("superseded", None, &[], &[], Some(&error));
                let _ = append_audit_event(
                    app,
                    "intake_source_superseded",
                    &source_sha256,
                    &serde_json::json!({ "stage": "before_publication", "error": &error }),
                );
                return Err(error);
            }
            case_run.transition("publishing")?;
            if let Some(lease) = central_queue_lease.as_mut() {
                lease.renew()?;
            }
            let patient_dir = match publish_stage_to_unique_directory(&stage, &desired_patient_dir)
            {
                Ok(path) => path,
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&stage);
                    rollback_counter_reservations(app, &counter_reservations);
                    rollback_generation_access(app, state, &permit);
                    return Err(error);
                }
            };
            // The filesystem publication is the irreversible business boundary.
            // From this point onward the output is never deleted and accounting is
            // never refunded merely because best-effort metadata finalization fails.
            case_run.mark_business_terminal();
            let mut publication_warnings = Vec::new();
            if let Err(error) = ensure_generation_inputs_current(
                &source,
                &source_sha256,
                &template_snapshots,
                processing_guard.as_ref(),
            ) {
                publication_warnings.push(format!(
                    "Комплект уже опубликован, но входные данные изменились сразу после границы публикации: {error}"
                ));
                let details = serde_json::json!({
                    "stage": "after_directory_publish",
                    "error": &error,
                });
                let _ = create_automation_exception(
                    app,
                    "published_inputs_changed_after_boundary",
                    "",
                    "Комплект опубликован, но входные данные изменились сразу после публикации; результат сохранён и требует проверки.",
                    &details,
                );
                let _ = append_audit_event(
                    app,
                    "published_inputs_changed_after_boundary",
                    &source_sha256,
                    &details,
                );
            }
            publication_warnings.extend(generation_publication::finalize_published_generation(app, &permit, &patient_dir));
            let audit_details = serde_json::json!({
                "output_folder": patient_dir.display().to_string(),
                "documents": &names,
                "source_kind": &source_kind,
                "source_copied": privacy.copy_source_to_output,
                "trust_report": privacy.write_trust_report,
                "reused_documents": reused_documents,
                "rerendered_documents": rerendered_documents,
                "resumed_from_case_id": req.resume_from_case_id,
            });
            if let Err(error) =
                append_audit_event(app, "intake_published", &source_sha256, &audit_details)
            {
                let _ = create_automation_exception(
                    app,
                    "audit_failure",
                    &source.display().to_string(),
                    "Комплект создан, но событие не удалось добавить в журнал аудита.",
                    &serde_json::json!({ "error": error }),
                );
            }
            increment_metric(app, "processed_sources", 1);
            increment_metric(app, "generated_documents", names.len() as u64);
            increment_metric(app, "reused_documents", reused_documents);
            increment_metric(app, "rerendered_documents", rerendered_documents);
            for (
                document_id,
                file_name,
                input_fingerprint,
                reused_from_case_id,
                output_sha256,
                output_size_bytes,
            ) in &document_records {
                let record = CaseDocumentRecord {
                    case_id: current_case_id.clone(),
                    document_id: document_id.clone(),
                    input_fingerprint: input_fingerprint.clone(),
                    output_path: patient_dir.join(file_name).display().to_string(),
                    output_sha256: output_sha256.clone(),
                    output_size_bytes: *output_size_bytes,
                    status: "published".into(),
                    reused_from_case_id: reused_from_case_id.clone(),
                    created_at: String::new(),
                    updated_at: String::new(),
                };
                let persist_result = default_state_db_path(app)
                    .and_then(|path| repository_for(&path))
                    .and_then(|repo| repo.upsert_case_document(&record).map_err(|error| error.to_string()));
                if let Err(error) = persist_result {
                    let _ = create_automation_exception(
                        app,
                        "resume_checkpoint_metadata",
                        &source.display().to_string(),
                        "Комплект создан, но метаданные одного документа для будущего resume не сохранены.",
                        &serde_json::json!({ "document_id": document_id, "error": error }),
                    );
                }
            }
            resume_engine::remove_checkpoint_tree(&app_data, &current_case_id);
            if req.confirmed_fields.is_empty() && req.confirmed_document_ids.is_empty() {
                increment_metric(app, "zero_touch_sources", 1);
            }
            let created = names
                .iter()
                .map(|name| patient_dir.join(name).display().to_string())
                .collect::<Vec<_>>();
            // Publication is already the irreversible business terminal point.
            // Persist independent plan-bound completion evidence before any further
            // best-effort post-publication metadata can fail.
            let local_completion = mark_local_completion(
                &app_data,
                &processing_job_sha256,
                &source_sha256,
                &processing_fingerprint,
            );
            let queue_completion = if let Some(lease) = central_queue_lease.as_mut() {
                lease.complete()
            } else {
                mark_shared_completion(&source, &processing_job_sha256).map(|_| ())
            };
            let case_completion =
                case_run.finish("completed", Some(&patient_dir), &created, &[], None);

            let mut completion_errors = Vec::new();
            if let Err(error) = &local_completion {
                completion_errors.push(format!("local_receipt: {error}"));
            }
            if let Err(error) = &queue_completion {
                completion_errors.push(format!("queue_receipt: {error}"));
            }
            if let Err(error) = &case_completion {
                completion_errors.push(format!("case_history: {error}"));
            }
            if !completion_errors.is_empty() {
                let details = serde_json::json!({
                    "errors": completion_errors,
                    "source_sha256": source_sha256,
                    "processing_job_sha256": processing_job_sha256,
                    "local_receipt_persisted": local_completion.is_ok(),
                    "queue_receipt_persisted": queue_completion.is_ok(),
                    "case_history_persisted": case_completion.is_ok(),
                });
                let _ = create_automation_exception(
                    app,
                    "publication_completion_metadata",
                    &source.display().to_string(),
                    "Комплект создан и опубликован, но часть квитанций завершения не сохранилась.",
                    &details,
                );
                let _ = append_audit_event(
                    app,
                    "publication_completion_metadata_degraded",
                    &source_sha256,
                    &details,
                );
            }

            if local_completion.is_err() && queue_completion.is_err() && case_completion.is_err() {
                // Never claim that published files were rolled back. The source is kept
                // out of the ordinary retry error path; a visible exception above tells
                // the operator that all completion ledgers require repair.
                let marker = workspace_hygiene::processed_marker_path(&source);
                let _ = std::fs::write(
                    &marker,
                    format!(
                        "sha256={source_sha256}\nprocessing_job_sha256={processing_job_sha256}\nstatus=published_completion_ledgers_failed\n"
                    ),
                );
            }
            if corpus_recording_enabled {
                let entry_id = format!("corpus-{}", Uuid::new_v4());
                let created_at = chrono::Utc::now().to_rfc3339();
                let corpus_db_path = default_state_db_path(app)?;
                let corpus_fingerprint_key = local_data_key_for(&corpus_db_path)?;
                let entry = build_corpus_entry(CorpusEntryRequest {
                    entry_id,
                    case_id: current_case_id.clone(),
                    source_sha256: &source_sha256,
                    fingerprint_key: &corpus_fingerprint_key,
                    input_text: &source_text,
                    domain: model_domain.clone(),
                    pack_id: (!pack.pack_id.trim().is_empty()).then(|| pack.pack_id.clone()),
                    cluster_id: Some(routing_recommendation.cluster_id.clone()),
                    model_case: &model_case_for_corpus,
                    deterministic_case: &deterministic_case_for_corpus,
                    final_case: &planning_case.case,
                    field_acceptance_source: if req.confirmed_fields.is_empty() {
                        CorpusAcceptanceSource::ZeroTouchShadow
                    } else {
                        CorpusAcceptanceSource::SpecialistConfirmed
                    },
                    proposed_kit_documents: routing_recommendation
                        .recommended_document_ids
                        .clone(),
                    kit_proposal_source: Some(if routing_recommendation.auto_select {
                        "curated-router:auto-candidate".into()
                    } else {
                        "curated-router:review".into()
                    }),
                    kit_documents: outputs
                        .iter()
                        .map(|output| output.document_id.clone())
                        .collect(),
                    kit_acceptance_source: if req.confirmed_document_ids.is_empty() {
                        CorpusAcceptanceSource::ZeroTouchShadow
                    } else {
                        CorpusAcceptanceSource::SpecialistConfirmed
                    },
                    created_at,
                });
                match entry.and_then(|entry| {
                    let metrics = corpus_entry_metrics(&entry);
                    repository_for(&corpus_db_path)?
                        .append_corpus_entry(&entry)
                        .map_err(|error| error.to_string())?;
                    append_audit_event(
                        app,
                        if entry.field_acceptance_source == CorpusAcceptanceSource::SpecialistConfirmed
                            || entry.kit_acceptance_source == CorpusAcceptanceSource::SpecialistConfirmed
                        {
                            "specialist_confirmed_corpus_recorded"
                        } else {
                            "zero_touch_shadow_corpus_recorded"
                        },
                        &source_sha256,
                        &serde_json::json!({
                            "entry_id": entry.entry_id,
                            "domain": entry.domain,
                            "kit_documents": entry.kit_documents,
                            "metrics": metrics,
                            "raw_values_stored": false,
                        }),
                    )?;
                    Ok::<(), String>(())
                }) {
                    Ok(()) => {}
                    Err(error) => {
                        let _ = create_automation_exception(
                            app,
                            "corpus_recorder",
                            &source.display().to_string(),
                            "Комплект создан, но обезличенная запись корпуса не сохранена.",
                            &serde_json::json!({ "error": error }),
                        );
                    }
                }
            }
            let source_finalize = match finalize_processed_source(
                &source,
                &source_sha256,
                &privacy,
                req.preserve_source_after_success,
            ) {
                Ok(details) => details,
                Err(error) => {
                    let marker = workspace_hygiene::processed_marker_path(&source);
                    let _ = std::fs::write(
                        &marker,
                        format!(
                            "sha256={source_sha256}\nstatus=published_hygiene_failed\nerror={error}\n"
                        ),
                    );
                    let details = serde_json::json!({
                        "error": error,
                        "marker": marker.display().to_string(),
                    });
                    let _ = create_automation_exception(
                        app,
                        "workspace_hygiene",
                        &source.display().to_string(),
                        "Комплект создан, но исходник не удалось переместить в архив рабочей папки.",
                        &details,
                    );
                    details
                }
            };
            if let Some(archived_source) = source_finalize
                .get("archived_source")
                .and_then(serde_json::Value::as_str)
            {
                let _ = case_run.update_source_path(Path::new(archived_source));
            }
            let _ = append_audit_event(
                app,
                "processed_source_finalized",
                &source_sha256,
                &source_finalize,
            );
            // Remove the canonical note and the legacy 18.0.7 name that included
            // the source extension (`Иванов.docx_ТРЕБУЕТ_ВНИМАНИЯ.txt`).  This
            // migration prevents stale “КОМПЛЕКТ НЕ СОЗДАН” files from surviving
            // after a later successful retry.
            let _ = std::fs::remove_file(source.with_file_name(attention_file_name(&stem)));
            let legacy_attention_stem = source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&stem);
            let _ = std::fs::remove_file(
                source.with_file_name(attention_file_name(legacy_attention_stem)),
            );
            let _ = std::fs::remove_file(source.with_file_name(unreadable_note_file_name(&stem)));
            let triage_document_ids = outputs
                .iter()
                .map(|output| output.document_id.clone())
                .collect::<Vec<_>>();
            let print_triage = match build_print_triage(
                app,
                &planning_case.case,
                &pack,
                &triage_document_ids,
            ) {
                Ok(report) => {
                    if !report.auto_print_allowed {
                        let patient_folder_for_review = patient_dir.display().to_string();
                        match persist_print_review_record(
                            app,
                            Some(&patient_folder_for_review),
                            &report,
                        ) {
                            Ok(path) => {
                                increment_metric(app, "print_review_queued", 1);
                                let details = serde_json::json!({
                                    "review_record": path.display().to_string(),
                                    "report": &report,
                                });
                                let _ = append_audit_event(
                                    app,
                                    "automatic_print_review_queued",
                                    &source_sha256,
                                    &details,
                                );
                            }
                            Err(error) => {
                                let details = serde_json::json!({ "error": error });
                                let _ = append_audit_event(
                                    app,
                                    "automatic_print_review_queue_failed",
                                    &source_sha256,
                                    &details,
                                );
                            }
                        }
                    } else {
                        increment_metric(app, "automatic_print_approved", 1);
                    }
                    Some(report)
                }
                Err(error) => {
                    let details = serde_json::json!({
                        "error": error,
                        "document_ids": triage_document_ids,
                    });
                    let _ = append_audit_event(
                        app,
                        "print_triage_unavailable",
                        &source_sha256,
                        &details,
                    );
                    None
                }
            };
            let elapsed_milliseconds = intake_started
                .elapsed()
                .as_millis()
                .min(u64::MAX as u128) as u64;
            increment_metric(app, "processing_milliseconds", elapsed_milliseconds);
            let _ = append_audit_event(
                app,
                "intake_roi_measured",
                &source_sha256,
                &serde_json::json!({
                    "processing_milliseconds": elapsed_milliseconds,
                    "generated_documents": names.len(),
                    "estimate_policy": "organization_baseline_minus_measured_runtime",
                }),
            );
            let created_documents = outputs
                .iter()
                .zip(created.iter())
                .map(|(output, path)| {
                    let label = pack
                        .documents
                        .iter()
                        .find(|document| document.id == output.document_id)
                        .map(|document| document.button_label.clone())
                        .unwrap_or_else(|| output.document_id.clone());
                    CreatedDocumentOutputDto {
                        document_id: output.document_id.clone(),
                        label,
                        path: path.clone(),
                    }
                })
                .collect();
            Ok(CreatedDocumentsIntakeResponse {
                status: "processed".into(),
                patient_folder: Some(patient_dir.display().to_string()),
                created_files: created,
                created_documents,
                missing: Vec::new(),
                attention_file: None,
                print_triage,
                message: if publication_warnings.is_empty() {
                    "Комплект документов создан и опубликован атомарно.".into()
                } else {
                    format!(
                        "Комплект документов опубликован. Требует внимания: {}",
                        publication_warnings.join(" ")
                    )
                },
            })
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListCaseRunsRequest {
    #[serde(default = "default_case_run_limit")]
    limit: usize,
}

fn default_case_run_limit() -> usize {
    100
}

#[tauri::command]
fn list_case_runs(
    req: ListCaseRunsRequest,
    app: tauri::AppHandle,
) -> Result<Vec<CaseRunRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_case_runs(req.limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_queue_status() -> central_queue::QueueStatus {
    central_queue::status()
}

#[derive(Debug, Serialize)]
struct CorpusStatusResponse {
    recording_enabled: bool,
    entry_count: u64,
    privacy_mode: String,
    message: String,
}

#[tauri::command]
fn get_corpus_status(app: tauri::AppHandle) -> Result<CorpusStatusResponse, String> {
    let config = load_semantic_model_config(&app)?;
    let entry_count = repository_for(&default_state_db_path(&app)?)?
        .corpus_entry_count()
        .map_err(|error| error.to_string())?;
    Ok(CorpusStatusResponse {
        recording_enabled: config.corpus_recording_enabled,
        entry_count,
        privacy_mode: "encrypted-hashed-no-raw-values".into(),
        message: if config.corpus_recording_enabled {
            format!(
                "Сбор обезличенного корпуса включён с согласия пилота. Завершённых записей: {entry_count}."
            )
        } else {
            format!(
                "Сбор корпуса выключен. Ранее сохранённых обезличенных записей: {entry_count}."
            )
        },
    })
}


#[derive(Debug, Deserialize)]
struct LearnedKitDecisionRequest {
    domain: DomainKind,
    cluster_id: String,
    #[serde(default)]
    pack_id: Option<String>,
}

#[tauri::command]
fn get_learned_kit_decision(
    req: LearnedKitDecisionRequest,
    app: tauri::AppHandle,
) -> Result<Option<KitLearningDecision>, String> {
    let cluster_id = req.cluster_id.trim();
    if cluster_id.is_empty() {
        return Err("cluster_id is required".into());
    }
    let entries = repository_for(&default_state_db_path(&app)?)?
        .list_corpus_entries(10_000)
        .map_err(|error| error.to_string())?;
    let key = KitRuleKey {
        domain: req.domain,
        cluster_id: cluster_id.to_string(),
        pack_id: req.pack_id.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
    };
    Ok(decision_for_key(&entries, &key, KitPromotionPolicy::default()))
}

#[derive(Debug, Deserialize)]
struct ExportCorpusRequest {
    output_path: String,
    #[serde(default = "default_corpus_export_limit")]
    limit: usize,
}

fn default_corpus_export_limit() -> usize {
    10_000
}

#[derive(Debug, Serialize)]
struct CorpusExportItem {
    entry: CorpusEntry,
    metrics: CorpusEntryMetrics,
}

#[derive(Debug, Serialize)]
struct CorpusExportResponse {
    output_path: String,
    entry_count: usize,
    schema: String,
}

#[tauri::command]
fn export_corpus(
    req: ExportCorpusRequest,
    app: tauri::AppHandle,
) -> Result<CorpusExportResponse, String> {
    let output = resolve_user_path(&app, req.output_path.trim())?;
    if output.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err("Экспорт обезличенного корпуса должен иметь расширение .json".into());
    }
    if output.exists() {
        return Err("Файл экспорта уже существует. Укажите новое имя, чтобы не перезаписать доказательный корпус.".into());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let entries = repository_for(&default_state_db_path(&app)?)?
        .list_corpus_entries(req.limit.clamp(1, 10_000))
        .map_err(|error| error.to_string())?;
    if entries.is_empty() {
        return Err("Обезличенный корпус пока пуст: завершите хотя бы одно дело с добровольно включённой записью корпуса.".into());
    }
    let items = entries
        .into_iter()
        .map(|entry| CorpusExportItem {
            metrics: corpus_entry_metrics(&entry),
            entry,
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "schema": "dokkomplekt.ground-truth-corpus.v1",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "privacy": {
            "raw_source_text": false,
            "raw_field_values": false,
            "storage_at_rest": "encrypted",
            "comparison_values": "installation-keyed-hmac-sha256"
        },
        "entries": items,
    });
    let temporary = output.with_extension(format!("json.{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?;
    let write_result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &output).map_err(|error| error.to_string())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    append_audit_event(
        &app,
        "ground_truth_corpus_exported",
        "",
        &serde_json::json!({
            "entry_count": payload["entries"].as_array().map(Vec::len).unwrap_or_default(),
            "schema": "dokkomplekt.ground-truth-corpus.v1",
            "raw_values_exported": false,
        }),
    )?;
    Ok(CorpusExportResponse {
        output_path: output.display().to_string(),
        entry_count: payload["entries"].as_array().map(Vec::len).unwrap_or_default(),
        schema: "dokkomplekt.ground-truth-corpus.v1".into(),
    })
}

#[derive(Debug, Deserialize)]
struct RetryCaseRunRequest {
    case_id: String,
}

#[tauri::command]
fn retry_case_run(
    req: RetryCaseRunRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let record = repo
        .case_run_by_id(req.case_id.trim())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Дело не найдено.".to_string())?;
    let mut intake: CreatedDocumentsIntakeRequest = serde_json::from_str(&record.request_json)
        .map_err(|error| format!("Сохранённый план дела повреждён: {error}"))?;
    let original = PathBuf::from(&record.source_path);
    let source = if original.exists() {
        original
    } else {
        let file_name = original
            .file_name()
            .ok_or_else(|| "Не удалось определить имя исходника дела.".to_string())?;
        record
            .patient_folder
            .as_deref()
            .map(PathBuf::from)
            .map(|folder| folder.join(file_name))
            .filter(|candidate| candidate.exists())
            .ok_or_else(|| {
                "Исходник дела не найден ни в архиве, ни в готовом комплекте. Переиздание невозможно без исходного файла.".to_string()
            })?
    };
    intake.source_path = source.display().to_string();
    if record.status == "completed" {
        intake.force_reissue = true;
        intake.preserve_source_after_success = true;
        append_audit_event(
            &app,
            "case_reissue_requested",
            &record.source_sha256,
            &serde_json::json!({ "previous_case_id": record.case_id }),
        )?;
    } else {
        intake.resume_from_case_id = Some(record.case_id.clone());
        repo.update_case_run(
            &record.case_id,
            "cancelled",
            record.patient_folder.as_deref(),
            &record.created_files_json,
            &record.missing_json,
            Some("Повторный запуск создан как новая атомарная попытка."),
        )
        .map_err(|error| error.to_string())?;
    }
    perform_created_documents_intake(&state, &app, intake)
        .and_then(|response| serde_json::to_value(response).map_err(|error| error.to_string()))
}

#[tauri::command]
fn get_privacy_preferences(app: tauri::AppHandle) -> Result<PrivacyPreferences, String> {
    load_privacy_preferences(&app)
}

#[derive(Debug, Deserialize)]
struct UpdatePrivacyPreferencesRequest {
    preferences: PrivacyPreferences,
}

#[tauri::command]
fn update_privacy_preferences(
    req: UpdatePrivacyPreferencesRequest,
    app: tauri::AppHandle,
) -> Result<PrivacyPreferences, String> {
    persist_privacy_preferences(&app, &req.preferences)?;
    append_audit_event(
        &app,
        "privacy_preferences_updated",
        "",
        &serde_json::to_value(&req.preferences).map_err(|error| error.to_string())?,
    )?;
    Ok(req.preferences)
}

#[tauri::command]
fn run_workspace_hygiene(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<WorkspaceHygieneReport, String> {
    let privacy = load_privacy_preferences(&app)?;
    let policy = privacy.retention_policy();
    let mut roots = BTreeSet::new();
    if let Ok(guard) = state.watcher.lock() {
        if let Some(handle) = guard.as_ref() {
            roots.insert(handle.folder.clone());
        }
    }
    if let Ok(repo) = repository_for(&default_state_db_path(&app)?) {
        if let Ok(cases) = repo.list_case_runs(500) {
            for case in cases {
                if !case.output_root.trim().is_empty() {
                    roots.insert(PathBuf::from(case.output_root));
                }
                if let Some(parent) = Path::new(&case.source_path).parent() {
                    roots.insert(parent.to_path_buf());
                }
            }
        }
    }
    let mut aggregate = WorkspaceHygieneReport::default();
    let now = std::time::SystemTime::now();
    for root in roots {
        match workspace_hygiene::cleanup_workspace_folder(&root, &policy, now) {
            Ok(report) => {
                aggregate
                    .archived_processed_sources
                    .extend(report.archived_processed_sources);
                aggregate.archived_service_files.extend(report.archived_service_files);
                aggregate.removed_orphan_markers.extend(report.removed_orphan_markers);
                aggregate
                    .removed_expired_archived_files
                    .extend(report.removed_expired_archived_files);
                aggregate.warnings.extend(report.warnings);
            }
            Err(error) => aggregate
                .warnings
                .push(format!("{}: {error}", root.display())),
        }
    }
    let details = serde_json::to_value(&aggregate).map_err(|error| error.to_string())?;
    append_audit_event(&app, "workspace_hygiene_manual", "", &details)?;
    Ok(aggregate)
}

#[derive(Debug, Deserialize)]
struct ListAutomationExceptionsRequest {
    #[serde(default)]
    include_resolved: bool,
}

#[tauri::command]
fn list_automation_exceptions(
    req: ListAutomationExceptionsRequest,
    app: tauri::AppHandle,
) -> Result<Vec<AutomationExceptionRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_exceptions(req.include_resolved)
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct ResolveAutomationExceptionRequest {
    exception_id: String,
    resolution: String,
}

#[tauri::command]
fn resolve_automation_exception(
    req: ResolveAutomationExceptionRequest,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let exception_id = req.exception_id.trim();
    if exception_id.is_empty() {
        return Err("Не указан идентификатор исключения.".into());
    }
    let resolved = repository_for(&default_state_db_path(&app)?)?
        .resolve_exception(exception_id, req.resolution.trim())
        .map_err(|error| error.to_string())?;
    if resolved {
        increment_metric(&app, "user_confirmations", 1);
        append_audit_event(
            &app,
            "exception_resolved",
            "",
            &serde_json::json!({
                "exception_id": exception_id,
                "resolution": req.resolution.trim(),
            }),
        )?;
    }
    Ok(resolved)
}

#[derive(Debug, Deserialize)]
struct ConfirmRiskExceptionRequest {
    exception_id: String,
}

#[tauri::command]
fn confirm_risk_exception_and_retry(
    req: ConfirmRiskExceptionRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let exception_id = req.exception_id.trim();
    if exception_id.is_empty() {
        return Err("Не указан идентификатор исключения.".into());
    }
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let exception = repo
        .list_exceptions(false)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.exception_id == exception_id)
        .ok_or_else(|| "Открытое исключение не найдено.".to_string())?;
    if exception.category != "risk_gate" {
        return Err("Пакетное подтверждение доступно только для risk-gate; отсутствующие поля нужно заполнить.".into());
    }
    let details: serde_json::Value = serde_json::from_str(&exception.details_json)
        .map_err(|error| format!("Детали risk-gate повреждены: {error}"))?;
    let mut confirmed_fields = details
        .get("blockers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("field_id").and_then(serde_json::Value::as_str))
        .filter(|field_id| is_valid_field_id(field_id))
        .map(str::to_string)
        .collect::<Vec<_>>();
    confirmed_fields.sort();
    confirmed_fields.dedup();
    if confirmed_fields.is_empty() {
        return Err("В risk-gate нет значений, доступных для пакетного подтверждения.".into());
    }
    let record = repo
        .list_case_runs(500)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|case| case.source_path == exception.source_path && case.status == "attention")
        .ok_or_else(|| "Не найдено остановленное дело для этого источника.".to_string())?;
    let mut intake: CreatedDocumentsIntakeRequest = serde_json::from_str(&record.request_json)
        .map_err(|error| format!("Сохранённый план дела повреждён: {error}"))?;
    if !Path::new(&record.source_path).exists() {
        return Err("Исходный файл больше не существует в рабочей папке.".into());
    }
    intake.confirmed_fields = confirmed_fields.clone();
    let resolved = repo
        .resolve_exception(
            exception_id,
            &format!(
                "Специалист одной кнопкой подтвердил {} значений: {}",
                confirmed_fields.len(),
                confirmed_fields.join(", ")
            ),
        )
        .map_err(|error| error.to_string())?;
    if !resolved {
        return Err("Исключение уже закрыто другим процессом.".into());
    }
    increment_metric(&app, "user_confirmations", confirmed_fields.len() as u64);
    increment_metric(&app, "attention_resolutions", 1);
    append_audit_event(
        &app,
        "risk_values_batch_confirmed",
        &record.source_sha256,
        &serde_json::json!({
            "exception_id": exception_id,
            "fields": &confirmed_fields,
            "case_id": record.case_id,
        }),
    )?;
    perform_created_documents_intake(&state, &app, intake)
        .and_then(|response| serde_json::to_value(response).map_err(|error| error.to_string()))
}

#[derive(Debug, Deserialize)]
struct ConfirmBundleExceptionRequest {
    exception_id: String,
    document_ids: Vec<String>,
}

#[tauri::command]
fn confirm_bundle_exception_and_retry(
    req: ConfirmBundleExceptionRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let exception_id = req.exception_id.trim();
    if exception_id.is_empty() {
        return Err("Не указан идентификатор исключения.".into());
    }
    let known_ids = state
        .pack
        .lock()
        .map_err(|_| "state lock failed")?
        .documents
        .iter()
        .map(|document| document.id.clone())
        .collect::<BTreeSet<_>>();
    let selected = req
        .document_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| known_ids.contains(value))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("Не выбран ни один существующий документ комплекта.".into());
    }

    let repo = repository_for(&default_state_db_path(&app)?)?;
    let exception = repo
        .list_exceptions(false)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.exception_id == exception_id)
        .ok_or_else(|| "Открытое исключение не найдено.".to_string())?;
    if exception.category != "bundle_decision" {
        return Err("Подтверждение состава доступно только для исключения Bundle Decision Engine.".into());
    }
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
    let resolved = repo
        .resolve_exception(
            exception_id,
            &format!("Специалист подтвердил комплект: {}", selected.join(", ")),
        )
        .map_err(|error| error.to_string())?;
    if !resolved {
        return Err("Исключение уже закрыто другим процессом.".into());
    }
    increment_metric(&app, "bundle_confirmations", 1);
    increment_metric(&app, "attention_resolutions", 1);
    append_audit_event(
        &app,
        "document_bundle_confirmed",
        &record.source_sha256,
        &serde_json::json!({
            "exception_id": exception_id,
            "document_ids": &selected,
            "case_id": record.case_id,
        }),
    )?;
    perform_created_documents_intake(&state, &app, intake)
        .and_then(|response| serde_json::to_value(response).map_err(|error| error.to_string()))
}

#[tauri::command]
fn get_automation_metrics(app: tauri::AppHandle) -> Result<AutomationMetrics, String> {
    repository_for(&default_state_db_path(&app)?)?
        .automation_metrics()
        .map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
struct ListAuditEventsRequest {
    #[serde(default = "default_audit_limit")]
    limit: usize,
}

fn default_audit_limit() -> usize {
    100
}

#[tauri::command]
fn list_audit_events(
    req: ListAuditEventsRequest,
    app: tauri::AppHandle,
) -> Result<Vec<AuditEventRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_audit_events(req.limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_clause_blocks(app: tauri::AppHandle) -> Result<Vec<ClauseBlockRecord>, String> {
    repository_for(&default_state_db_path(&app)?)?
        .list_clause_blocks()
        .map_err(|e| e.to_string())
}
#[derive(Debug, Deserialize)]
struct SaveClauseBlockRequest {
    block_id: String,
    title: String,
    content: String,
}
#[tauri::command]
fn save_clause_block(
    req: SaveClauseBlockRequest,
    app: tauri::AppHandle,
) -> Result<Vec<ClauseBlockRecord>, String> {
    let id = req.block_id.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Идентификатор блока может содержать латинские буквы, цифры, _ и -.".into());
    }
    let repo = repository_for(&default_state_db_path(&app)?)?;
    repo.save_clause_block(id, req.title.trim(), &req.content)
        .map_err(|e| e.to_string())?;
    repo.list_clause_blocks().map_err(|e| e.to_string())
}
#[derive(Debug, Deserialize)]
struct DeleteClauseBlockRequest {
    block_id: String,
}
#[tauri::command]
fn delete_clause_block(
    req: DeleteClauseBlockRequest,
    app: tauri::AppHandle,
) -> Result<Vec<ClauseBlockRecord>, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    repo.delete_clause_block(req.block_id.trim())
        .map_err(|e| e.to_string())?;
    repo.list_clause_blocks().map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct SuggestTemplateMarkupRequest {
    file_name: String,
    bytes_base64: String,
    default_year: i32,
}
#[tauri::command]
fn suggest_template_markup_command(
    req: SuggestTemplateMarkupRequest,
) -> Result<Vec<TemplateMarkupCandidate>, String> {
    let bytes = decode_word_payload(Some(&req.file_name), &req.bytes_base64)?;
    let text = extract_docx_text_from_bytes(&bytes).map_err(|e| e.to_string())?;
    Ok(suggest_template_markup(&text, req.default_year))
}
#[derive(Debug, Deserialize)]
struct ApplyTemplateMarkupRequest {
    input_path: String,
    output_path: String,
    replacements: Vec<TemplateMarkupReplacement>,
}
#[tauri::command]
fn apply_template_markup_command(
    req: ApplyTemplateMarkupRequest,
    app: tauri::AppHandle,
) -> Result<TemplateMarkupReport, String> {
    let input = resolve_user_path(&app, &req.input_path)?;
    let output = resolve_user_path(&app, &req.output_path)?;
    apply_template_markup_file(&input, &output, &req.replacements).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
struct PreviewMailMergeRequest {
    delimited_text: String,
}
#[tauri::command]
fn preview_mail_merge(req: PreviewMailMergeRequest) -> Result<MailMergeTable, String> {
    parse_delimited_table(&req.delimited_text)
}

#[derive(Debug, Deserialize)]
struct PrepareMailMergeFileRequest {
    file_name: String,
    bytes_base64: String,
}

#[derive(Debug, Serialize)]
struct PrepareMailMergeFileResponse {
    delimited_text: String,
    table: MailMergeTable,
}

#[tauri::command]
fn prepare_mail_merge_file(
    req: PrepareMailMergeFileRequest,
) -> Result<PrepareMailMergeFileResponse, String> {
    let bytes = universal_intake::decode_uploaded_payload(&req.file_name, &req.bytes_base64)?;
    let delimited_text =
        universal_intake::mail_merge_upload_to_delimited(&req.file_name, &bytes)?;
    let table = parse_delimited_table(&delimited_text)?;
    Ok(PrepareMailMergeFileResponse {
        delimited_text,
        table,
    })
}

#[derive(Debug, Deserialize)]
struct ImportTemplateFileRequest {
    document_id: String,
    /// Original file name, used only for a human-readable suffix.
    #[serde(default)]
    file_name: Option<String>,
    /// Raw DOCX bytes (base64) from the webview file picker…
    #[serde(default)]
    bytes_base64: Option<String>,
    /// …or plain template text pasted by the user; a real DOCX is generated.
    #[serde(default)]
    template_text: Option<String>,
}

#[derive(Debug, Serialize)]
struct ImportTemplateFileResponse {
    template_path: String,
    extracted_text: String,
}

/// Persist a user template as a real DOCX under app_data/user-templates and return
/// its absolute path plus the extracted text. This closes the gap between the UI
/// (which previously invented non-existent paths) and the renderer (which needs a
/// real file): whether the user picked a .docx or pasted plain text, the pack ends
/// up pointing at a renderable file.
#[tauri::command]
fn import_template_file(
    req: ImportTemplateFileRequest,
    app: tauri::AppHandle,
) -> Result<ImportTemplateFileResponse, String> {
    let templates_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("user-templates");
    std::fs::create_dir_all(&templates_dir).map_err(|e| e.to_string())?;
    let original_name = req.file_name.as_deref().unwrap_or_default();
    let lower_name = original_name.to_ascii_lowercase();
    let extension = if req.bytes_base64.is_some() && lower_name.ends_with(".docm") {
        "docm"
    } else {
        "docx"
    };
    let stem = original_name
        .strip_suffix(".docx")
        .or_else(|| original_name.strip_suffix(".DOCX"))
        .or_else(|| original_name.strip_suffix(".docm"))
        .or_else(|| original_name.strip_suffix(".DOCM"))
        .unwrap_or(original_name);
    let suffix = (!stem.trim().is_empty())
        .then(|| sanitize_path_component(stem))
        .filter(|name| !name.is_empty())
        .map(|name| format!("_{name}"))
        .unwrap_or_default();
    let desired_target = templates_dir.join(format!(
        "{}{}.{}",
        sanitize_path_component(&req.document_id),
        suffix,
        extension
    ));
    let reservation = UniqueFileReservation::acquire(&desired_target)?;
    let target = reservation.path.clone();

    match (&req.bytes_base64, &req.template_text) {
        (Some(bytes_b64), _) => {
            let bytes = decode_word_payload(req.file_name.as_deref(), bytes_b64)?;
            // Validate in memory before persisting the upload. Active content
            // and external relationships are rejected before Mark-of-the-Web can
            // be lost by copying the file into app-data.
            validate_safe_template_bytes(&bytes).map_err(|error| {
                format!(
                    "Шаблон содержит макросы, встроенные объекты или внешние связи и заблокирован: {error}. Сохраните безопасную копию как DOCX без активного содержимого."
                )
            })?;
            extract_docx_text_from_bytes(&bytes)
                .map_err(|e| format!("Файл не распознан как DOCX: {e}"))?;
            std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
        }
        (None, Some(text)) => {
            if text.len() > MAX_DOCX_BYTES {
                return Err("Текст шаблона слишком большой: максимум 50 МБ.".into());
            }
            create_docx_from_text(&target, text).map_err(|e| e.to_string())?;
        }
        (None, None) => return Err("Передайте DOCX-файл или текст шаблона.".into()),
    }

    // Extract through the same reader the pipeline uses; a broken upload is
    // rejected here, before it can enter the pack.
    let extracted_text = extract_docx_text(&target).map_err(|e| {
        let _ = std::fs::remove_file(&target);
        format!("Файл не распознан как DOCX: {e}")
    })?;
    let target = reservation.commit()?;
    Ok(ImportTemplateFileResponse {
        template_path: target.display().to_string(),
        extracted_text,
    })
}

#[derive(Debug, Deserialize)]
struct SemanticExtractRequest {
    source_text: String,
    #[serde(default)]
    model_output: Option<String>,
    default_year: i32,
}

#[derive(Debug, Serialize)]
struct SemanticFieldDto {
    field_id: String,
    value: String,
    confidence: f32,
    method: String,
    source: String,
    evidence: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SemanticExtractResponse {
    fields: Vec<SemanticFieldDto>,
    warnings: Vec<String>,
    model_applied: bool,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct PrintFilesRequest {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    jobs: Vec<PrintJobRequest>,
}

#[tauri::command]
fn print_files(
    req: PrintFilesRequest,
    app: tauri::AppHandle,
) -> Result<PrintFilesResponse, String> {
    if req.paths.is_empty() && req.jobs.is_empty() {
        return Err("Не выбран ни один файл для печати.".into());
    }
    let raw_jobs = if req.jobs.is_empty() {
        req.paths
            .into_iter()
            .map(|path| PrintJobRequest { path, copies: 1 })
            .collect::<Vec<_>>()
    } else {
        req.jobs
    };
    let mut resolved_by_path = BTreeMap::<String, (PathBuf, u16)>::new();
    for job in raw_jobs {
        if job.copies > MAX_PRINT_COPIES {
            return Err(format!(
                "Количество копий для «{}» превышает допустимый предел {MAX_PRINT_COPIES}.",
                job.path
            ));
        }
        let path = resolve_user_path(&app, &job.path)?;
        let key = path.to_string_lossy().to_string();
        resolved_by_path.insert(key, (path, job.copies));
    }
    let resolved = resolved_by_path.into_values().collect::<Vec<_>>();
    let print_preferences = load_print_preferences(&app)?;
    let response = print_resolved_jobs(&resolved, &print_preferences);
    let details = serde_json::to_value(&response).map_err(|error| error.to_string())?;
    if response.failed_files.is_empty() {
        append_audit_event(&app, "manual_print_queued", "", &details)?;
    } else {
        increment_metric(&app, "print_failures", response.failed_files.len() as u64);
        create_automation_exception(
            &app,
            "print_failure",
            "",
            "Не все выбранные документы были отправлены на печать.",
            &details,
        )?;
        append_audit_event(&app, "manual_print_failed", "", &details)?;
    }
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ExportPdfRequest {
    paths: Vec<String>,
    #[serde(default)]
    output_dir: Option<String>,
    #[serde(default)]
    pdfa_1: bool,
}

#[derive(Debug, Serialize)]
struct PdfExportFailure {
    path: String,
    error: String,
}

#[derive(Debug, Serialize)]
struct ExportPdfResponse {
    created_files: Vec<String>,
    failed_files: Vec<PdfExportFailure>,
    pdfa_1_requested: bool,
    conformance_note: String,
}

#[tauri::command]
fn export_files_to_pdf(
    req: ExportPdfRequest,
    app: tauri::AppHandle,
) -> Result<ExportPdfResponse, String> {
    if req.paths.is_empty() {
        return Err("Не выбран ни один файл для PDF-экспорта.".into());
    }
    let explicit_output = req
        .output_dir
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| resolve_user_path(&app, value))
        .transpose()?;
    if let Some(output) = &explicit_output {
        std::fs::create_dir_all(output).map_err(|error| error.to_string())?;
    }

    let mut created_files = Vec::new();
    let mut failed_files = Vec::new();
    for raw_path in req.paths {
        let path = match resolve_user_path(&app, &raw_path) {
            Ok(path) => path,
            Err(error) => {
                failed_files.push(PdfExportFailure {
                    path: raw_path,
                    error,
                });
                continue;
            }
        };
        let destination_dir = explicit_output
            .clone()
            .or_else(|| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(sanitize_path_component)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "document".into());
        let suffix = if req.pdfa_1 { ".pdfa-1.pdf" } else { ".pdf" };
        let desired = destination_dir.join(format!("{stem}{suffix}"));
        let result = (|| -> Result<PathBuf, String> {
            let (temporary_pdf, temporary_dir) =
                convert_office_document_to_pdf(&path, req.pdfa_1)?;
            let reservation = UniqueFileReservation::acquire(&desired)?;
            if let Err(error) = std::fs::copy(&temporary_pdf, &reservation.path) {
                let _ = std::fs::remove_dir_all(&temporary_dir);
                return Err(format!("Не удалось сохранить PDF: {error}"));
            }
            let _ = std::fs::remove_dir_all(&temporary_dir);
            reservation.commit()
        })();
        match result {
            Ok(output) => created_files.push(output.display().to_string()),
            Err(error) => failed_files.push(PdfExportFailure {
                path: path.display().to_string(),
                error,
            }),
        }
    }
    let response = ExportPdfResponse {
        created_files,
        failed_files,
        pdfa_1_requested: req.pdfa_1,
        conformance_note: if req.pdfa_1 {
            "LibreOffice запрошен в режиме PDF/A-1 с тегированной структурой. Для юридически значимого архива профиль PDF/A и уровень соответствия необходимо дополнительно проверять валидатором veraPDF; приложение не выдаёт неподтверждённый статус PDF/A-1A.".into()
        } else {
            "Создан стандартный PDF через локальный LibreOffice sidecar.".into()
        },
    };
    let details = serde_json::to_value(&response).map_err(|error| error.to_string())?;
    append_audit_event(&app, "pdf_export", "", &details)?;
    Ok(response)
}


#[derive(Debug, Deserialize)]
struct CreateKedoPackageRequest {
    paths: Vec<String>,
    output_root: String,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Serialize)]
struct KedoPackageDocument {
    file_name: String,
    sha256: String,
    size_bytes: u64,
    detached_signature_name: String,
}

#[derive(Debug, Serialize)]
struct CreateKedoPackageResponse {
    package_folder: String,
    manifest_path: String,
    checksum_path: String,
    documents: Vec<KedoPackageDocument>,
    conformance_note: String,
}

fn verify_pdf_signature(path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Не удалось открыть PDF {}: {error}", path.display()))?;
    let mut header = [0u8; 5];
    file.read_exact(&mut header)
        .map_err(|error| format!("Не удалось проверить PDF {}: {error}", path.display()))?;
    if &header != b"%PDF-" {
        return Err(format!("Файл не имеет PDF-сигнатуры: {}", path.display()));
    }
    Ok(())
}

/// Creates an atomic KEDO hand-off package: locally converted PDF/A-1 candidates,
/// an XML descriptor, SHA-256 inventory and explicit detached-signature slots.
/// Cryptographic signing is intentionally left to a configured CryptoPro/Goskey
/// provider; the application never fabricates an empty or fake signature.
#[tauri::command]
fn create_kedo_package(
    req: CreateKedoPackageRequest,
    app: tauri::AppHandle,
) -> Result<CreateKedoPackageResponse, String> {
    if req.paths.is_empty() {
        return Err("Для КЭДО-пакета не выбран ни один документ.".into());
    }
    let output_root = resolve_user_path(&app, &req.output_root)?;
    std::fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&output_root, Duration::from_secs(24 * 60 * 60))?;
    let stage = output_root.join(format!(".kedo-stage-{}", Uuid::new_v4()));
    std::fs::create_dir(&stage)
        .map_err(|error| format!("Не удалось создать staging КЭДО: {error}"))?;

    let result = (|| -> Result<(PathBuf, Vec<KedoPackageDocument>), String> {
        let mut documents = Vec::new();
        for (index, raw_path) in req.paths.iter().enumerate() {
            let source = resolve_user_path(&app, raw_path)?;
            validate_printable_file(&source)?;
            let extension = source
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let stem = source
                .file_stem()
                .and_then(|value| value.to_str())
                .map(sanitize_path_component)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("document-{}", index + 1));
            let desired = stage.join(format!("{stem}.pdf"));
            let reservation = UniqueFileReservation::acquire(&desired)?;
            let destination = reservation.path.clone();

            if extension == "pdf" {
                verify_pdf_signature(&source)?;
                std::fs::copy(&source, &destination)
                    .map_err(|error| format!("Не удалось добавить PDF в КЭДО-пакет: {error}"))?;
            } else {
                let (temporary_pdf, temporary_dir) =
                    convert_office_document_to_pdf(&source, true)?;
                let copied = std::fs::copy(&temporary_pdf, &destination)
                    .map_err(|error| format!("Не удалось добавить PDF/A-кандидат: {error}"));
                let _ = std::fs::remove_dir_all(&temporary_dir);
                copied?;
            }
            verify_pdf_signature(&destination)?;
            let destination = reservation.commit()?;
            let (size_bytes, _, sha256) = file_content_signature(&destination)?;
            let file_name = destination
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document.pdf")
                .to_string();
            documents.push(KedoPackageDocument {
                detached_signature_name: format!("{file_name}.sgn"),
                file_name,
                sha256,
                size_bytes,
            });
        }

        let title = req
            .title
            .as_deref()
            .map(sanitize_path_component)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "КЭДО-пакет".into());
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kedoPackage schema=\"1\">\n  <title>{}</title>\n  <createdUnix>{}</createdUnix>\n  <pdfProfile requested=\"PDF/A-1\" certified=\"false\" />\n  <documents>\n",
            xml_escape(&title),
            OffsetDateTime::now_utc().unix_timestamp()
        );
        let mut checksums = String::new();
        let mut signatures = Vec::new();
        for (index, document) in documents.iter().enumerate() {
            xml.push_str(&format!(
                "    <document index=\"{}\"><file>{}</file><sha256>{}</sha256><sizeBytes>{}</sizeBytes><detachedSignature required=\"true\">{}</detachedSignature></document>\n",
                index + 1,
                xml_escape(&document.file_name),
                document.sha256,
                document.size_bytes,
                xml_escape(&document.detached_signature_name),
            ));
            checksums.push_str(&format!("{}  {}\n", document.sha256, document.file_name));
            signatures.push(serde_json::json!({
                "document": document.file_name,
                "signature": document.detached_signature_name,
                "status": "required",
            }));
        }
        xml.push_str("  </documents>\n</kedoPackage>\n");
        std::fs::write(stage.join("kedo-manifest.xml"), xml)
            .map_err(|error| format!("Не удалось записать XML КЭДО: {error}"))?;
        std::fs::write(stage.join("SHA256SUMS.txt"), checksums)
            .map_err(|error| format!("Не удалось записать контрольные суммы: {error}"))?;
        std::fs::write(
            stage.join("SIGNATURES_REQUIRED.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": 1,
                "signatures": signatures,
                "note": "Подписи должны быть созданы внешним доверенным провайдером КриптоПро/Госключ; Dokkomplekt не подделывает ЭП."
            }))
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("Не удалось записать план подписания: {error}"))?;
        Ok((output_root.join(title), documents))
    })();

    let (desired, documents) = match result {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    let published = match publish_stage_to_unique_directory(&stage, &desired) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(error);
        }
    };
    let response = CreateKedoPackageResponse {
        manifest_path: published.join("kedo-manifest.xml").display().to_string(),
        checksum_path: published.join("SHA256SUMS.txt").display().to_string(),
        package_folder: published.display().to_string(),
        documents,
        conformance_note: "Создан безопасный hand-off КЭДО: PDF/A-1 запрошен у LibreOffice, но юридическая маркировка PDF/A-1A требует отдельной проверки veraPDF; откреплённые подписи должны создать КриптоПро, Госключ или оператор КЭДО.".into(),
    };
    append_audit_event(
        &app,
        "kedo_package_created",
        "",
        &serde_json::to_value(&response).map_err(|error| error.to_string())?,
    )?;
    Ok(response)
}

#[derive(Debug, Deserialize)]
struct PickFolderRequest {
    initial_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct PickFolderResponse {
    selected_path: Option<String>,
}

#[tauri::command]
async fn pick_folder(req: PickFolderRequest) -> Result<PickFolderResponse, String> {
    let selected_path = tauri::async_runtime::spawn_blocking(move || pick_folder_blocking(req.initial_path))
        .await
        .map_err(|error| format!("Не удалось открыть выбор папки: {error}"))??;
    Ok(PickFolderResponse { selected_path })
}

fn pick_folder_blocking(initial_path: Option<String>) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'Выберите папку'
$dialog.ShowNewFolderButton = $true
if ($env:DOKKOMPLEKT_PICK_FOLDER_INITIAL -and (Test-Path -LiteralPath $env:DOKKOMPLEKT_PICK_FOLDER_INITIAL -PathType Container)) {
  $dialog.SelectedPath = $env:DOKKOMPLEKT_PICK_FOLDER_INITIAL
}
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  [Console]::Out.Write($dialog.SelectedPath)
}
"#;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-STA", "-Command", script])
            .env(
                "DOKKOMPLEKT_PICK_FOLDER_INITIAL",
                initial_path.as_deref().unwrap_or_default(),
            )
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Не удалось запустить системный выбор папки: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Системный выбор папки завершился с ошибкой: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        normalized_picker_output(&output.stdout)
    }

    #[cfg(target_os = "macos")]
    {
        let mut script = String::from("POSIX path of (choose folder with prompt \"Выберите папку\"");
        if let Some(path) = initial_path.filter(|value| Path::new(value).is_dir()) {
            script.push_str(" default location POSIX file ");
            script.push_str(&format!("{:?}", path));
        }
        script.push(')');
        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .map_err(|error| format!("Не удалось открыть системный выбор папки: {error}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Ok(None);
            }
            return Err(format!("Системный выбор папки завершился с ошибкой: {}", stderr.trim()));
        }
        normalized_picker_output(&output.stdout)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let initial = initial_path.filter(|value| Path::new(value).is_dir());
        let output = if command_exists("zenity") {
            let mut command = std::process::Command::new("zenity");
            command.args(["--file-selection", "--directory", "--title=Выберите папку"]);
            if let Some(path) = initial.as_deref() {
                command.arg(format!("--filename={}/", path.trim_end_matches('/')));
            }
            command.output()
        } else if command_exists("kdialog") {
            let mut command = std::process::Command::new("kdialog");
            command.arg("--getexistingdirectory");
            command.arg(initial.as_deref().unwrap_or("."));
            command.output()
        } else {
            return Err("Системный выбор папки недоступен: установите zenity или kdialog.".into());
        }
        .map_err(|error| format!("Не удалось открыть системный выбор папки: {error}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        normalized_picker_output(&output.stdout)
    }
}

fn normalized_picker_output(bytes: &[u8]) -> Result<Option<String>, String> {
    let raw = String::from_utf8_lossy(bytes);
    let trimmed = raw.trim();
    let value = if trimmed.len() > 1 { trimmed.trim_end_matches('/') } else { trimmed }.to_string();
    if value.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(&value);
    if !path.is_dir() {
        return Err("Выбранная папка не существует или недоступна.".into());
    }
    Ok(Some(path.display().to_string()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|path| path.join(name).is_file())
    })
}

#[derive(Debug, Deserialize)]
struct OpenPathRequest {
    path: String,
}

#[tauri::command]
fn open_in_file_manager(req: OpenPathRequest, app: tauri::AppHandle) -> Result<(), String> {
    let path = resolve_user_path(&app, &req.path)?;
    open_path_in_file_manager(&path)
}

/// Semantic extraction preview: deterministic engine plus (optional) type-validated
/// model output, returned with per-field method/confidence, plus the exact prompt to
/// send a semantic model. Read-only; performs no IO.
#[tauri::command]
fn semantic_extract(
    req: SemanticExtractRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let (domain_probe_case, _) = parse_source_text(&req.source_text, req.default_year);
    let model_domain = domain_probe_case
        .active_domains
        .first()
        .cloned()
        .unwrap_or(DomainKind::Generic);
    let semantic_model_config = load_semantic_model_config(&app)?;
    let live_model_outputs = if req.model_output.is_none() {
        if semantic_model_config.enabled {
            let transport = LocalSemanticModelTransport::new(&semantic_model_config)?;
            Some(transport.complete_many(&build_extraction_prompt_for_domain_and_language(
                &req.source_text,
                &model_domain,
                &semantic_model_config.preferred_language,
            ))?)
        } else {
            None
        }
    } else {
        None
    };
    let (mut extracted_case, mut report) = extract_understanding(
        &req.source_text,
        req.default_year,
        req.model_output.as_deref(),
    );
    if let Some(outputs) = live_model_outputs.as_deref() {
        report.warnings.extend(apply_model_consensus_with_source(
            &mut extracted_case,
            outputs,
            &req.source_text,
            req.default_year,
        ));
        for (field_id, value) in &extracted_case.values {
            if value.source == ValueSource::Model
                && !report.fields.iter().any(|field| field.field_id == *field_id)
            {
                report.fields.push(ExtractedField {
                    field_id: field_id.clone(),
                    value: value.value.clone(),
                    confidence: value.confidence,
                    method: "model-consensus".into(),
                });
            }
        }
    }
    for (field_id, value) in
        apply_learned_scanner_rules(&app, &req.source_text, &mut extracted_case)?
    {
        if !report.fields.iter().any(|field| field.field_id == field_id) {
            report.fields.push(ExtractedField {
                field_id,
                value,
                confidence: 0.88,
                method: "learned-scanner".into(),
            });
        }
    }
    let evidence_by_field = extracted_case
        .values
        .iter()
        .map(|(field_id, value)| {
            (
                field_id.clone(),
                (
                    value_source_label(value.source).to_string(),
                    value
                        .evidence
                        .iter()
                        .map(|evidence| evidence.excerpt.clone())
                        .filter(|excerpt| !excerpt.trim().is_empty())
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    transact_default_state(&app, &state, |snapshot| {
        merge_parsed_case(&mut snapshot.semantic_case, extracted_case)?;
        Ok(((), true))
    })?;
    report.fields.sort_by(|left, right| left.field_id.cmp(&right.field_id));
    report.warnings.sort();
    report.warnings.dedup();
    let model_applied = report
        .fields
        .iter()
        .any(|field| field.method.starts_with("model"));
    let fields: Vec<SemanticFieldDto> = report
        .fields
        .into_iter()
        .map(|f: ExtractedField| {
            let (source, evidence) = evidence_by_field
                .get(&f.field_id)
                .cloned()
                .unwrap_or_else(|| ("источник не указан".into(), Vec::new()));
            SemanticFieldDto {
                field_id: f.field_id,
                value: f.value,
                confidence: f.confidence,
                method: f.method,
                source,
                evidence,
            }
        })
        .collect();
    let response = SemanticExtractResponse {
        fields,
        warnings: report.warnings,
        model_applied,
        prompt: build_extraction_prompt_for_domain_and_language(
            &req.source_text,
            &model_domain,
            &semantic_model_config.preferred_language,
        ),
    };
    serde_json::to_value(response).map_err(|e| e.to_string())
}


#[cfg(test)]
mod publication_completion_receipt_tests {
    use super::*;

    #[test]
    fn local_completion_receipt_is_atomic_and_plan_bound() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-completion-receipt-{}",
            Uuid::new_v4()
        ));
        let job = "a".repeat(64);
        let source = "b".repeat(64);
        let plan = "c".repeat(64);
        let path = mark_local_completion(&root, &job, &source, &plan)
            .expect("persist local completion receipt");
        assert_eq!(path, local_completion_receipt(&root, &job));
        assert!(path.is_file());
        let body = std::fs::read_to_string(&path).expect("read local completion receipt");
        assert!(body.contains(&format!("processing_job_sha256={job}")));
        assert!(body.contains(&format!("source_sha256={source}")));
        assert!(body.contains(&format!("processing_fingerprint={plan}")));
        assert!(local_completion_receipt_matches(&root, &job, &source, &plan));
        std::fs::write(&path, b"schema=1\n").expect("corrupt local completion receipt");
        assert!(!local_completion_receipt_matches(&root, &job, &source, &plan));
        assert_ne!(
            local_completion_receipt(&root, &job),
            local_completion_receipt(&root, &"d".repeat(64))
        );
        std::fs::remove_dir_all(root).expect("cleanup completion receipt test root");
    }
}
