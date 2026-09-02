use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use time::OffsetDateTime;
use uuid::Uuid;

use dokkomplekt_core::{ATTENTION_SUFFIX, UNREADABLE_SUFFIX};

const PROCESSED_SUFFIX: &str = ".dokkomplekt-processed";
const FINALIZING_PREFIX: &str = ".dokkomplekt-finalizing-";
const FINALIZING_SUFFIX: &str = ".pending";
const FINALIZING_CLAIM_GRACE: Duration = Duration::from_secs(30 * 60);
const RECOVERED_SOURCE_PREFIX: &str = "ВОССТАНОВЛЕННЫЙ ИСХОДНИК";
const ARCHIVE_STAGE_PREFIX: &str = ".dokkomplekt-archive-stage-";
const RECOVERY_STAGE_PREFIX: &str = ".dokkomplekt-recovery-stage-";
const RECEIPT_STAGE_PREFIX: &str = ".dokkomplekt-receipt-stage-";

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
    pub recovered_finalizing_sources: Vec<String>,
    pub removed_stale_staging_files: Vec<String>,
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
    create_real_directory_below(parent, &month_folder).map_err(|error| {
        format!(
            "Не удалось создать безопасную папку архива {}: {error}",
            month_folder.display()
        )
    })?;

    // Bind destructive cleanup to the file identity that exists at this instant.
    // A replacement created later at `source` is a different pathname and is never
    // touched by the archive/delete phase below.
    let claim = claim_matching_source(source, source_sha256)?;
    let destination = match copy_claim_to_unique_archive(&claim, &month_folder, source) {
        Ok(path) => path,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(error, recovery));
        }
    };
    let archived_sha256 = sha256_file(&destination)?;
    if archived_sha256 != claim.verified_sha256 {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            "Контрольная сумма архивной копии изменилась до фиксации квитанции.".into(),
            recovery,
        ));
    }

    let receipt = destination.with_file_name(format!(
        "{}.dokkomplekt-receipt.json",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source")
    ));
    if let Err(error) = write_receipt(&receipt, source, &destination, &archived_sha256) {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(error, recovery));
    }

    if let Err(error) = fs::remove_file(&claim.path) {
        let _ = fs::remove_file(&receipt);
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            format!(
                "Архивная копия подготовлена, но захваченный исходник {} не удалён: {error}",
                claim.path.display()
            ),
            recovery,
        ));
    }

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

