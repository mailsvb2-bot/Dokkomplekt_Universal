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
    /// v1 installations enabled the trust report implicitly. That made an
    /// ancillary audit artifact part of the critical publication path: after a
    /// restart the semantic case could be restored without in-memory source
    /// provenance, so manual generation rendered DOCX into staging and then
    /// discarded the whole stage while trying to build the report.
    ///
    /// New and migrated installations therefore treat the report as explicit
    /// opt-in. The flag is persisted only after the user saves privacy settings,
    /// so an intentional future opt-in is preserved without resurrecting the
    /// legacy fail-closed default.
    #[serde(default)]
    pub(crate) trust_report_explicit: bool,
}

impl Default for PrivacyPreferences {
    fn default() -> Self {
        let retention = WorkspaceRetentionPolicy::default();
        Self {
            copy_source_to_output: false,
            write_trust_report: false,
            include_values_in_trust_report: false,
            temp_retention_hours: 0,
            archive_processed_sources: retention.archive_processed_sources,
            archive_folder_name: retention.archive_folder_name,
            service_note_retention_days: retention.service_note_retention_days,
            processed_marker_retention_days: retention.processed_marker_retention_days,
            archived_source_retention_days: retention.archived_source_retention_days,
            trust_report_explicit: false,
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

fn normalize_loaded_privacy_preferences(mut preferences: PrivacyPreferences) -> PrivacyPreferences {
    if !preferences.trust_report_explicit {
        preferences.write_trust_report = false;
    }
    preferences
}

pub(crate) fn load_privacy_preferences(
    app: &tauri::AppHandle,
) -> Result<PrivacyPreferences, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let loaded = repo
        .load_state_value::<PrivacyPreferences>(PRIVACY_PREFERENCES_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    Ok(normalize_loaded_privacy_preferences(loaded))
}

pub(crate) fn persist_privacy_preferences(
    app: &tauri::AppHandle,
    preferences: &PrivacyPreferences,
) -> Result<(), String> {
    if preferences.temp_retention_hours > 24 * 30 {
        return Err("Срок хранения временных источников должен быть от 0 до 720 часов.".into());
    }
    preferences.retention_policy().validate()?;
    let mut persisted = preferences.clone();
    persisted.trust_report_explicit = true;
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(PRIVACY_PREFERENCES_STATE_KEY, &persisted)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_report_is_not_part_of_default_document_publication() {
        let preferences = PrivacyPreferences::default();
        assert!(!preferences.write_trust_report);
        assert!(!preferences.trust_report_explicit);
    }

    #[test]
    fn legacy_implicit_trust_report_is_migrated_off() {
        let legacy = PrivacyPreferences {
            write_trust_report: true,
            trust_report_explicit: false,
            ..PrivacyPreferences::default()
        };
        let migrated = normalize_loaded_privacy_preferences(legacy);
        assert!(!migrated.write_trust_report);
    }

    #[test]
    fn explicit_trust_report_choice_is_preserved() {
        let explicit = PrivacyPreferences {
            write_trust_report: true,
            trust_report_explicit: true,
            ..PrivacyPreferences::default()
        };
        let loaded = normalize_loaded_privacy_preferences(explicit);
        assert!(loaded.write_trust_report);
    }
}