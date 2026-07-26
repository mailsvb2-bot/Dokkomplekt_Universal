use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use time::OffsetDateTime;

const PROCESSED_SUFFIX: &str = ".dokkomplekt-processed";
const ATTENTION_SUFFIX: &str = "_ТРЕБУЕТ_ВНИМАНИЯ.txt";
const UNREADABLE_SUFFIX: &str = " — НЕ ПРОЧИТАН.txt";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceRetentionPolicy {
    pub archive_processed_sources: bool,
    pub archive_folder_name: String,
    pub service_note_retention_days: u32,
    pub processed_marker_retention_days: u32,
    /// Zero means that archived sources are retained indefinitely.
    pub archived_source_retention_days: u32,
}

impl Default for WorkspaceRetentionPolicy {
    fn default() -> Self {
        Self {
            archive_processed_sources: true,
            archive_folder_name: "_обработано".into(),
            service_note_retention_days: 30,
            processed_marker_retention_days: 7,
            archived_source_retention_days: 365,
        }
    }
}

impl WorkspaceRetentionPolicy {
    pub fn validate(&self) -> Result<(), String> {
        validate_archive_folder_name(&self.archive_folder_name)?;
        if !(1..=3650).contains(&self.service_note_retention_days) {
            return Err(
                "Срок архивирования служебных заметок должен быть от 1 до 3650 дней.".into(),
            );
        }
        if !(1..=3650).contains(&self.processed_marker_retention_days) {
            return Err("Срок хранения processed-маркеров должен быть от 1 до 3650 дней.".into());
        }
        if self.archived_source_retention_days > 3650 {
            return Err(
                "Срок хранения архива не может превышать 3650 дней; 0 означает бессрочно.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessedSourceArchiveResult {
    pub archived_source: Option<String>,
    pub receipt_path: Option<String>,
    pub marker_removed: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct WorkspaceHygieneReport {
    pub archived_processed_sources: Vec<String>,
    pub archived_service_files: Vec<String>,
    pub removed_orphan_markers: Vec<String>,
    pub removed_expired_archived_files: Vec<String>,
    pub removed_queue_receipts: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn archive_processed_source(
    source: &Path,
    source_sha256: &str,
    policy: &WorkspaceRetentionPolicy,
) -> Result<ProcessedSourceArchiveResult, String> {
    policy.validate()?;
    let parent = source
        .parent()
        .ok_or_else(|| "У источника нет родительской папки.".to_string())?;
    let marker = processed_marker_path(source);
    if !policy.archive_processed_sources {
        return Ok(ProcessedSourceArchiveResult {
            archived_source: None,
            receipt_path: None,
            marker_removed: false,
        });
    }

    let month_folder = archive_month_folder(parent, policy);
    fs::create_dir_all(&month_folder).map_err(|error| {
        format!(
            "Не удалось создать папку архива {}: {error}",
            month_folder.display()
        )
    })?;
    let destination = unique_destination(&month_folder, source, source_sha256)?;
    move_file_safely(source, &destination)?;

    let receipt = destination.with_file_name(format!(
        "{}.dokkomplekt-receipt.json",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source")
    ));
    write_receipt(&receipt, source, &destination, source_sha256)?;
    let marker_removed = if marker.exists() {
        fs::remove_file(&marker).is_ok()
    } else {
        false
    };

    Ok(ProcessedSourceArchiveResult {
        archived_source: Some(destination.display().to_string()),
        receipt_path: Some(receipt.display().to_string()),
        marker_removed,
    })
}

pub fn cleanup_workspace_folder(
    folder: &Path,
    policy: &WorkspaceRetentionPolicy,
    now: SystemTime,
) -> Result<WorkspaceHygieneReport, String> {
    policy.validate()?;
    if !folder.exists() {
        return Ok(WorkspaceHygieneReport::default());
    }
    let mut report = WorkspaceHygieneReport::default();
    let archive_root = folder.join(&policy.archive_folder_name);
    let service_archive = archive_root.join("_служебные").join(current_month());

    let entries = fs::read_dir(folder)
        .map_err(|error| format!("Не удалось прочитать рабочую папку: {error}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path == archive_root || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let age = file_age(&path, now);
        if is_service_note(name)
            && age.is_some_and(|value| {
                value >= Duration::from_secs(u64::from(policy.service_note_retention_days) * 86_400)
            })
        {
            if let Err(error) = fs::create_dir_all(&service_archive) {
                report.warnings.push(format!(
                    "Не удалось создать архив служебных файлов {}: {error}",
                    service_archive.display()
                ));
                continue;
            }
            match move_to_unique_folder(&path, &service_archive) {
                Ok(destination) => report
                    .archived_service_files
                    .push(destination.display().to_string()),
                Err(error) => report.warnings.push(error),
            }
            continue;
        }
        if name.ends_with(PROCESSED_SUFFIX)
            && age.is_some_and(|value| {
                value
                    >= Duration::from_secs(
                        u64::from(policy.processed_marker_retention_days) * 86_400,
                    )
            })
        {
            let marker_hash = marker_sha256(&path);
            let matching_source =
                marker_source_candidates(folder, &path)
                    .into_iter()
                    .find(|candidate| {
                        marker_hash.as_deref().is_some_and(|expected| {
                            sha256_file(candidate).is_ok_and(|actual| actual == expected)
                        })
                    });
            if policy.archive_processed_sources {
                if let (Some(source), Some(hash)) =
                    (matching_source.as_ref(), marker_hash.as_deref())
                {
                    match archive_processed_source(source, hash, policy) {
                        Ok(result) => {
                            let _ = fs::remove_file(&path);
                            if let Some(archived) = result.archived_source {
                                report.archived_processed_sources.push(archived);
                            }
                        }
                        Err(error) => report.warnings.push(error),
                    }
                    continue;
                }
            }
            if matching_source.is_none() {
                match fs::remove_file(&path) {
                    Ok(()) => report
                        .removed_orphan_markers
                        .push(path.display().to_string()),
                    Err(error) => report.warnings.push(format!(
                        "Не удалось удалить устаревший маркер {}: {error}",
                        path.display()
                    )),
                }
            }
        }
    }

    let completed_queue = folder.join(".dokkomplekt-queue").join("completed");
    if completed_queue.is_dir() {
        let retention = Duration::from_secs(90 * 86_400);
        if let Ok(entries) = fs::read_dir(&completed_queue) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && file_age(&path, now).is_some_and(|age| age >= retention) {
                    match fs::remove_file(&path) {
                        Ok(()) => report
                            .removed_queue_receipts
                            .push(path.display().to_string()),
                        Err(error) => report.warnings.push(format!(
                            "Не удалось удалить устаревшую квитанцию очереди {}: {error}",
                            path.display()
                        )),
                    }
                }
            }
        }
    }

    if policy.archived_source_retention_days > 0 && archive_root.exists() {
        let retention =
            Duration::from_secs(u64::from(policy.archived_source_retention_days) * 86_400);
        cleanup_expired_archive_files(&archive_root, now, retention, &mut report)?;
    }
    Ok(report)
}

pub fn processed_marker_path(source: &Path) -> PathBuf {
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    source.with_file_name(format!("{name}{PROCESSED_SUFFIX}"))
}

pub fn processed_marker_candidates(source: &Path) -> Vec<PathBuf> {
    let current = processed_marker_path(source);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    let legacy = source.with_file_name(format!("{stem}{PROCESSED_SUFFIX}"));
    if current == legacy {
        vec![current]
    } else {
        vec![current, legacy]
    }
}

fn validate_archive_folder_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return Err("Имя папки архива должно содержать от 1 до 80 символов.".into());
    }
    if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Папка архива должна быть простой подпапкой без разделителей пути.".into());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("Имя папки архива содержит управляющие символы.".into());
    }
    Ok(())
}

fn archive_month_folder(parent: &Path, policy: &WorkspaceRetentionPolicy) -> PathBuf {
    parent
        .join(policy.archive_folder_name.trim())
        .join(current_month())
}

fn current_month() -> String {
    let now = OffsetDateTime::now_utc();
    format!("{:04}-{:02}", now.year(), u8::from(now.month()))
}

fn unique_destination(
    folder: &Path,
    source: &Path,
    source_sha256: &str,
) -> Result<PathBuf, String> {
    let file_name = source
        .file_name()
        .ok_or_else(|| "Не удалось определить имя источника.".to_string())?;
    let direct = folder.join(file_name);
    if !direct.exists() {
        return Ok(direct);
    }
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let short_hash = source_sha256.get(..12).unwrap_or(source_sha256);
    for index in 1..=10_000u32 {
        let suffix = if index == 1 {
            short_hash.to_string()
        } else {
            format!("{short_hash}-{index}")
        };
        let name = if extension.is_empty() {
            format!("{stem}-{suffix}")
        } else {
            format!("{stem}-{suffix}.{extension}")
        };
        let candidate = folder.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Не удалось подобрать уникальное имя для архивного источника.".into())
}

fn move_to_unique_folder(source: &Path, folder: &Path) -> Result<PathBuf, String> {
    let hash = sha256_file(source)?;
    let destination = unique_destination(folder, source, &hash)?;
    move_file_safely(source, &destination)?;
    Ok(destination)
}

fn move_file_safely(source: &Path, destination: &Path) -> Result<(), String> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            fs::copy(source, destination).map_err(|copy_error| {
                format!(
                    "Не удалось переместить {} в {}: rename={rename_error}; copy={copy_error}",
                    source.display(),
                    destination.display()
                )
            })?;
            let copied_hash = sha256_file(destination)?;
            let source_hash = sha256_file(source)?;
            if copied_hash != source_hash {
                let _ = fs::remove_file(destination);
                return Err("Контрольная сумма архивной копии не совпала с источником.".into());
            }
            fs::remove_file(source).map_err(|error| {
                format!(
                    "Архивная копия создана, но исходник {} не удалён: {error}",
                    source.display()
                )
            })
        }
    }
}