pub fn delete_processed_source_if_matches(
    source: &Path,
    source_sha256: &str,
) -> Result<(), String> {
    let claim = claim_matching_source(source, source_sha256)?;
    let final_sha256 = match sha256_file(&claim.path) {
        Ok(hash) => hash,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(
                format!("Не удалось повторно проверить захваченный исходник: {error}"),
                recovery,
            ));
        }
    };
    if final_sha256 != claim.verified_sha256 {
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            "Захваченный исходник изменился перед удалением; удаление отменено.".into(),
            recovery,
        ));
    }
    fs::remove_file(&claim.path).map_err(|error| {
        let recovery = recover_finalizing_claim(&claim.path);
        with_recovery_detail(
            format!(
                "Не удалось удалить проверенный захваченный исходник {}: {error}",
                claim.path.display()
            ),
            recovery,
        )
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
        if path == archive_root {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if is_workspace_staging_name(name) {
            if file_age(&path, now).is_some_and(|age| age >= FINALIZING_CLAIM_GRACE) {
                match fs::remove_file(&path) {
                    Ok(()) => report
                        .removed_stale_staging_files
                        .push(path.display().to_string()),
                    Err(error) => report.warnings.push(format!(
                        "Не удалось удалить stale workspace staging {}: {error}",
                        path.display()
                    )),
                }
            }
            continue;
        }
        if is_finalizing_claim_name(name) {
            match finalizing_claim_timestamp(name) {
                Some(claimed_at) if finalizing_claim_is_stale(claimed_at, now) => {
                    match recover_finalizing_claim(&path) {
                        Ok(recovered) => report
                            .recovered_finalizing_sources
                            .push(recovered.display().to_string()),
                        Err(error) => report.warnings.push(error),
                    }
                }
                Some(_) => {}
                None => report.warnings.push(format!(
                    "Пропущен malformed finalization claim: {}",
                    path.display()
                )),
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let age = file_age(&path, now);
        if is_service_note(name)
            && age.is_some_and(|value| {
                value >= Duration::from_secs(u64::from(policy.service_note_retention_days) * 86_400)
            })
        {
            if let Err(error) = create_real_directory_below(folder, &service_archive) {
                report.warnings.push(error);
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

    if archive_root.exists() {
        let retention = (policy.archived_source_retention_days > 0).then(|| {
            Duration::from_secs(u64::from(policy.archived_source_retention_days) * 86_400)
        });
        match ensure_real_directory_below(folder, &archive_root) {
            Ok(archive_root_canonical) => cleanup_expired_archive_files(
                &archive_root,
                &archive_root_canonical,
                now,
                retention,
                &mut report,
            )?,
            Err(error) => report.warnings.push(error),
        }
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

#[derive(Debug)]
struct FinalizingSourceClaim {
    path: PathBuf,
    verified_sha256: String,
}

fn normalize_expected_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Ожидаемый SHA-256 источника имеет неверный формат.".into());
    }
    Ok(normalized)
}

fn finalizing_claim_path(source: &Path) -> Result<PathBuf, String> {
    let parent = source
        .parent()
        .ok_or_else(|| "У источника нет родительской папки.".to_string())?;
    let timestamp = OffsetDateTime::now_utc().unix_timestamp();
    let base = format!("{FINALIZING_PREFIX}{timestamp}-{}", Uuid::new_v4());
    let name = match source.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => {
            format!("{base}.{extension}{FINALIZING_SUFFIX}")
        }
        _ => format!("{base}{FINALIZING_SUFFIX}"),
    };
    Ok(parent.join(name))
}

fn claim_matching_source(
    source: &Path,
    expected_sha256: &str,
) -> Result<FinalizingSourceClaim, String> {
    let expected_sha256 = normalize_expected_sha256(expected_sha256)?;
    let before = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "Не удалось проверить исходник перед безопасной финализацией {}: {error}",
            source.display()
        )
    })?;
    if metadata_is_link_or_reparse(&before) || !before.is_file() {
        return Err(format!(
            "Небезопасный исходник заблокирован перед финализацией: {}",
            source.display()
        ));
    }

    let claim_path = finalizing_claim_path(source)?;
    if claim_path.exists() {
        return Err("Не удалось подобрать уникальное имя для finalization claim.".into());
    }
    fs::rename(source, &claim_path).map_err(|error| {
        format!(
            "Не удалось атомарно захватить исходник {} для финализации: {error}",
            source.display()
        )
    })?;

    let claimed_metadata = fs::symlink_metadata(&claim_path).map_err(|error| {
        format!(
            "Исходник захвачен как {}, но не удалось проверить его тип: {error}",
            claim_path.display()
        )
    })?;
    if metadata_is_link_or_reparse(&claimed_metadata) || !claimed_metadata.is_file() {
        return Err(format!(
            "Захваченный исходник небезопасен; он сохранён без удаления: {}",
            claim_path.display()
        ));
    }

    let actual_sha256 = match sha256_file(&claim_path) {
        Ok(hash) => hash,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim_path);
            return Err(with_recovery_detail(
                format!("Не удалось проверить SHA-256 захваченного исходника: {error}"),
                recovery,
            ));
        }
    };
    if actual_sha256 != expected_sha256 {
        let recovery = recover_finalizing_claim(&claim_path);
        return Err(with_recovery_detail(
            format!(
                "Исходник был заменён между проверкой и финализацией: ожидался {expected_sha256}, захвачен {actual_sha256}."
            ),
            recovery,
        ));
    }
    Ok(FinalizingSourceClaim {
        path: claim_path,
        verified_sha256: actual_sha256,
    })
}

