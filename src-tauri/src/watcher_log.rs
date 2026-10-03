use std::io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager as _;
use uuid::Uuid;

const RUNTIME_LOG_DIR: &str = "runtime-logs";
const WATCHER_LOG_NAME: &str = "watcher.log";
const OWNERSHIP_HEADER: &[u8] = b"# dokkomplekt-runtime-log-v1\n";
const LEGACY_MIGRATION_NOTE: &[u8] = b"# migrated legacy app-data watcher.log\n";
const LEGACY_ARCHIVE_NAME: &str = "watcher.legacy.log";
pub(crate) const WATCHER_LOG_ACTIVE_QUOTA_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const WATCHER_LOG_TOTAL_QUOTA_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const WATCHER_LOG_RETENTION_SECONDS: u64 = 14 * 24 * 60 * 60;
const WATCHER_LOG_MAX_ARCHIVES: usize = 3;
static WATCHER_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Copy)]
struct RuntimeLogPolicy {
    active_quota_bytes: u64,
    total_quota_bytes: u64,
    retention: Duration,
    max_archives: usize,
}

impl RuntimeLogPolicy {
    const fn watcher() -> Self {
        Self {
            active_quota_bytes: WATCHER_LOG_ACTIVE_QUOTA_BYTES,
            total_quota_bytes: WATCHER_LOG_TOTAL_QUOTA_BYTES,
            retention: Duration::from_secs(WATCHER_LOG_RETENTION_SECONDS),
            max_archives: WATCHER_LOG_MAX_ARCHIVES,
        }
    }
}

pub(crate) fn watcher_log_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Каталог журналов фонового агента недоступен: {error}"))?;
    let path = data_dir.join(RUNTIME_LOG_DIR).join(WATCHER_LOG_NAME);
    let _guard = WATCHER_LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "runtime log lock failed".to_string())?;
    retire_legacy_watcher_log(&data_dir, &path, SystemTime::now())?;
    Ok(path)
}

pub(crate) fn append_watcher_log(path: &Path, message: &str) -> Result<(), String> {
    let _guard = WATCHER_LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "runtime log lock failed".to_string())?;
    append_with_policy(
        path,
        message,
        RuntimeLogPolicy::watcher(),
        SystemTime::now(),
    )
}

pub(crate) fn cleanup_watcher_logs(path: &Path) -> Result<usize, String> {
    let _guard = WATCHER_LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "runtime log lock failed".to_string())?;
    cleanup_archives_with_policy(path, RuntimeLogPolicy::watcher(), SystemTime::now())
}

pub(crate) fn owned_watcher_log_bytes(path: &Path) -> Result<u64, String> {
    let _guard = WATCHER_LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "runtime log lock failed".to_string())?;
    owned_log_bytes(path)
}

fn owned_log_bytes(active_path: &Path) -> Result<u64, String> {
    let Some(root) = active_path.parent() else {
        return Err("Не удалось определить каталог runtime log.".into());
    };
    let root_metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Не удалось проверить каталог runtime log: {error}")),
    };
    if crate::publication_metadata_is_link_or_reparse(&root_metadata) || !root_metadata.is_dir() {
        return Err(format!(
            "Каталог runtime log имеет небезопасный тип: {}",
            root.display()
        ));
    }

    let mut total = validate_active_log(active_path)?;
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path == active_path || !is_archive_name(&path) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(format!(
                "Архив runtime log имеет небезопасный тип: {}",
                path.display()
            ));
        }
        if file_has_ownership_header(&path)? {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

fn append_with_policy(
    path: &Path,
    message: &str,
    policy: RuntimeLogPolicy,
    now: SystemTime,
) -> Result<(), String> {
    validate_policy(policy)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    ensure_safe_log_root(parent)?;
    // Enforce the existing archive boundary before accepting another write.
    // If ownership is ambiguous or quota cleanup is blocked, logging stops
    // instead of growing the disk behind a failed cleanup.
    cleanup_archives_with_policy(path, policy, now)?;
    let line = format!("{}\n", message.replace(['\r', '\n'], " "));
    if OWNERSHIP_HEADER.len() as u64 + line.len() as u64 > policy.active_quota_bytes {
        return Err("Одна запись runtime log превышает лимит активного журнала.".into());
    }

    let active_len = validate_active_log(path)?;
    if active_len > 0 && active_len.saturating_add(line.len() as u64) > policy.active_quota_bytes {
        rotate_active_log(path, now)?;
    }

    if !path.exists() {
        initialize_active_log_atomically(path)?;
    }

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| format!("Не удалось открыть runtime log: {error}"))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("Не удалось записать runtime log: {error}"))?;
    file.flush()
        .map_err(|error| format!("Не удалось завершить запись runtime log: {error}"))?;
    cleanup_archives_with_policy(path, policy, now)?;
    Ok(())
}

