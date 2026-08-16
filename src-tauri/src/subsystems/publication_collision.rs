//! Explicit output-collision publication policies.

fn publish_stage_replacing_with_backup(
    stage: &Path,
    desired: &Path,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parent = desired.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let lock_path = parent.join(format!(
        ".dokkomplekt-dir-replace-{}.lock",
        sanitize_path_component(
            desired
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("output")
        )
    ));
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "Не удалось получить эксклюзивную блокировку безопасной замены комплекта: {error}"
            )
        })?;

    let result = (|| -> Result<(PathBuf, Option<PathBuf>), String> {
        if !desired.exists() {
            std::fs::rename(stage, desired).map_err(|error| {
                format!("Не удалось опубликовать новый комплект: {error}")
            })?;
            return Ok((desired.to_path_buf(), None));
        }
        if !desired.is_dir() {
            return Err(format!(
                "Нельзя безопасно заменить результат: {} существует и не является папкой.",
                desired.display()
            ));
        }

        let backup_root = parent.join(".dokkomplekt-backups");
        std::fs::create_dir_all(&backup_root)
            .map_err(|error| format!("Не удалось создать каталог резервных копий: {error}"))?;
        let stem = sanitize_path_component(
            desired
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Комплект"),
        );
        let backup = backup_root.join(format!(
            "{stem}.backup-{}-{}",
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Uuid::new_v4()
        ));

        std::fs::rename(desired, &backup).map_err(|error| {
            format!(
                "Не удалось сначала сохранить существующий комплект в резервную копию: {error}"
            )
        })?;
        match std::fs::rename(stage, desired) {
            Ok(()) => Ok((desired.to_path_buf(), Some(backup))),
            Err(publish_error) => {
                match std::fs::rename(&backup, desired) {
                    Ok(()) => Err(format!(
                        "Новый комплект не опубликован ({publish_error}); предыдущая версия восстановлена."
                    )),
                    Err(rollback_error) => Err(format!(
                        "КРИТИЧЕСКАЯ ОШИБКА безопасной замены: новый комплект не опубликован ({publish_error}), а автоматическое восстановление старой папки не удалось ({rollback_error}). Резервная копия сохранена по пути {}.",
                        backup.display()
                    )),
                }
            }
        }
    })();

    drop(lock);
    let _ = std::fs::remove_file(&lock_path);
    result
}


#[cfg(test)]
mod publication_collision_tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dokkomplekt-{label}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    #[test]
    fn replace_with_backup_never_destroys_previous_directory_before_backup() {
        let root = temp_root("replace-with-backup");
        let desired = root.join("Комплект");
        let stage = root.join(".stage");
        std::fs::create_dir_all(&desired).unwrap();
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(desired.join("old.txt"), "old").unwrap();
        std::fs::write(stage.join("new.txt"), "new").unwrap();

        let (published, backup) = publish_stage_replacing_with_backup(&stage, &desired).unwrap();
        let backup = backup.expect("existing target must be backed up");
        assert_eq!(published, desired);
        assert_eq!(std::fs::read_to_string(desired.join("new.txt")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(backup.join("old.txt")).unwrap(), "old");
        assert!(!desired.join("old.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replace_policy_without_existing_target_publishes_without_backup() {
        let root = temp_root("replace-new-target");
        let desired = root.join("Комплект");
        let stage = root.join(".stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("new.txt"), "new").unwrap();

        let (published, backup) = publish_stage_replacing_with_backup(&stage, &desired).unwrap();
        assert_eq!(published, desired);
        assert!(backup.is_none());
        assert_eq!(std::fs::read_to_string(desired.join("new.txt")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(root);
    }
}