fn copy_claim_to_unique_archive(
    claim: &FinalizingSourceClaim,
    folder: &Path,
    original_source: &Path,
) -> Result<PathBuf, String> {
    for _ in 0..=10_000u32 {
        let destination = unique_destination(folder, original_source, &claim.verified_sha256)?;
        let staging = staging_path(folder, ARCHIVE_STAGE_PREFIX);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось создать скрытый staging архива {}: {error}",
                    staging.display()
                ));
            }
        };

        let before_sha256 = match sha256_file(&claim.path) {
            Ok(hash) => hash,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if before_sha256 != claim.verified_sha256 {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err("Захваченный исходник изменился до архивирования.".into());
        }

        let mut input = match fs::File::open(&claim.path) {
            Ok(file) => file,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Не удалось открыть захваченный исходник для архивирования: {error}"
                ));
            }
        };
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Не удалось скопировать захваченный исходник в staging архива: {error}"
            ));
        }
        if let Err(error) = output.sync_all() {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Не удалось синхронизировать staging архива: {error}"
            ));
        }
        drop(output);

        let after_sha256 = match sha256_file(&claim.path) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let staged_sha256 = match sha256_file(&staging) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if after_sha256 != claim.verified_sha256 || staged_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&staging);
            return Err("Контрольная сумма изменилась во время безопасного архивирования.".into());
        }

        match fs::hard_link(&staging, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging);
                continue;
            }
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Файловая система не позволила атомарно опубликовать архив {}: {error}",
                    destination.display()
                ));
            }
        }
        if let Err(error) = fs::remove_file(&staging) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Архив опубликован, но staging {} не удалён; публикация отменена: {error}",
                staging.display()
            ));
        }
        let published_sha256 = match sha256_file(&destination) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
        };
        if published_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&destination);
            return Err(
                "Опубликованный архив не совпадает с проверенным SHA-256 исходника.".into(),
            );
        }
        return Ok(destination);
    }
    Err("Не удалось подобрать уникальное имя для архивного источника.".into())
}

fn is_finalizing_claim_name(name: &str) -> bool {
    name.starts_with(FINALIZING_PREFIX) && name.ends_with(FINALIZING_SUFFIX)
}

fn finalizing_claim_timestamp(name: &str) -> Option<i64> {
    let body = name
        .strip_prefix(FINALIZING_PREFIX)?
        .strip_suffix(FINALIZING_SUFFIX)?;
    let (timestamp, _) = body.split_once('-')?;
    timestamp.parse().ok()
}

fn finalizing_claim_is_stale(claimed_at: i64, now: SystemTime) -> bool {
    let Ok(now_since_epoch) = now.duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let Ok(claimed_at) = u64::try_from(claimed_at) else {
        return true;
    };
    now_since_epoch.as_secs().saturating_sub(claimed_at) >= FINALIZING_CLAIM_GRACE.as_secs()
}

