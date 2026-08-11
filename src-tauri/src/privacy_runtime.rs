use crate::{default_state_db_path, repository_for, universal_intake, WorkspaceRetentionPolicy};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::Manager as _;

const PRIVACY_PREFERENCES_STATE_KEY: &str = "privacy_preferences_v1";
static LEARNING_WORKSPACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn lock_learning_workspace() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    LEARNING_WORKSPACE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "learning workspace lock failed".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PrivacyPreferences {
    pub(crate) copy_source_to_output: bool,
    pub(crate) write_trust_report: bool,
    pub(crate) include_values_in_trust_report: bool,
    pub(crate) temp_retention_hours: u32,
    pub(crate) archive_processed_sources: bool,
    pub(crate) archive_folder_name: String,
    pub(crate) service_note_retention_days: u32,
    pub(crate) processed_marker_retention_days: u32,
    pub(crate) archived_source_retention_days: u32,
}

impl Default for PrivacyPreferences {
    fn default() -> Self {
        let retention = WorkspaceRetentionPolicy::default();
        Self {
            copy_source_to_output: false,
            write_trust_report: true,
            include_values_in_trust_report: false,
            temp_retention_hours: 0,
            archive_processed_sources: retention.archive_processed_sources,
            archive_folder_name: retention.archive_folder_name,
            service_note_retention_days: retention.service_note_retention_days,
            processed_marker_retention_days: retention.processed_marker_retention_days,
            archived_source_retention_days: retention.archived_source_retention_days,
        }
    }
}

impl PrivacyPreferences {
    pub(crate) fn retention_policy(&self) -> WorkspaceRetentionPolicy {
        WorkspaceRetentionPolicy {
            archive_processed_sources: self.archive_processed_sources,
            archive_folder_name: self.archive_folder_name.clone(),
            service_note_retention_days: self.service_note_retention_days,
            processed_marker_retention_days: self.processed_marker_retention_days,
            archived_source_retention_days: self.archived_source_retention_days,
        }
    }
}

pub(crate) fn load_privacy_preferences(
    app: &tauri::AppHandle,
) -> Result<PrivacyPreferences, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    Ok(repo
        .load_state_value::<PrivacyPreferences>(PRIVACY_PREFERENCES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default())
}

pub(crate) fn persist_privacy_preferences(
    app: &tauri::AppHandle,
    preferences: &PrivacyPreferences,
) -> Result<(), String> {
    if preferences.temp_retention_hours > 24 * 30 {
        return Err("Срок хранения временных источников должен быть от 0 до 720 часов.".into());
    }
    preferences.retention_policy().validate()?;
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(PRIVACY_PREFERENCES_STATE_KEY, preferences)
        .map_err(|error| error.to_string())
}

pub(crate) fn cleanup_intake_workspace(app: &tauri::AppHandle) -> Result<usize, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    // Destructive cleanup must fail closed. If the user's privacy policy cannot
    // be loaded, deleting anything under a guessed/default policy is forbidden.
    let privacy = load_privacy_preferences(app)?;
    let max_age = Duration::from_secs(u64::from(privacy.temp_retention_hours) * 60 * 60);
    let mut removed = universal_intake::cleanup_workspace(&data_dir.join("intake-work"), max_age)?;
    // Learning imports and their normalized artifacts may contain the same
    // sensitive source data as ordinary intake. Serialize cleanup against active
    // learning commands, and never traverse outside these app-data-owned roots.
    let _learning_guard = lock_learning_workspace()?;
    for workspace in ["template-learning-inputs", "template-learning-work"] {
        removed = removed.saturating_add(universal_intake::cleanup_workspace(
            &data_dir.join(workspace),
            max_age,
        )?);
    }
    Ok(removed)
}

pub(crate) fn start_periodic_intake_cleanup(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5 * 60));
        if let Err(error) = cleanup_intake_workspace(&app) {
            eprintln!("Периодическая очистка временных источников пропущена: {error}");
        }
    });
}
