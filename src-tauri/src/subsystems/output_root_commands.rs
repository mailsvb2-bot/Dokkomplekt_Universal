const DEFAULT_OUTPUT_FOLDER_NAME: &str = "Выписанные пациенты";

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
    Ok(())
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
fn ensure_output_root(req: EnsureOutputRootRequest) -> Result<String, String> {
    let trimmed = req.output_root.trim();
    let path = PathBuf::from(trimmed);
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
    fn ensure_output_root_physically_creates_missing_directory() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-output-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(!root.exists());
        ensure_output_root_path(&root).unwrap();
        assert!(root.is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }
}