fn finalizing_claim_extension(claim: &Path) -> Option<String> {
    let name = claim.file_name()?.to_str()?;
    let without_pending = name.strip_suffix(FINALIZING_SUFFIX)?;
    Path::new(without_pending)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn recover_finalizing_claim(claim: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(claim).map_err(|error| {
        format!(
            "Не удалось проверить finalization claim {}: {error}",
            claim.display()
        )
    })?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Небезопасный finalization claim сохранён без обработки: {}",
            claim.display()
        ));
    }
    let parent = claim
        .parent()
        .ok_or_else(|| "У finalization claim нет родительской папки.".to_string())?;
    let extension = finalizing_claim_extension(claim);

    for _ in 0..256u16 {
        let stem = format!("{RECOVERED_SOURCE_PREFIX} {}", Uuid::new_v4());
        let name = extension
            .as_deref()
            .map(|ext| format!("{stem}.{ext}"))
            .unwrap_or(stem);
        let destination = parent.join(name);
        let staging = staging_path(parent, RECOVERY_STAGE_PREFIX);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось создать скрытый staging recovery {}: {error}",
                    staging.display()
                ));
            }
        };

        let before_sha256 = match sha256_file(claim) {
            Ok(hash) => hash,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let mut input = match fs::File::open(claim) {
            Ok(file) => file,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Не удалось открыть finalization claim для recovery: {error}"
                ));
            }
        };
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось сохранить recovery staging: {error}"));
        }
        if let Err(error) = output.sync_all() {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Не удалось синхронизировать recovery staging: {error}"
            ));
        }
        drop(output);

        let after_sha256 = match sha256_file(claim) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let staged_sha256 = match sha256_file(&staging) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if before_sha256 != after_sha256 || before_sha256 != staged_sha256 {
            let _ = fs::remove_file(&staging);
            return Err(
                "Finalization claim изменился во время recovery; исходник оставлен нетронутым."
                    .into(),
            );
        }

        match fs::hard_link(&staging, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging);
                continue;
            }
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Файловая система не позволила атомарно опубликовать recovery {}: {error}",
                    destination.display()
                ));
            }
        }
        if let Err(error) = fs::remove_file(&staging) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Recovery опубликован, но staging {} не удалён; публикация отменена: {error}",
                staging.display()
            ));
        }
        let recovered_sha256 = match sha256_file(&destination) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
        };
        if recovered_sha256 != before_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Опубликованный recovery-файл не совпадает с finalization claim.".into());
        }
        if let Err(error) = fs::remove_file(claim) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Recovery-копия подготовлена, но claim {} не удалён: {error}",
                claim.display()
            ));
        }
        return Ok(destination);
    }
    Err("Не удалось подобрать уникальное имя для recovery-файла.".into())
}

fn with_recovery_detail(message: String, recovery: Result<PathBuf, String>) -> String {
    match recovery {
        Ok(path) => format!(
            "{message} Файл сохранён для повторной обработки: {}",
            path.display()
        ),
        Err(error) => format!("{message} Recovery: {error}"),
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
    // Service files use the same identity-safe claim protocol as processed sources.
    // A replacement that reuses `source` after the initial hash is recovered rather
    // than deleted, and the visible archive destination is create-if-absent.
    let hash = sha256_file(source)?;
    let claim = claim_matching_source(source, &hash)?;
    let destination = match copy_claim_to_unique_archive(&claim, folder, source) {
        Ok(path) => path,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(error, recovery));
        }
    };
    if let Err(error) = fs::remove_file(&claim.path) {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            format!(
                "Архивная копия служебного файла подготовлена, но захваченный источник {} не удалён: {error}",
                claim.path.display()
            ),
            recovery,
        ));
    }
    Ok(destination)
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
    publish_bytes_create_new(receipt, &bytes, RECEIPT_STAGE_PREFIX)
}

fn publish_bytes_create_new(
    destination: &Path,
    bytes: &[u8],
    stage_prefix: &str,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "У публикуемого файла нет родительской папки.".to_string())?;
    let staging = staging_path(parent, stage_prefix);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|error| {
            format!(
                "Не удалось создать скрытый staging {}: {error}",
                staging.display()
            )
        })?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(&staging);
        return Err(format!("Не удалось записать скрытый staging: {error}"));
    }
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(&staging);
        return Err(format!(
            "Не удалось синхронизировать скрытый staging: {error}"
        ));
    }
    drop(file);

    let staged = match fs::read(&staging) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось проверить скрытый staging: {error}"));
        }
    };
    if staged != bytes {
        let _ = fs::remove_file(&staging);
        return Err("Содержимое staging изменилось до публикации.".into());
    }

    match fs::hard_link(&staging, destination) {
        Ok(()) => {}
        Err(error) => {
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Не удалось атомарно опубликовать {} без перезаписи существующего файла: {error}",
                destination.display()
            ));
        }
    }
    if let Err(error) = fs::remove_file(&staging) {
        let _ = fs::remove_file(destination);
        return Err(format!(
            "Файл опубликован, но staging {} не удалён; публикация отменена: {error}",
            staging.display()
        ));
    }
    let published = match fs::read(destination) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(destination);
            return Err(format!("Не удалось проверить опубликованный файл: {error}"));
        }
    };
    if published != bytes {
        let _ = fs::remove_file(destination);
        return Err("Опубликованный файл не совпадает с проверенным staging.".into());
    }
    Ok(())
}

