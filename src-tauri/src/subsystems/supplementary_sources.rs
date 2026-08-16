#[derive(Debug, Clone, Serialize)]
struct SupplementarySourceDto {
    source_id: String,
    role: String,
    name: String,
    source_kind: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct SupplementarySourcesResponse {
    sources: Vec<SupplementarySourceDto>,
    semantic_case: SemanticCase,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AttachSupplementaryFileRequest {
    role: String,
    file_name: String,
    bytes_base64: String,
    #[serde(default)]
    relative_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AttachSupplementaryFolderRequest {
    role: String,
    folder_path: String,
}

#[derive(Debug, Deserialize)]
struct RemoveSupplementarySourceRequest {
    source_id: String,
}

fn normalize_supplementary_role(value: &str) -> Result<String, String> {
    let role = value.trim().to_lowercase();
    if role.is_empty()
        || role.len() > 120
        || !role
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("Некорректная роль дополнительного источника.".into());
    }
    Ok(role)
}

fn supplementary_source_dto(source: &dokkomplekt_core::SupplementarySourceSpec) -> SupplementarySourceDto {
    SupplementarySourceDto {
        source_id: source.source_id.clone(),
        role: source.role.clone(),
        name: source.name.clone(),
        source_kind: source.source_kind.clone(),
        path: source.path.clone(),
    }
}

fn supplementary_response(case: &SemanticCase, warnings: Vec<String>) -> SupplementarySourcesResponse {
    SupplementarySourcesResponse {
        sources: dokkomplekt_core::supplementary_sources(case, None)
            .iter()
            .map(supplementary_source_dto)
            .collect(),
        semantic_case: case.clone(),
        warnings,
    }
}

fn apply_supplementary_source(case: &mut SemanticCase, source: &dokkomplekt_core::SupplementarySourceSpec) {
    dokkomplekt_core::upsert_supplementary_source(case, source);
    if source.role == "medical.diary_texts" {
        dokkomplekt_core::upsert_medical_diary_text_source(
            case,
            &source.source_id,
            &source.name,
            &source.text,
        );
    }
}

fn normalized_source_from_path(
    app: &tauri::AppHandle,
    path: &Path,
    role: &str,
    source_id: String,
    display_name: String,
) -> Result<dokkomplekt_core::SupplementarySourceSpec, String> {
    let work = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("supplementary-normalized")
        .join(&source_id);
    let normalized = universal_intake::normalize_path(path, &work, 0)?;
    Ok(dokkomplekt_core::SupplementarySourceSpec {
        source_id,
        role: role.to_string(),
        name: display_name,
        source_kind: normalized.source_kind,
        text: normalized.text,
        path: path.display().to_string(),
    })
}


fn cleanup_owned_supplementary_files(app: &tauri::AppHandle) {
    let Ok(app_data) = app.path().app_data_dir() else {
        return;
    };
    for relative in ["supplementary-sources/current", "supplementary-normalized"] {
        let path = app_data.join(relative);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[tauri::command]
fn list_supplementary_sources(state: State<'_, AppState>) -> Result<SupplementarySourcesResponse, String> {
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    Ok(supplementary_response(&case, Vec::new()))
}

#[tauri::command]
fn attach_supplementary_file(
    req: AttachSupplementaryFileRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SupplementarySourcesResponse, String> {
    let role = normalize_supplementary_role(&req.role)?;
    let bytes = universal_intake::decode_uploaded_payload(&req.file_name, &req.bytes_base64)?;
    let source_id = Uuid::new_v4().to_string();
    let safe_name = sanitize_path_component(
        Path::new(&req.file_name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source"),
    );
    if safe_name.is_empty() {
        return Err("Имя дополнительного файла некорректно.".into());
    }
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("supplementary-sources")
        .join("current");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let target = root.join(format!("{}-{}", &source_id[..8], safe_name));
    atomic_write_file(&target, &bytes)?;
    let display_name = req
        .relative_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&req.file_name)
        .to_string();
    let source = normalized_source_from_path(&app, &target, &role, source_id, display_name)?;
    transact_default_state(&app, &state, |snapshot| {
        apply_supplementary_source(&mut snapshot.semantic_case, &source);
        Ok((supplementary_response(&snapshot.semantic_case, Vec::new()), true))
    })
}

fn collect_supplementary_folder_files(
    root: &Path,
    current: &Path,
    depth: usize,
    role: &str,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > 3 || out.len() >= 200 {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(current)
        .map_err(|error| format!("Не удалось прочитать папку дополнительных материалов: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if out.len() >= 200 {
            break;
        }
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_supplementary_folder_files(root, &path, depth + 1, role, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let accepted = if role == "medical.diary_dates" {
            matches!(extension.as_str(), "docx" | "docm")
        } else if role == "medical.diary_texts" {
            matches!(extension.as_str(), "docx" | "docm" | "txt" | "rtf")
        } else {
            dokkomplekt_core::is_supported_intake_extension(&extension)
        };
        if accepted && path.starts_with(root) {
            out.push(path);
        }
    }
    Ok(())
}

#[tauri::command]
fn attach_supplementary_folder(
    req: AttachSupplementaryFolderRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SupplementarySourcesResponse, String> {
    let role = normalize_supplementary_role(&req.role)?;
    let folder = resolve_user_path(&app, &req.folder_path)?;
    if !folder.is_dir() {
        return Err("Выбранная папка дополнительных материалов не найдена.".into());
    }
    let mut files = Vec::new();
    collect_supplementary_folder_files(&folder, &folder, 0, &role, &mut files)?;
    if files.is_empty() {
        return Err("В выбранной папке нет поддерживаемых файлов для этого источника.".into());
    }
    let truncated = files.len() >= 200;
    let mut sources = Vec::new();
    let mut warnings = Vec::new();
    for path in files {
        let relative = path
            .strip_prefix(&folder)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let source_id = Uuid::new_v4().to_string();
        match normalized_source_from_path(&app, &path, &role, source_id, relative) {
            Ok(source) => sources.push(source),
            Err(error) => warnings.push(format!("{}: {error}", path.display())),
        }
    }
    if sources.is_empty() {
        return Err(format!(
            "Не удалось прочитать файлы папки: {}",
            warnings.join("; ")
        ));
    }
    if truncated {
        warnings.push("За один импорт обрабатывается не более 200 файлов.".into());
    }
    transact_default_state(&app, &state, |snapshot| {
        for source in &sources {
            apply_supplementary_source(&mut snapshot.semantic_case, source);
        }
        Ok((supplementary_response(&snapshot.semantic_case, warnings.clone()), true))
    })
}

#[tauri::command]
fn remove_supplementary_source(
    req: RemoveSupplementarySourceRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<SupplementarySourcesResponse, String> {
    let source_id = req.source_id.trim().to_string();
    if source_id.is_empty() {
        return Err("Не указан дополнительный источник.".into());
    }
    let existing = {
        let case = state.semantic_case.lock().map_err(|_| "state lock failed")?;
        dokkomplekt_core::supplementary_sources(&case, None)
            .into_iter()
            .find(|source| source.source_id == source_id)
    };
    let response = transact_default_state(&app, &state, |snapshot| {
        let changed = dokkomplekt_core::remove_supplementary_source(
            &mut snapshot.semantic_case,
            &source_id,
        );
        dokkomplekt_core::remove_medical_diary_text_source(&mut snapshot.semantic_case, &source_id);
        Ok((supplementary_response(&snapshot.semantic_case, Vec::new()), changed))
    })?;
    if let Some(source) = existing {
        if !source.path.trim().is_empty() {
            let path = PathBuf::from(&source.path);
            let owned_root = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("supplementary-sources")
                .join("current");
            if path.starts_with(&owned_root) {
                let _ = std::fs::remove_file(path);
            }
        }
        if let Ok(app_data) = app.path().app_data_dir() {
            let normalized = app_data.join("supplementary-normalized").join(&source_id);
            let _ = std::fs::remove_dir_all(normalized);
        }
    }
    Ok(response)
}

fn supplementary_template_path_for_document(
    document: &DocumentTemplateSpec,
    case: &SemanticCase,
) -> Result<Option<String>, String> {
    if document.category != DomainKind::Medical
        || dokkomplekt_core::domains::medical::canonical_medical_role(&document.role_id) != "diaries"
    {
        return Ok(None);
    }
    let sources = dokkomplekt_core::supplementary_sources(case, Some("medical.diary_dates"));
    if sources.is_empty() {
        return Ok(None);
    }
    let usable = sources
        .into_iter()
        .filter(|source| !source.path.trim().is_empty())
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Err("Для «Даты» не найден ни один доступный шаблон.".into());
    }
    if usable.len() == 1 {
        return Ok(Some(usable[0].path.clone()));
    }
    let admission = case
        .get("medical.admission_date")
        .ok_or_else(|| "Для выбора шаблона «Даты» нужна дата поступления.".to_string())?;
    let year = current_year_utc();
    let normalized = dokkomplekt_core::parse_flexible_date(admission, year)
        .ok_or_else(|| "Дата поступления некорректна; шаблон «Даты» не выбран.".to_string())?;
    let day = normalized
        .split('.')
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| "Не удалось определить день поступления для шаблона «Даты».".to_string())?;
    let names = usable
        .iter()
        .map(|source| {
            Path::new(&source.name)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(source.name.as_str())
                .to_string()
        })
        .collect::<Vec<_>>();
    let selection = dokkomplekt_core::select_diary_template_for_admission(&names, day)
        .ok_or_else(|| {
            format!(
                "В источнике «Даты» нет шаблона для дня {day:02} (или legacy D0+1)."
            )
        })?;
    usable
        .iter()
        .zip(names.iter())
        .find(|(_, name)| **name == selection.file_name)
        .map(|(source, _)| Some(source.path.clone()))
        .ok_or_else(|| "Выбранный шаблон «Даты» потерян из списка источников.".to_string())
}