fn initialize_active_log_atomically(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    let temp = parent.join(format!(".watcher-init-{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Не удалось подготовить runtime log: {error}"))?;
        file.write_all(OWNERSHIP_HEADER)
            .map_err(|error| format!("Не удалось записать ownership runtime log: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Не удалось синхронизировать runtime log: {error}"))?;
        drop(file);

        match std::fs::hard_link(&temp, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                validate_active_log(path).map(|_| ())
            }
            Err(error) => Err(format!(
                "Не удалось атомарно опубликовать runtime log: {error}"
            )),
        }
    })();
    let _ = std::fs::remove_file(&temp);
    result
}

fn retire_legacy_watcher_log(
    data_dir: &Path,
    active_path: &Path,
    now: SystemTime,
) -> Result<(), String> {
    let legacy = data_dir.join(WATCHER_LOG_NAME);
    let metadata = match std::fs::symlink_metadata(&legacy) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Не удалось проверить legacy watcher log: {error}")),
    };
    if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        // An unexpected object at the historical path is not treated as app-owned.
        return Ok(());
    }
    if !legacy_watcher_log_is_owned(&legacy)? {
        return Ok(());
    }

    let root = active_path
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    ensure_safe_log_root(root)?;
    let archive = root.join(LEGACY_ARCHIVE_NAME);
    match std::fs::symlink_metadata(&archive) {
        Ok(existing) => {
            if crate::publication_metadata_is_link_or_reparse(&existing)
                || !existing.is_file()
                || !file_has_ownership_header(&archive)?
            {
                return Err(
                    "Legacy runtime log не перенесён: целевой архив не имеет ownership proof."
                        .into(),
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_legacy_archive_atomically(&legacy, &archive, metadata.len())?;
        }
        Err(error) => {
            return Err(format!(
                "Не удалось проверить legacy runtime archive: {error}"
            ))
        }
    }

    std::fs::remove_file(&legacy)
        .map_err(|error| format!("Legacy watcher log перенесён, но исходник не удалён: {error}"))?;
    cleanup_archives_with_policy(active_path, RuntimeLogPolicy::watcher(), now)?;
    Ok(())
}

fn legacy_watcher_log_is_owned(path: &Path) -> Result<bool, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Не удалось проверить legacy watcher log: {error}"))?;
    if metadata.len() == 0 {
        return Ok(true);
    }

    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Не удалось открыть legacy watcher log: {error}"))?;
    const PREFIX: &[u8] = b"[watcher] ";
    let mut buffer = [0_u8; 8192];
    let mut at_line_start = true;
    let mut prefix_index = 0_usize;
    let mut saw_owned_line = false;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Не удалось прочитать legacy watcher log: {error}"))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if at_line_start {
                if *byte == b'\n' || prefix_index >= PREFIX.len() || *byte != PREFIX[prefix_index] {
                    return Ok(false);
                }
                prefix_index += 1;
                if prefix_index == PREFIX.len() {
                    at_line_start = false;
                    saw_owned_line = true;
                }
            } else if *byte == b'\n' {
                at_line_start = true;
                prefix_index = 0;
            }
        }
    }
    if at_line_start && prefix_index != 0 {
        return Ok(false);
    }
    Ok(saw_owned_line)
}