fn staging_path(folder: &Path, prefix: &str) -> PathBuf {
    folder.join(format!("{prefix}{}{FINALIZING_SUFFIX}", Uuid::new_v4()))
}

fn is_workspace_staging_name(name: &str) -> bool {
    name.ends_with(FINALIZING_SUFFIX)
        && [
            ARCHIVE_STAGE_PREFIX,
            RECOVERY_STAGE_PREFIX,
            RECEIPT_STAGE_PREFIX,
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
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

fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

fn create_real_directory_below(root: &Path, directory: &Path) -> Result<PathBuf, String> {
    let root_canonical = root
        .canonicalize()
        .map_err(|error| format!("Не удалось проверить корень {}: {error}", root.display()))?;
    let relative = directory.strip_prefix(root).map_err(|_| {
        format!(
            "Архивный путь находится вне рабочей папки: {}",
            directory.display()
        )
    })?;
    let mut current = root_canonical.clone();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata_is_link_or_reparse(&metadata) {
                    return Err(format!(
                        "Архивный каталог-ссылка/reparse point заблокирован: {}",
                        current.display()
                    ));
                }
                if !metadata.is_dir() {
                    return Err(format!(
                        "Архивный путь не является каталогом: {}",
                        current.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|create_error| {
                    format!(
                        "Не удалось создать архивный каталог {}: {create_error}",
                        current.display()
                    )
                })?;
                let metadata = fs::symlink_metadata(&current).map_err(|metadata_error| {
                    format!(
                        "Не удалось проверить созданный архивный каталог {}: {metadata_error}",
                        current.display()
                    )
                })?;
                if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                    return Err(format!(
                        "Созданный архивный путь небезопасен: {}",
                        current.display()
                    ));
                }
            }
            Err(error) => {
                return Err(format!(
                    "Не удалось проверить архивный каталог {}: {error}",
                    current.display()
                ));
            }
        }
    }
    let canonical = directory.canonicalize().map_err(|error| {
        format!(
            "Не удалось канонизировать архивный каталог {}: {error}",
            directory.display()
        )
    })?;
    if !canonical.starts_with(&root_canonical) {
        return Err(format!(
            "Архивный каталог вышел за пределы рабочей папки: {}",
            directory.display()
        ));
    }
    Ok(canonical)
}

fn ensure_real_directory_below(root: &Path, directory: &Path) -> Result<PathBuf, String> {
    let root_canonical = root
        .canonicalize()
        .map_err(|error| format!("Не удалось проверить корень {}: {error}", root.display()))?;
    let mut current = root_canonical.clone();
    let relative = directory.strip_prefix(root).map_err(|_| {
        format!(
            "Архивный путь находится вне рабочей папки: {}",
            directory.display()
        )
    })?;
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!(
                "Не удалось проверить архивный каталог {}: {error}",
                current.display()
            )
        })?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "Архивный каталог-ссылка/reparse point заблокирован: {}",
                current.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!(
                "Архивный путь не является каталогом: {}",
                current.display()
            ));
        }
    }
    let canonical = directory.canonicalize().map_err(|error| {
        format!(
            "Не удалось канонизировать архивный каталог {}: {error}",
            directory.display()
        )
    })?;
    if !canonical.starts_with(&root_canonical) {
        return Err(format!(
            "Архивный каталог вышел за пределы рабочей папки: {}",
            directory.display()
        ));
    }
    Ok(canonical)
}

