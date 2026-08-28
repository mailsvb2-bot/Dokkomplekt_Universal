fn persistence_restore_error(state: &AppState) -> Option<String> {
    if !state.persistence_blocked.load(Ordering::SeqCst) {
        return None;
    }
    Some(
        state
            .persistence_error
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| "неизвестная ошибка базы состояния".into()),
    )
}

fn mark_default_state_restore_failure(
    app: &tauri::AppHandle,
    state: &AppState,
    db_path: &Path,
    error: &str,
) {
    state.persistence_blocked.store(true, Ordering::SeqCst);
    if let Ok(mut slot) = state.persistence_error.lock() {
        *slot = Some(error.to_string());
    }
    let marker = db_path.with_extension("recovery-required.txt");
    let message = format!(
        "Доккомплект не изменял повреждённую базу состояния.\nПуть: {}\nОшибка: {}\nЗагрузите исправную резервную базу через интерфейс.\n",
        db_path.display(), error
    );
    let _ = std::fs::write(marker, message);
    let _ = app.emit("state-recovery-required", error);
}

fn ensure_default_state_loaded(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    if let Some(reason) = persistence_restore_error(state) {
        return Err(format!(
            "Восстановление состояния заблокировано для защиты данных: {reason}. Загрузите исправную резервную базу; текущие данные не будут перезаписаны."
        ));
    }
    if state.db_path.lock().map_err(|_| "state lock failed")?.is_some() {
        return Ok(());
    }

    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    if let Some(reason) = persistence_restore_error(state) {
        return Err(format!(
            "Восстановление состояния заблокировано для защиты данных: {reason}. Загрузите исправную резервную базу; текущие данные не будут перезаписаны."
        ));
    }
    if state.db_path.lock().map_err(|_| "state lock failed")?.is_some() {
        return Ok(());
    }

    let db_path = default_state_db_path(app)?;
    if !db_path.exists() {
        *state.db_path.lock().map_err(|_| "state lock failed")? = Some(db_path);
        return Ok(());
    }

    match load_state_from_locked(app, &db_path, state, true) {
        Ok(()) => Ok(()),
        Err(error) => {
            mark_default_state_restore_failure(app, state, &db_path, &error);
            Err(format!(
                "Восстановление состояния заблокировано для защиты данных: {error}. Загрузите исправную резервную базу; текущие данные не будут перезаписаны."
            ))
        }
    }
}
