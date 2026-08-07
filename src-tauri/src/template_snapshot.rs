use crate::{resolve_user_path, universal_intake, MAX_DOCX_BYTES};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::Manager as _;

#[derive(Debug)]
pub(crate) struct TemplateSnapshot {
    live_path: PathBuf,
    snapshot: universal_intake::StableSourceSnapshot,
    label: String,
}

impl TemplateSnapshot {
    pub(crate) fn capture(
        app: &tauri::AppHandle,
        configured_path: &str,
        label: &str,
    ) -> Result<Self, String> {
        let live_path = resolve_user_path(app, configured_path)?;
        let extension = live_path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "docx" | "docm") {
            return Err(format!(
                "Шаблон «{label}» должен быть DOCX или DOCM: {}",
                live_path.display()
            ));
        }
        let metadata = std::fs::metadata(&live_path)
            .map_err(|error| format!("Не удалось прочитать шаблон «{label}»: {error}"))?;
        if !metadata.is_file() {
            return Err(format!(
                "Шаблон «{label}» не является файлом: {}",
                live_path.display()
            ));
        }
        if metadata.len() > MAX_DOCX_BYTES as u64 {
            return Err(format!(
                "Шаблон «{label}» превышает безопасный предел {} МБ.",
                MAX_DOCX_BYTES / (1024 * 1024)
            ));
        }
        let workspace = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join("template-snapshot-work");
        Self::capture_path(&live_path, &workspace, label)
    }

    fn capture_path(live_path: &Path, workspace: &Path, label: &str) -> Result<Self, String> {
        let snapshot = universal_intake::capture_stable_source(live_path, workspace)
            .map_err(|error| format!("Не удалось стабилизировать шаблон «{label}»: {error}"))?;
        Ok(Self {
            live_path: live_path.to_path_buf(),
            snapshot,
            label: label.to_string(),
        })
    }

    pub(crate) fn live_path(&self) -> &Path {
        &self.live_path
    }

    pub(crate) fn path(&self) -> &Path {
        self.snapshot.path()
    }

    pub(crate) fn sha256(&self) -> &str {
        self.snapshot.sha256()
    }

    pub(crate) fn ensure_current(&self) -> Result<(), String> {
        match universal_intake::current_source_matches(&self.live_path, self.sha256()) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!(
                "Шаблон «{}» изменился во время операции. Результат из устаревшей версии не опубликован; повторите действие после завершения редактирования шаблона.",
                self.label
            )),
            Err(error) => Err(format!(
                "Не удалось повторно проверить шаблон «{}» перед публикацией: {error}",
                self.label
            )),
        }
    }
}

pub(crate) fn ensure_all_current(
    snapshots: &BTreeMap<String, TemplateSnapshot>,
) -> Result<(), String> {
    for snapshot in snapshots.values() {
        snapshot.ensure_current()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn immutable_template_snapshot_detects_live_replacement() {
        let root = std::env::temp_dir().join(format!("dkk-template-snapshot-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let live = root.join("template.docx");
        std::fs::write(&live, b"template-version-one").unwrap();

        let snapshot = TemplateSnapshot::capture_path(&live, &workspace, "Тест").unwrap();
        assert_eq!(
            std::fs::read(snapshot.path()).unwrap(),
            b"template-version-one"
        );
        snapshot.ensure_current().unwrap();

        std::fs::write(&live, b"template-version-two").unwrap();
        assert!(snapshot.ensure_current().is_err());
        assert_eq!(
            std::fs::read(snapshot.path()).unwrap(),
            b"template-version-one"
        );

        drop(snapshot);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_live_template_is_not_current() {
        let root = std::env::temp_dir().join(format!("dkk-template-missing-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let live = root.join("template.docx");
        std::fs::write(&live, b"stable-template").unwrap();

        let snapshot = TemplateSnapshot::capture_path(&live, &workspace, "Тест").unwrap();
        std::fs::remove_file(&live).unwrap();
        assert!(snapshot.ensure_current().is_err());

        drop(snapshot);
        let _ = std::fs::remove_dir_all(root);
    }
}