fn cleanup_expired_archive_files(
    folder: &Path,
    archive_root_canonical: &Path,
    now: SystemTime,
    retention: Option<Duration>,
    report: &mut WorkspaceHygieneReport,
) -> Result<(), String> {
    let entries = fs::read_dir(folder).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report
                    .warnings
                    .push(format!("Не удалось прочитать элемент архива: {error}"));
                continue;
            }
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report.warnings.push(format!(
                    "Не удалось проверить архивный элемент {}: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if metadata_is_link_or_reparse(&metadata) {
            report.warnings.push(format!(
                "Очистка пропустила ссылку/reparse point внутри архива: {}",
                path.display()
            ));
            continue;
        }
        let canonical = match path.canonicalize() {
            Ok(value) if value.starts_with(archive_root_canonical) => value,
            Ok(_) => {
                report.warnings.push(format!(
                    "Очистка заблокировала путь вне корня архива: {}",
                    path.display()
                ));
                continue;
            }
            Err(error) => {
                report.warnings.push(format!(
                    "Не удалось канонизировать архивный элемент {}: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if metadata.is_dir() {
            cleanup_expired_archive_files(&path, archive_root_canonical, now, retention, report)?;
            let _ = fs::remove_dir(&canonical);
            continue;
        }
        if metadata.is_file() {
            let name = canonical
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if is_workspace_staging_name(name)
                && file_age(&canonical, now).is_some_and(|age| age >= FINALIZING_CLAIM_GRACE)
            {
                match fs::remove_file(&canonical) {
                    Ok(()) => report
                        .removed_stale_staging_files
                        .push(canonical.display().to_string()),
                    Err(error) => report.warnings.push(format!(
                        "Не удалось удалить stale archive staging {}: {error}",
                        canonical.display()
                    )),
                }
                continue;
            }
            if retention.is_some_and(|retention| {
                file_age(&canonical, now).is_some_and(|age| age >= retention)
            }) {
                match fs::remove_file(&canonical) {
                    Ok(()) => report
                        .removed_expired_archived_files
                        .push(canonical.display().to_string()),
                    Err(error) => report.warnings.push(format!(
                        "Не удалось удалить архивный файл {}: {error}",
                        canonical.display()
                    )),
                }
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

    #[cfg(unix)]
    #[test]
    fn cleanup_never_follows_symlink_outside_archive_root() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink-boundary");
        let outside = temp_root("symlink-outside");
        let victim = outside.join("victim.txt");
        fs::write(&victim, b"must survive").unwrap();
        let archive = root.join("_обработано");
        fs::create_dir_all(&archive).unwrap();
        symlink(&outside, archive.join("escape")).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);
        let report = cleanup_workspace_folder(&root, &WorkspaceRetentionPolicy::default(), now)
            .expect("cleanup must fail closed without following link");
        assert!(victim.exists());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("ссылку/reparse point")));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    fn hash_bytes(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn recovered_files(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with(RECOVERED_SOURCE_PREFIX))
            })
            .collect()
    }

    #[test]
    fn archive_never_archives_replacement_under_stale_sha() {
        let root = temp_root("archive-replacement");
        let source = root.join("case.docx");
        fs::write(&source, b"replacement").unwrap();
        let error = archive_processed_source(
            &source,
            &hash_bytes(b"processed-old-version"),
            &WorkspaceRetentionPolicy::default(),
        )
        .unwrap_err();
        assert!(error.contains("заменён"));
        let recovered = recovered_files(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(fs::read(&recovered[0]).unwrap(), b"replacement");
        let receipts = fs::read_dir(root.join("_обработано"))
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count();
        assert_eq!(receipts, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_never_removes_replacement_under_stale_sha() {
        let root = temp_root("delete-replacement");
        let source = root.join("case.docx");
        fs::write(&source, b"replacement").unwrap();
        let error = delete_processed_source_if_matches(&source, &hash_bytes(b"old")).unwrap_err();
        assert!(error.contains("заменён"));
        let recovered = recovered_files(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(fs::read(&recovered[0]).unwrap(), b"replacement");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_removes_only_matching_claimed_source() {
        let root = temp_root("delete-matching");
        let source = root.join("case.docx");
        fs::write(&source, b"processed").unwrap();
        let hash = sha256_file(&source).unwrap();
        delete_processed_source_if_matches(&source, &hash).unwrap();
        assert!(!source.exists());
        assert!(recovered_files(&root).is_empty());
        assert!(fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(FINALIZING_PREFIX)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn receipt_publication_never_overwrites_existing_final_file() {
        let root = temp_root("receipt-no-overwrite");
        let receipt = root.join("receipt.json");
        let original = root.join("case.docx");
        let archived = root.join("archived.docx");
        fs::write(&receipt, b"existing-receipt").unwrap();
        let error =
            write_receipt(&receipt, &original, &archived, &hash_bytes(b"document")).unwrap_err();
        assert!(error.contains("без перезаписи"));
        assert_eq!(fs::read(&receipt).unwrap(), b"existing-receipt");
        assert!(!fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(RECEIPT_STAGE_PREFIX)
            }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn receipt_publication_is_complete_and_leaves_no_staging_file() {
        let root = temp_root("receipt-complete");
        let receipt = root.join("receipt.json");
        let original = root.join("case.docx");
        let archived = root.join("archived.docx");
        write_receipt(&receipt, &original, &archived, &hash_bytes(b"document")).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        assert_eq!(parsed["schema"], 1);
        assert!(!fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(RECEIPT_STAGE_PREFIX)
            }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_archive_preserves_existing_destination_without_overwrite() {
        let root = temp_root("service-no-overwrite");
        let source = root.join("case_ТРЕБУЕТ_ВНИМАНИЯ.txt");
        fs::write(&source, b"new-service-note").unwrap();
        let folder = root
            .join("_обработано")
            .join("_служебные")
            .join(current_month());
        fs::create_dir_all(&folder).unwrap();
        let existing = folder.join(source.file_name().unwrap());
        fs::write(&existing, b"existing-service-note").unwrap();

        let archived = move_to_unique_folder(&source, &folder).unwrap();
        assert_eq!(fs::read(&existing).unwrap(), b"existing-service-note");
        assert_eq!(fs::read(&archived).unwrap(), b"new-service-note");
        assert_ne!(archived, existing);
        assert!(!source.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_nested_staging_is_removed_even_with_indefinite_archive_retention() {
        let root = temp_root("stale-stage-indefinite");
        let archive = root.join("_обработано").join("2026-08");
        fs::create_dir_all(&archive).unwrap();
        let staging = archive.join(format!("{RECEIPT_STAGE_PREFIX}old{FINALIZING_SUFFIX}"));
        let retained = archive.join("keep.docx");
        fs::write(&staging, b"stale").unwrap();
        fs::write(&retained, b"keep").unwrap();
        let policy = WorkspaceRetentionPolicy {
            archived_source_retention_days: 0,
            ..WorkspaceRetentionPolicy::default()
        };
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);

        let report = cleanup_workspace_folder(&root, &policy, now).unwrap();
        assert!(!staging.exists());
        assert!(retained.exists());
        assert_eq!(report.removed_stale_staging_files.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_receipt_uses_verified_archived_sha() {
        let root = temp_root("archive-receipt-sha");
        let source = root.join("case.docx");
        fs::write(&source, b"processed").unwrap();
        let hash = sha256_file(&source).unwrap();
        let result =
            archive_processed_source(&source, &hash, &WorkspaceRetentionPolicy::default()).unwrap();
        let archived = PathBuf::from(result.archived_source.unwrap());
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(result.receipt_path.unwrap()).unwrap()).unwrap();
        assert_eq!(
            receipt["sha256"].as_str(),
            Some(sha256_file(&archived).unwrap().as_str())
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_recovers_stale_finalization_claim_as_supported_source() {
        let root = temp_root("stale-finalization-claim");
        let claim = root.join(format!(
            "{FINALIZING_PREFIX}1-{}.docx{FINALIZING_SUFFIX}",
            Uuid::new_v4()
        ));
        fs::write(&claim, b"survives-crash").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);
        let report =
            cleanup_workspace_folder(&root, &WorkspaceRetentionPolicy::default(), now).unwrap();
        assert_eq!(report.recovered_finalizing_sources.len(), 1);
        assert!(!claim.exists());
        let recovered = PathBuf::from(&report.recovered_finalizing_sources[0]);
        assert_eq!(
            recovered.extension().and_then(|value| value.to_str()),
            Some("docx")
        );
        assert_eq!(fs::read(&recovered).unwrap(), b"survives-crash");
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