fn write_legacy_archive_atomically(
    legacy: &Path,
    archive: &Path,
    legacy_len: u64,
) -> Result<(), String> {
    let root = archive
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    let temp = root.join(format!(
        ".watcher-legacy-migrate-{}.tmp",
        Uuid::new_v4().simple()
    ));
    let result = (|| -> Result<(), String> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| format!("Не удалось подготовить legacy runtime archive: {error}"))?;
        output
            .write_all(OWNERSHIP_HEADER)
            .and_then(|_| output.write_all(LEGACY_MIGRATION_NOTE))
            .map_err(|error| format!("Не удалось записать legacy runtime archive: {error}"))?;

        let reserved = (OWNERSHIP_HEADER.len() + LEGACY_MIGRATION_NOTE.len()) as u64;
        let payload_limit = WATCHER_LOG_ACTIVE_QUOTA_BYTES.saturating_sub(reserved);
        let mut source = std::fs::File::open(legacy)
            .map_err(|error| format!("Не удалось открыть legacy watcher log: {error}"))?;
        if legacy_len > payload_limit {
            source
                .seek(SeekFrom::Start(legacy_len - payload_limit))
                .map_err(|error| format!("Не удалось ограничить legacy watcher log: {error}"))?;
            let mut reader = BufReader::new(source);
            let mut partial_line = Vec::new();
            reader
                .read_until(b'\n', &mut partial_line)
                .map_err(|error| format!("Не удалось выровнять legacy watcher log: {error}"))?;
            std::io::copy(&mut reader, &mut output)
                .map_err(|error| format!("Не удалось перенести legacy watcher log: {error}"))?;
        } else {
            std::io::copy(&mut source, &mut output)
                .map_err(|error| format!("Не удалось перенести legacy watcher log: {error}"))?;
        }
        output.sync_all().map_err(|error| {
            format!("Не удалось синхронизировать legacy runtime archive: {error}")
        })?;
        drop(output);
        std::fs::hard_link(&temp, archive).map_err(|error| {
            format!("Не удалось атомарно опубликовать legacy runtime archive: {error}")
        })?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp);
    result
}

fn validate_policy(policy: RuntimeLogPolicy) -> Result<(), String> {
    if policy.active_quota_bytes < OWNERSHIP_HEADER.len() as u64 + 1
        || policy.total_quota_bytes < policy.active_quota_bytes
        || policy.max_archives == 0
    {
        return Err("Некорректная политика квоты runtime log.".into());
    }
    Ok(())
}

fn ensure_safe_log_root(root: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) => {
            if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(format!(
                    "Каталог runtime log имеет небезопасный тип: {}",
                    root.display()
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(root)
                .map_err(|e| format!("Не удалось создать каталог runtime log: {e}"))?;
            let metadata = std::fs::symlink_metadata(root)
                .map_err(|e| format!("Не удалось проверить каталог runtime log: {e}"))?;
            if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
                return Err(format!(
                    "Созданный каталог runtime log имеет небезопасный тип: {}",
                    root.display()
                ));
            }
        }
        Err(error) => return Err(format!("Не удалось проверить каталог runtime log: {error}")),
    }
    Ok(())
}

fn validate_active_log(path: &Path) -> Result<u64, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Не удалось проверить runtime log: {error}")),
    };
    if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Runtime log имеет небезопасный тип: {}",
            path.display()
        ));
    }
    if !file_has_ownership_header(path)? {
        return Err(format!(
            "Существующий runtime log не имеет ownership proof и не изменён: {}",
            path.display()
        ));
    }
    Ok(metadata.len())
}

fn file_has_ownership_header(path: &Path) -> Result<bool, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Не удалось проверить ownership runtime log: {error}"))?;
    let mut header = vec![0_u8; OWNERSHIP_HEADER.len()];
    match file.read_exact(&mut header) {
        Ok(()) => Ok(header == OWNERSHIP_HEADER),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(format!(
            "Не удалось прочитать ownership runtime log: {error}"
        )),
    }
}

fn rotate_active_log(path: &Path, now: SystemTime) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    let timestamp = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let archive = parent.join(format!(
        "watcher.{timestamp}.{}.log",
        Uuid::new_v4().simple()
    ));
    std::fs::rename(path, &archive)
        .map_err(|error| format!("Не удалось ротировать runtime log: {error}"))
}

