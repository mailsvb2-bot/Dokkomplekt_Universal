use crate::{default_state_db_path, repository_for, universal_intake, WorkspaceRetentionPolicy};
use serde::{Deserialize, Serialize};
use std::path::Path;
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TechnicalStorageCategory {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) bytes: u64,
    pub(crate) retention_managed: bool,
    pub(crate) quota_bytes: Option<u64>,
    pub(crate) retention_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TechnicalStorageStatus {
    pub(crate) total_bytes: u64,
    pub(crate) retention_managed_bytes: u64,
    pub(crate) categories: Vec<TechnicalStorageCategory>,
}

const MIB: u64 = 1024 * 1024;
const MANUAL_BATCH_MIN_OUTPUT_BYTES_PER_DOCUMENT: u64 = 8 * MIB;
const MANUAL_BATCH_OUTPUT_EXPANSION_FACTOR: u64 = 3;
const MANUAL_BATCH_MIN_RECOVERY_RESERVE_BYTES: u64 = 64 * MIB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManualBatchStorageEstimate {
    pub(crate) staged_outputs_bytes: u64,
    pub(crate) transient_render_bytes: u64,
    pub(crate) retained_source_bytes: u64,
    pub(crate) recovery_reserve_bytes: u64,
    pub(crate) required_free_bytes: u64,
}

fn checked_capacity_add(left: u64, right: u64) -> Result<u64, String> {
    left.checked_add(right)
        .ok_or_else(|| "Оценка требуемого места для комплекта переполнена.".to_string())
}

fn estimate_manual_batch_storage(
    documents: &[(u64, u64)],
    retained_source_bytes: u64,
) -> Result<ManualBatchStorageEstimate, String> {
    if documents.is_empty() {
        return Err("Нельзя оценить место для пустого комплекта документов.".into());
    }

    let mut staged_outputs_bytes = 0_u64;
    let mut transient_render_bytes = 0_u64;
    for (template_bytes, image_field_count) in documents {
        let expanded = template_bytes
            .checked_mul(MANUAL_BATCH_OUTPUT_EXPANSION_FACTOR)
            .ok_or_else(|| {
                "Размер шаблона слишком велик для безопасной оценки места.".to_string()
            })?;
        let image_budget = image_field_count
            .checked_mul(dokkomplekt_docx::MAX_IMAGE_ASSET_BYTES)
            .ok_or_else(|| {
                "Количество изображений слишком велико для безопасной оценки места.".to_string()
            })?;
        let estimated_output = checked_capacity_add(
            expanded.max(MANUAL_BATCH_MIN_OUTPUT_BYTES_PER_DOCUMENT),
            image_budget,
        )?;
        staged_outputs_bytes = checked_capacity_add(staged_outputs_bytes, estimated_output)?;
        transient_render_bytes = transient_render_bytes.max(estimated_output);
    }

    // Rendering uses a sibling temporary DOCX before the final staged file is committed,
    // so peak usage includes one additional largest-output allowance. Recovery also needs
    // headroom while the previous published directory may still exist.
    let recovery_reserve_bytes =
        MANUAL_BATCH_MIN_RECOVERY_RESERVE_BYTES.max(staged_outputs_bytes / 4);
    let required_free_bytes = checked_capacity_add(
        checked_capacity_add(
            checked_capacity_add(staged_outputs_bytes, transient_render_bytes)?,
            retained_source_bytes,
        )?,
        recovery_reserve_bytes,
    )?;

    Ok(ManualBatchStorageEstimate {
        staged_outputs_bytes,
        transient_render_bytes,
        retained_source_bytes,
        recovery_reserve_bytes,
        required_free_bytes,
    })
}

fn capacity_mib_ceil(bytes: u64) -> u64 {
    bytes.saturating_add(MIB - 1) / MIB
}

fn require_available_capacity(
    estimate: ManualBatchStorageEstimate,
    available_bytes: u64,
) -> Result<ManualBatchStorageEstimate, String> {
    if available_bytes < estimate.required_free_bytes {
        return Err(format!(
            "Недостаточно свободного места для безопасного создания комплекта: требуется не менее {} МБ (включая {} МБ резерва для commit/recovery), доступно {} МБ. Генерация остановлена до создания временного комплекта.",
            capacity_mib_ceil(estimate.required_free_bytes),
            capacity_mib_ceil(estimate.recovery_reserve_bytes),
            capacity_mib_ceil(available_bytes),
        ));
    }
    Ok(estimate)
}

