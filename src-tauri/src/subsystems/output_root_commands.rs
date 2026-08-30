const DEFAULT_OUTPUT_FOLDER_NAME: &str = "Выписанные пациенты";
const OUTPUT_PREFERENCES_STATE_KEY: &str = "output_preferences_v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutputPreferences {
    #[serde(default)]
    output_root: String,
    #[serde(default = "default_output_folder_parts")]
    folder_parts: Vec<FolderNamePart>,
    #[serde(default)]
    naming_confirmed: bool,
}

fn default_output_folder_parts() -> Vec<FolderNamePart> {
    vec![FolderNamePart::DocumentNumber, FolderNamePart::DocumentDate]
}

fn validate_output_folder_parts(parts: &[FolderNamePart]) -> Result<(), String> {
    if parts.is_empty() {
        return Err("Выберите хотя бы один компонент имени подпапки результата.".into());
    }
    let mut unique = Vec::<FolderNamePart>::new();
    for part in parts {
        if unique.contains(part) {
            return Err("Правило имени подпапки содержит повторяющийся компонент.".into());
        }
        unique.push(*part);
    }
    Ok(())
}

impl Default for OutputPreferences {
    fn default() -> Self {
        Self {
            output_root: String::new(),
            folder_parts: default_output_folder_parts(),
            naming_confirmed: false,
        }
    }
}

fn normalize_output_preferences(mut preferences: OutputPreferences) -> OutputPreferences {
    if preferences.folder_parts.is_empty() {
        preferences.folder_parts = default_output_folder_parts();
        preferences.naming_confirmed = false;
    }
    if preferences.output_root.trim().is_empty() {
        preferences.output_root.clear();
        preferences.naming_confirmed = false;
    }
    preferences
}

fn load_output_preferences_from_store(app: &tauri::AppHandle) -> Result<OutputPreferences, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let preferences = repo
        .load_state_value::<OutputPreferences>(OUTPUT_PREFERENCES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(normalize_output_preferences(preferences))
}

fn persist_output_preferences(app: &tauri::AppHandle, preferences: &OutputPreferences) -> Result<(), String> {
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(OUTPUT_PREFERENCES_STATE_KEY, preferences)
        .map_err(|error| error.to_string())
}

fn canonical_default_output_root_under(desktop: &Path) -> PathBuf {
    desktop.join(DEFAULT_OUTPUT_FOLDER_NAME)
}

fn canonical_default_output_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|error| format!("Не удалось определить папку рабочего стола: {error}"))?;
    Ok(canonical_default_output_root_under(&desktop))
}

fn startup_output_root_candidate(
    preferences: &OutputPreferences,
    canonical_default: PathBuf,
) -> PathBuf {
    if preferences.output_root.trim().is_empty() {
        canonical_default
    } else {
        PathBuf::from(preferences.output_root.trim())
    }
}

fn ensure_startup_output_root(app: &tauri::AppHandle) -> Result<String, String> {
    let mut preferences = load_output_preferences_from_store(app)?;
    let default_root = canonical_default_output_root(app)?;
    let candidate = startup_output_root_candidate(&preferences, default_root);
    let path = resolve_user_visible_absolute_path(
        &candidate.display().to_string(),
        "Папка готовых документов",
    )?;
    if let Err(error) = ensure_output_root_path(&path) {
        if !preferences.output_root.trim().is_empty() && preferences.naming_confirmed {
            preferences.naming_confirmed = false;
            persist_output_preferences(app, &preferences)?;
        }
        return Err(error);
    }
    let ensured = path.display().to_string();

    if preferences.output_root.trim().is_empty() {
        preferences.output_root = ensured.clone();
        preferences.naming_confirmed = false;
        persist_output_preferences(app, &preferences)?;
    }

    Ok(ensured)
}

fn verify_output_root_round_trip(path: &Path) -> Result<(), String> {
    use std::io::Write as _;

    let nonce = Uuid::new_v4();
    let staged = path.join(format!(".dokkomplekt-output-probe-{nonce}.tmp"));
    let renamed = path.join(format!(".dokkomplekt-output-probe-{nonce}.verified"));
    let payload = b"dokkomplekt-output-root-preflight-v1";
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .map_err(|error| format!("Папка недоступна для создания файлов: {error}"))?;
        file.write_all(payload)
            .map_err(|error| format!("Папка недоступна для записи: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Файловая система не подтвердила запись: {error}"))?;
        drop(file);
        std::fs::rename(&staged, &renamed)
            .map_err(|error| format!("Папка не поддержала безопасное переименование результата: {error}"))?;
        let read_back = std::fs::read(&renamed)
            .map_err(|error| format!("Не удалось прочитать записанный проверочный файл: {error}"))?;
        if read_back != payload {
            return Err("Проверочная запись в папке изменилась после сохранения.".into());
        }
        std::fs::remove_file(&renamed)
            .map_err(|error| format!("Не удалось удалить проверочный файл: {error}"))?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&staged);
    let _ = std::fs::remove_file(&renamed);
    result
}