fn cleanup_archives_with_policy(
    active_path: &Path,
    policy: RuntimeLogPolicy,
    now: SystemTime,
) -> Result<usize, String> {
    validate_policy(policy)?;
    let root = active_path
        .parent()
        .ok_or_else(|| "Не удалось определить каталог runtime log.".to_string())?;
    let root_metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Не удалось проверить каталог runtime log: {error}")),
    };
    if crate::publication_metadata_is_link_or_reparse(&root_metadata) || !root_metadata.is_dir() {
        return Err(format!(
            "Каталог runtime log имеет небезопасный тип: {}",
            root.display()
        ));
    }

    let active_bytes = validate_active_log(active_path)?;
    let mut archives = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path == active_path || !is_archive_name(&path) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
            return Err(format!(
                "Архив runtime log имеет небезопасный тип и очистка остановлена: {}",
                path.display()
            ));
        }
        // A filename pattern is not ownership. Unknown lookalikes stay untouched.
        if !file_has_ownership_header(&path)? {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let age = now.duration_since(modified).unwrap_or_default();
        archives.push((path, metadata.len(), modified, age >= policy.retention));
    }

    let mut removed = 0usize;
    for (path, _, _, expired) in &archives {
        if *expired {
            std::fs::remove_file(path)
                .map_err(|error| format!("Не удалось удалить истёкший runtime log: {error}"))?;
            removed += 1;
        }
    }
    archives.retain(|(_, _, _, expired)| !*expired);
    archives.sort_by_key(|(_, _, modified, _)| *modified);

    let mut archive_bytes = archives.iter().fold(0_u64, |total, (_, bytes, _, _)| {
        total.saturating_add(*bytes)
    });
    while !archives.is_empty()
        && (archives.len() > policy.max_archives
            || active_bytes.saturating_add(archive_bytes) > policy.total_quota_bytes)
    {
        let (path, bytes, _, _) = archives.remove(0);
        std::fs::remove_file(&path)
            .map_err(|error| format!("Не удалось применить квоту runtime log: {error}"))?;
        archive_bytes = archive_bytes.saturating_sub(bytes);
        removed += 1;
    }
    if active_bytes.saturating_add(archive_bytes) > policy.total_quota_bytes {
        return Err(
            "Активный runtime log превышает общую квоту; новые записи заблокированы.".into(),
        );
    }
    Ok(removed)
}