pub(crate) fn ensure_manual_batch_storage_capacity(
    stage_parent: &Path,
    documents: &[(u64, u64)],
    retained_source_bytes: u64,
) -> Result<(), String> {
    let estimate = estimate_manual_batch_storage(documents, retained_source_bytes)?;
    let available_bytes = fs2::available_space(stage_parent).map_err(|error| {
        format!(
            "Не удалось проверить свободное место в {}: {error}. Комплект не создаётся без проверки диска.",
            stage_parent.display()
        )
    })?;
    require_available_capacity(estimate, available_bytes).map(|_| ())
}

impl Default for PrivacyPreferences {
    fn default() -> Self {
        let retention = WorkspaceRetentionPolicy::default();
        Self {
            // A primary document dropped into the created-documents intake is a
            // user document, not a service artifact. The canonical publication
            // contract therefore keeps an immutable copy in the patient folder
            // by default before the original top-level source is finalized.
            copy_source_to_output: true,
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
    // Pre-parity installations could persist the old opt-out value. Keeping it
    // would make a successfully processed dropped primary disappear from the
    // user's patient folder after upgrade. Source placement is now a canonical
    // created-documents invariant; retention/archiving of the original remains
    // independently configurable below.
    preferences.copy_source_to_output = true;
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
    // The dropped primary belongs to the patient document set. Do not allow an
    // old UI/state payload to silently turn this user-visible invariant off.
    persisted.copy_source_to_output = true;
    persisted.trust_report_explicit = true;
    repository_for(&default_state_db_path(app)?)?
        .save_state_value(PRIVACY_PREFERENCES_STATE_KEY, &persisted)
        .map_err(|error| error.to_string())
}

fn owned_path_size(path: &Path) -> Result<u64, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "Не удалось определить размер {}: {error}",
                path.display()
            ))
        }
    };
    if crate::publication_metadata_is_link_or_reparse(&metadata) {
        return Err(format!(
            "Подсчёт технических данных заблокирован: {} является ссылкой/reparse point.",
            path.display()
        ));
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path).map_err(|error| {
        format!(
            "Не удалось прочитать {} для подсчёта размера: {error}",
            path.display()
        )
    })? {
        let entry = entry.map_err(|error| error.to_string())?;
        total = total.saturating_add(owned_path_size(&entry.path())?);
    }
    Ok(total)
}