fn write_receipt(
    receipt: &Path,
    original: &Path,
    archived: &Path,
    source_sha256: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "schema": 1,
        "original_name": original.file_name().and_then(|value| value.to_str()).unwrap_or("source"),
        "archived_name": archived.file_name().and_then(|value| value.to_str()).unwrap_or("source"),
        "sha256": source_sha256,
        "archived_at_unix": OffsetDateTime::now_utc().unix_timestamp(),
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt)
        .map_err(|error| format!("Не удалось создать квитанцию архива: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("Не удалось записать квитанцию архива: {error}"))?;
    file.sync_all().map_err(|error| error.to_string())
}

fn marker_sha256(marker: &Path) -> Option<String> {
    fs::read_to_string(marker).ok()?.lines().find_map(|line| {
        line.strip_prefix("sha256=")
            .map(str::trim)
            .filter(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
            .map(str::to_ascii_lowercase)
    })
}

fn marker_source_candidates(folder: &Path, marker: &Path) -> Vec<PathBuf> {
    let Some(marker_name) = marker.file_name().and_then(|value| value.to_str()) else {
        return Vec::new();
    };
    let Some(base) = marker_name.strip_suffix(PROCESSED_SUFFIX) else {
        return Vec::new();
    };
    let exact = folder.join(base);
    if exact.is_file() {
        return vec![exact];
    }
    fs::read_dir(folder)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path != marker
                && path.file_stem().and_then(|value| value.to_str()) == Some(base)
        })
        .collect()
}