fn ensure_output_root_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("Папка готовых документов не указана".into());
    }
    if !path.is_absolute() {
        return Err(format!(
            "Папка готовых документов должна быть абсолютным путём: {}",
            path.display()
        ));
    }
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "Не удалось создать папку готовых документов {}: {error}",
            path.display()
        )
    })?;
    if !path.is_dir() {
        return Err(format!(
            "Путь готовых документов не является папкой: {}",
            path.display()
        ));
    }
    // The write/sync/rename/read/delete round trip is the authority. It supports
    // valid redirected/junction/network folders while still failing closed when
    // publication semantics are not actually available.
    verify_output_root_round_trip(path)
}

#[derive(Debug, Deserialize)]
struct EnsureOutputRootRequest {
    output_root: String,
}

#[tauri::command]
fn get_default_output_root(app: tauri::AppHandle) -> Result<String, String> {
    Ok(canonical_default_output_root(&app)?.display().to_string())
}

#[tauri::command]
fn get_output_preferences(app: tauri::AppHandle) -> Result<OutputPreferences, String> {
    load_output_preferences_from_store(&app)
}

#[tauri::command]
fn save_output_preferences(
    req: OutputPreferences,
    app: tauri::AppHandle,
) -> Result<OutputPreferences, String> {
    validate_output_folder_parts(&req.folder_parts)?;
    let mut preferences = normalize_output_preferences(req);
    if !preferences.output_root.is_empty() {
        let path = resolve_user_visible_absolute_path(
            &preferences.output_root,
            "Папка готовых документов",
        )?;
        ensure_output_root_path(&path)?;
        preferences.output_root = path.display().to_string();
    }
    persist_output_preferences(&app, &preferences)?;
    Ok(preferences)
}

#[tauri::command]
fn ensure_output_root(req: EnsureOutputRootRequest) -> Result<String, String> {
    let path = resolve_user_visible_absolute_path(&req.output_root, "Папка готовых документов")?;
    ensure_output_root_path(&path)?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod default_output_root_contract_tests {
    use super::*;

    #[test]
    fn canonical_default_output_root_is_desktop_child() {
        let desktop = Path::new("C:/Users/Operator/Desktop");
        assert_eq!(
            canonical_default_output_root_under(desktop),
            desktop.join("Выписанные пациенты")
        );
    }

    #[test]
    fn startup_uses_canonical_root_only_until_a_durable_choice_exists() {
        let canonical = PathBuf::from("canonical-default");
        let empty = OutputPreferences::default();
        assert_eq!(
            startup_output_root_candidate(&empty, canonical.clone()),
            canonical
        );

        let selected = OutputPreferences {
            output_root: "D:/Doctor/Patients".into(),
            ..OutputPreferences::default()
        };
        assert_eq!(
            startup_output_root_candidate(&selected, PathBuf::from("ignored-default")),
            PathBuf::from("D:/Doctor/Patients")
        );
    }

    #[test]
    fn ensure_output_root_physically_verifies_full_file_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-output-root-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        assert!(!root.exists());
        ensure_output_root_path(&root).unwrap();
        assert!(root.is_dir());
        let leftovers = std::fs::read_dir(&root).unwrap().count();
        assert_eq!(leftovers, 0, "preflight must not leave probe files behind");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ensure_output_root_rejects_relative_paths() {
        assert!(ensure_output_root_path(Path::new("relative/output")).is_err());
    }

    #[test]
    fn empty_or_corrupt_preferences_restore_safe_folder_identity_defaults() {
        let empty = normalize_output_preferences(OutputPreferences {
            output_root: String::new(),
            folder_parts: Vec::new(),
            naming_confirmed: true,
        });
        assert_eq!(empty.folder_parts.len(), 2);
        assert!(!empty.naming_confirmed);
        assert!(matches!(empty.folder_parts[0], FolderNamePart::DocumentNumber));
        assert!(matches!(empty.folder_parts[1], FolderNamePart::DocumentDate));
    }

    #[test]
    fn persisted_output_rules_reject_empty_and_duplicate_parts() {
        assert!(validate_output_folder_parts(&[]).is_err());
        assert!(validate_output_folder_parts(&[
            FolderNamePart::DocumentNumber,
            FolderNamePart::DocumentNumber,
        ])
        .is_err());
        assert!(validate_output_folder_parts(&[
            FolderNamePart::DocumentNumber,
            FolderNamePart::DocumentDate,
        ])
        .is_ok());
    }

}
