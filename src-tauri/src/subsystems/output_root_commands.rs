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
}