fn is_archive_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    name.starts_with("watcher.") && name.ends_with(".log") && name != WATCHER_LOG_NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_policy() -> RuntimeLogPolicy {
        RuntimeLogPolicy {
            active_quota_bytes: 96,
            total_quota_bytes: 240,
            retention: Duration::from_secs(60 * 60),
            max_archives: 2,
        }
    }

    #[test]
    fn atomic_initialization_cleans_temp_when_publish_fails() {
        let root =
            std::env::temp_dir().join(format!("dkk-runtime-log-init-fail-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(WATCHER_LOG_NAME);
        std::fs::create_dir(&path).unwrap();

        let error = initialize_active_log_atomically(&path).unwrap_err();
        assert!(error.contains("небезопасный тип") || error.contains("опубликовать"));
        let stale_temp = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .any(|name| name.starts_with(".watcher-init-"));
        assert!(!stale_temp);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_app_data_log_is_migrated_into_owned_bounded_archive() {
        let data_dir =
            std::env::temp_dir().join(format!("dkk-runtime-log-legacy-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).unwrap();
        let legacy = data_dir.join(WATCHER_LOG_NAME);
        std::fs::write(
            &legacy,
            b"[watcher] source_processing_failed; visible_note=true\n[watcher] worker_panic; retry_blocked=true\n",
        )
        .unwrap();
        let active = data_dir.join(RUNTIME_LOG_DIR).join(WATCHER_LOG_NAME);

        retire_legacy_watcher_log(&data_dir, &active, SystemTime::now()).unwrap();

        assert!(!legacy.exists());
        let archive = data_dir.join(RUNTIME_LOG_DIR).join(LEGACY_ARCHIVE_NAME);
        assert!(file_has_ownership_header(&archive).unwrap());
        assert!(std::fs::metadata(&archive).unwrap().len() <= WATCHER_LOG_ACTIVE_QUOTA_BYTES);
        assert_eq!(
            owned_log_bytes(&active).unwrap(),
            std::fs::metadata(&archive).unwrap().len()
        );

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn unrecognized_legacy_file_is_preserved_and_not_claimed_as_owned() {
        let data_dir =
            std::env::temp_dir().join(format!("dkk-runtime-log-legacy-unknown-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).unwrap();
        let legacy = data_dir.join(WATCHER_LOG_NAME);
        std::fs::write(&legacy, b"user-created file at historical path\n").unwrap();
        let active = data_dir.join(RUNTIME_LOG_DIR).join(WATCHER_LOG_NAME);

        retire_legacy_watcher_log(&data_dir, &active, SystemTime::now()).unwrap();

        assert_eq!(
            std::fs::read(&legacy).unwrap(),
            b"user-created file at historical path\n"
        );
        assert!(!data_dir.join(RUNTIME_LOG_DIR).exists());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn runtime_log_rotation_enforces_total_quota_and_archive_count() {
        let root = std::env::temp_dir().join(format!("dkk-runtime-log-{}", Uuid::new_v4()));
        let path = root.join(WATCHER_LOG_NAME);
        for index in 0..40 {
            append_with_policy(
                &path,
                &format!("event={index}; status=bounded"),
                tiny_policy(),
                SystemTime::now(),
            )
            .unwrap();
        }
        let entries = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let archives = entries.iter().filter(|path| is_archive_name(path)).count();
        let total = entries
            .iter()
            .map(|path| std::fs::metadata(path).unwrap().len())
            .sum::<u64>();
        assert!(archives <= tiny_policy().max_archives);
        assert!(std::fs::metadata(&path).unwrap().len() <= tiny_policy().active_quota_bytes);
        assert!(total <= tiny_policy().total_quota_bytes);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_preserves_unknown_lookalike_file() {
        let root = std::env::temp_dir().join(format!("dkk-runtime-log-unknown-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(WATCHER_LOG_NAME);
        let unknown = root.join("watcher.1.lookalike.log");
        std::fs::write(&unknown, b"user-owned").unwrap();
        append_with_policy(&path, "status=ok", tiny_policy(), SystemTime::now()).unwrap();
        cleanup_archives_with_policy(&path, tiny_policy(), SystemTime::now()).unwrap();
        assert_eq!(std::fs::read(&unknown).unwrap(), b"user-owned");
        assert_eq!(
            owned_log_bytes(&path).unwrap(),
            std::fs::metadata(&path).unwrap().len()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_active_log_fails_cleanup_without_panicking_or_deleting() {
        let root =
            std::env::temp_dir().join(format!("dkk-runtime-log-oversized-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(WATCHER_LOG_NAME);
        let mut bytes = OWNERSHIP_HEADER.to_vec();
        bytes.resize((tiny_policy().total_quota_bytes + 1) as usize, b'x');
        std::fs::write(&path, &bytes).unwrap();

        let error =
            cleanup_archives_with_policy(&path, tiny_policy(), SystemTime::now()).unwrap_err();
        assert!(error.contains("превышает общую квоту"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), bytes.len() as u64);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn unsafe_archive_blocks_append_before_log_can_grow() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("dkk-runtime-log-blocked-{}", Uuid::new_v4()));
        let external =
            std::env::temp_dir().join(format!("dkk-runtime-log-target-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        let path = root.join(WATCHER_LOG_NAME);
        append_with_policy(&path, "status=seed", tiny_policy(), SystemTime::now()).unwrap();
        let before = std::fs::metadata(&path).unwrap().len();
        symlink(&external, root.join("watcher.unsafe.log")).unwrap();

        let error = append_with_policy(
            &path,
            "status=must-not-grow",
            tiny_policy(),
            SystemTime::now(),
        )
        .unwrap_err();
        assert!(error.contains("небезопасный тип"));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), before);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(external);
    }

    #[test]
    fn zero_retention_removes_only_owned_rotated_logs() {
        let root =
            std::env::temp_dir().join(format!("dkk-runtime-log-retention-{}", Uuid::new_v4()));
        let path = root.join(WATCHER_LOG_NAME);
        let policy = RuntimeLogPolicy {
            retention: Duration::ZERO,
            ..tiny_policy()
        };
        for index in 0..8 {
            append_with_policy(
                &path,
                &format!("long-enough-event={index}; status=rotate-now"),
                policy,
                SystemTime::now(),
            )
            .unwrap();
        }
        let archives = std::fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| is_archive_name(path))
            .count();
        assert_eq!(archives, 0);
        assert!(path.is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn runtime_log_refuses_symlinked_root() {
        use std::os::unix::fs::symlink;
        let link = std::env::temp_dir().join(format!("dkk-runtime-log-link-{}", Uuid::new_v4()));
        let external =
            std::env::temp_dir().join(format!("dkk-runtime-log-external-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&external).unwrap();
        symlink(&external, &link).unwrap();
        let path = link.join(WATCHER_LOG_NAME);
        let error = append_with_policy(&path, "status=blocked", tiny_policy(), SystemTime::now())
            .unwrap_err();
        assert!(error.contains("небезопасный тип"));
        assert!(!external.join(WATCHER_LOG_NAME).exists());
        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_dir_all(external);
    }
}
