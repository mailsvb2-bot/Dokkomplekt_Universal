const DOCUMENT_SELECTION_STATE_KEY: &str = "document_selection_v1";

fn normalize_document_selection(pack: &DocumentPack, requested: &[String]) -> Vec<String> {
    let requested = requested
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    pack.documents
        .iter()
        .filter(|document| requested.contains(document.id.as_str()))
        .map(|document| document.id.clone())
        .collect()
}

fn persisted_document_selection(
    app: &tauri::AppHandle,
    pack: &DocumentPack,
) -> Result<Vec<String>, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let stored = repo
        .load_state_value::<Vec<String>>(DOCUMENT_SELECTION_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(normalize_document_selection(pack, &stored))
}

#[derive(Debug, Serialize)]
struct FirstRunStateResponse {
    pack: DocumentPack,
    has_user_buttons: bool,
    selected_document_ids: Vec<String>,
    message: String,
}

#[tauri::command]
fn first_run_state(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FirstRunStateResponse, String> {
    ensure_default_state_loaded(&app, &state)?;
    if state.persistence_blocked.load(Ordering::SeqCst) {
        let reason = state
            .persistence_error
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| "неизвестная ошибка базы состояния".into());
        return Err(format!(
            "Восстановление состояния заблокировано для защиты данных: {reason}. Загрузите исправную резервную базу; текущие данные не будут перезаписаны."
        ));
    }
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let has_user_buttons = !pack.documents.is_empty();
    let message = if has_user_buttons {
        "Рабочий комплект загружен. Можно положить первичный документ в папку автоматизации.".into()
    } else {
        "Первоначальная настройка: нажмите «Создать свои кнопки» и выберите реальные рабочие шаблоны. Программа сама определит рабочий профиль по всему набору; профессию выбирать не нужно.".into()
    };
    let selected_document_ids = persisted_document_selection(&app, &pack)?;
    Ok(FirstRunStateResponse {
        has_user_buttons,
        pack,
        selected_document_ids,
        message,
    })
}

#[derive(Debug, Deserialize)]
struct SetDocumentSelectionRequest {
    document_ids: Vec<String>,
}

#[tauri::command]
fn set_document_selection(
    req: SetDocumentSelectionRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<String>, String> {
    ensure_default_state_loaded(&app, &state)?;
    ensure_persistence_available(&state)?;
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let requested = req
        .document_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>();
    let normalized = normalize_document_selection(&pack, &req.document_ids);
    if normalized.len() != requested.len() {
        return Err("Выбор документов содержит кнопку, которой нет в текущем рабочем наборе.".into());
    }
    let path = default_state_db_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    repository_for(&path)?
        .save_state_value(DOCUMENT_SELECTION_STATE_KEY, &normalized)
        .map_err(|error| error.to_string())?;
    Ok(normalized)
}