pub(crate) fn collect_technical_storage_status(
    app: &tauri::AppHandle,
) -> Result<TechnicalStorageStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let privacy = load_privacy_preferences(app)?;
    let temp_retention_seconds = u64::from(privacy.temp_retention_hours) * 60 * 60;
    let managed = [
        (
            "intake-work",
            "Временные исходники",
            None,
            Some(temp_retention_seconds),
        ),
        (
            "template-learning-inputs",
            "Входы обучения шаблонов",
            None,
            Some(temp_retention_seconds),
        ),
        (
            "template-learning-work",
            "Рабочие данные обучения",
            None,
            Some(temp_retention_seconds),
        ),
        (
            "runtime-logs",
            "Журналы фонового агента",
            Some(crate::watcher_log::WATCHER_LOG_TOTAL_QUOTA_BYTES),
            Some(crate::watcher_log::WATCHER_LOG_RETENTION_SECONDS),
        ),
        (
            "template-compiler-cache",
            "Кэш compiler шаблонов",
            Some(crate::TEMPLATE_COMPILER_CACHE_QUOTA_BYTES),
            Some(crate::TEMPLATE_COMPILER_CACHE_RETENTION_SECONDS),
        ),
        (
            "word-scanner-cache",
            "Кэш Word-сканера",
            Some(crate::WORD_SCANNER_CACHE_QUOTA_BYTES),
            Some(crate::WORD_SCANNER_CACHE_RETENTION_SECONDS),
        ),
    ];
    let mut categories = Vec::with_capacity(managed.len() + 1);
    let mut retention_managed_bytes = 0_u64;
    for (key, label, quota_bytes, retention_seconds) in managed {
        let bytes = if key == "runtime-logs" {
            crate::watcher_log::owned_watcher_log_bytes(
                &data_dir.join("runtime-logs").join("watcher.log"),
            )?
        } else if key == "template-compiler-cache" {
            universal_intake::owned_workspace_group_bytes(
                &[
                    "template-contract-migration",
                    "template-render-inference",
                    "template-inference-work",
                ]
                .into_iter()
                .map(|workspace| data_dir.join(workspace))
                .collect::<Vec<_>>(),
            )?
        } else if key == "word-scanner-cache" {
            universal_intake::owned_workspace_bytes(&data_dir.join("word-scanner-work"))?
        } else {
            owned_path_size(&data_dir.join(key))?
        };
        retention_managed_bytes = retention_managed_bytes.saturating_add(bytes);
        categories.push(TechnicalStorageCategory {
            key: key.into(),
            label: label.into(),
            bytes,
            retention_managed: true,
            quota_bytes,
            retention_seconds,
        });
    }
    let total_bytes = owned_path_size(&data_dir)?;
    categories.push(TechnicalStorageCategory {
        key: "other-app-data".into(),
        label: "Остальные локальные данные приложения".into(),
        bytes: total_bytes.saturating_sub(retention_managed_bytes),
        retention_managed: false,
        quota_bytes: None,
        retention_seconds: None,
    });
    Ok(TechnicalStorageStatus {
        total_bytes,
        retention_managed_bytes,
        categories,
    })
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
    removed = removed.saturating_add(crate::watcher_log::cleanup_watcher_logs(
        &data_dir.join("runtime-logs").join("watcher.log"),
    )?);
    removed = removed.saturating_add(universal_intake::enforce_ephemeral_workspace_group_quota(
        &[
            "template-contract-migration",
            "template-render-inference",
            "template-inference-work",
        ]
        .into_iter()
        .map(|workspace| data_dir.join(workspace))
        .collect::<Vec<_>>(),
        Duration::from_secs(crate::TEMPLATE_COMPILER_CACHE_RETENTION_SECONDS),
        crate::TEMPLATE_COMPILER_CACHE_QUOTA_BYTES,
        0,
    )?);
    removed = removed.saturating_add(universal_intake::enforce_ephemeral_workspace_quota(
        &data_dir.join("word-scanner-work"),
        Duration::from_secs(crate::WORD_SCANNER_CACHE_RETENTION_SECONDS),
        crate::WORD_SCANNER_CACHE_QUOTA_BYTES,
        0,
    )?);
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
    fn technical_storage_size_counts_owned_files_without_mutation() {
        let root =
            std::env::temp_dir().join(format!("dokkomplekt-storage-size-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("one.bin"), b"1234").unwrap();
        std::fs::write(root.join("nested").join("two.bin"), b"123456").unwrap();
        assert_eq!(owned_path_size(&root).unwrap(), 10);
        assert!(root.join("one.bin").exists());
        assert!(root.join("nested").join("two.bin").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn technical_storage_size_never_follows_symlink_outside_owned_root() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("dokkomplekt-storage-link-{}", uuid::Uuid::new_v4()));
        let external = std::env::temp_dir().join(format!(
            "dokkomplekt-storage-external-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(external.join("outside.bin"), b"must-not-be-counted").unwrap();
        symlink(&external, root.join("linked-external")).unwrap();

        let error = owned_path_size(&root).unwrap_err();
        assert!(error.contains("ссылкой/reparse point"));
        assert!(external.join("outside.bin").exists());

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(external);
    }

    #[test]
    fn manual_batch_storage_estimate_includes_render_peak_source_and_recovery() {
        let estimate =
            estimate_manual_batch_storage(&[(2 * MIB, 0), (20 * MIB, 1)], 5 * MIB).unwrap();
        assert_eq!(estimate.staged_outputs_bytes, 100 * MIB);
        assert_eq!(estimate.transient_render_bytes, 92 * MIB);
        assert_eq!(estimate.retained_source_bytes, 5 * MIB);
        assert_eq!(estimate.recovery_reserve_bytes, 64 * MIB);
        assert_eq!(estimate.required_free_bytes, 261 * MIB);
    }

    #[test]
    fn manual_batch_storage_gate_fails_closed_below_required_capacity() {
        let estimate = estimate_manual_batch_storage(&[(MIB, 0)], 0).unwrap();
        let error =
            require_available_capacity(estimate, estimate.required_free_bytes - 1).unwrap_err();
        assert!(error.contains("Недостаточно свободного места"));
        assert!(error.contains("commit/recovery"));
        assert!(require_available_capacity(estimate, estimate.required_free_bytes).is_ok());
    }

    #[test]
    fn dropped_primary_is_part_of_default_patient_folder_publication() {
        let preferences = PrivacyPreferences::default();
        assert!(preferences.copy_source_to_output);
    }

    #[test]
    fn legacy_source_opt_out_is_migrated_to_patient_folder_contract() {
        let legacy = PrivacyPreferences {
            copy_source_to_output: false,
            ..PrivacyPreferences::default()
        };
        let migrated = normalize_loaded_privacy_preferences(legacy);
        assert!(migrated.copy_source_to_output);
    }

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
