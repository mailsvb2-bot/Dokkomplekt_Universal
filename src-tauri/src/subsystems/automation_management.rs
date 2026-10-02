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