fn is_service_note(name: &str) -> bool {
    name.ends_with(ATTENTION_SUFFIX) || name.ends_with(UNREADABLE_SUFFIX)
}

fn file_age(path: &Path, now: SystemTime) -> Option<Duration> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    now.duration_since(modified).ok()
}

fn cleanup_expired_archive_files(
    folder: &Path,
    now: SystemTime,
    retention: Duration,
    report: &mut WorkspaceHygieneReport,
) -> Result<(), String> {
    for entry in fs::read_dir(folder)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            cleanup_expired_archive_files(&path, now, retention, report)?;
            let _ = fs::remove_dir(&path);
            continue;
        }
        if path.is_file() && file_age(&path, now).is_some_and(|age| age >= retention) {
            match fs::remove_file(&path) {
                Ok(()) => report
                    .removed_expired_archived_files
                    .push(path.display().to_string()),
                Err(error) => report.warnings.push(format!(
                    "Не удалось удалить архивный файл {}: {error}",
                    path.display()
                )),
            }
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("dokkomplekt-hygiene-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn processed_source_moves_out_of_working_folder_and_gets_receipt() {
        let root = temp_root("archive");
        let source = root.join("Иванов.docx");
        fs::write(&source, b"document").unwrap();
        let hash = sha256_file(&source).unwrap();
        fs::write(processed_marker_path(&source), format!("sha256={hash}\n")).unwrap();
        let result =
            archive_processed_source(&source, &hash, &WorkspaceRetentionPolicy::default()).unwrap();
        assert!(!source.exists());
        assert!(!processed_marker_path(&source).exists());
        assert!(Path::new(result.archived_source.as_ref().unwrap()).exists());
        assert!(Path::new(result.receipt_path.as_ref().unwrap()).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_archives_old_attention_and_removes_orphan_marker() {
        let root = temp_root("cleanup");
        let attention = root.join("Иванов_ТРЕБУЕТ_ВНИМАНИЯ.txt");
        let marker = root.join("Иванов.docx.dokkomplekt-processed");
        fs::write(&attention, b"attention").unwrap();
        fs::write(&marker, b"sha256=abc").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);
        let report =
            cleanup_workspace_folder(&root, &WorkspaceRetentionPolicy::default(), now).unwrap();
        assert_eq!(report.archived_service_files.len(), 1);
        assert_eq!(report.removed_orphan_markers.len(), 1);
        assert!(!attention.exists());
        assert!(!marker.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_migrates_legacy_processed_source_into_archive() {
        let root = temp_root("legacy-marker");
        let source = root.join("Иванов.docx");
        fs::write(&source, b"document").unwrap();
        let hash = sha256_file(&source).unwrap();
        let legacy = root.join("Иванов.dokkomplekt-processed");
        fs::write(&legacy, format!("sha256={hash}\n")).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);
        let report =
            cleanup_workspace_folder(&root, &WorkspaceRetentionPolicy::default(), now).unwrap();
        assert_eq!(report.archived_processed_sources.len(), 1);
        assert!(!source.exists());
        assert!(!legacy.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_folder_cannot_escape_working_directory() {
        let policy = WorkspaceRetentionPolicy {
            archive_folder_name: "../outside".into(),
            ..WorkspaceRetentionPolicy::default()
        };
        assert!(policy.validate().is_err());
    }
}
