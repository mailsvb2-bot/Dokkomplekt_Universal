use super::metadata_is_link_like;
use fs2::FileExt as _;
use std::fs::File;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub(super) const ACTIVE_SESSION_MARKER: &str = ".active";
pub(super) const SESSION_OWNERSHIP_MARKER: &str = ".dokkomplekt-owned-session";
const SESSION_OWNERSHIP_PROOF: &[u8] = b"dokkomplekt-owned-session-v1";
const ACTIVE_SESSION_GRACE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
struct OwnedSessionUsage {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
    live: bool,
}

pub(crate) fn owned_workspace_bytes(workspace: &Path) -> Result<u64, String> {
    owned_workspace_group_bytes(&[workspace.to_path_buf()])
}

pub(crate) fn owned_workspace_group_bytes(workspaces: &[PathBuf]) -> Result<u64, String> {
    Ok(scan_owned_session_group(workspaces)?
        .into_iter()
        .fold(0_u64, |total, session| total.saturating_add(session.bytes)))
}

pub(crate) fn enforce_ephemeral_workspace_quota(
    workspace: &Path,
    retention: Duration,
    max_bytes: u64,
    required_bytes: u64,
) -> Result<usize, String> {
    enforce_ephemeral_workspace_group_quota(
        &[workspace.to_path_buf()],
        retention,
        max_bytes,
        required_bytes,
    )
}

pub(crate) fn enforce_ephemeral_workspace_group_quota(
    workspaces: &[PathBuf],
    retention: Duration,
    max_bytes: u64,
    required_bytes: u64,
) -> Result<usize, String> {
    if max_bytes == 0 || required_bytes > max_bytes {
        return Err(format!(
            "Требуемый резерв временного cache ({required_bytes} байт) превышает квоту {max_bytes} байт."
        ));
    }

    let now = SystemTime::now();
    let mut sessions = scan_owned_session_group(workspaces)?;
    let mut removed = 0_usize;

    // Retention cleanup only touches finished/unlocked sessions.
    let mut retained = Vec::with_capacity(sessions.len());
    for session in sessions.drain(..) {
        let expired = now
            .duration_since(session.modified)
            .ok()
            .is_some_and(|age| age >= retention);
        if !session.live && expired && !session_has_live_process_lease(&session.path)? {
            remove_sensitive_session(&session.path)?;
            removed += 1;
        } else {
            retained.push(session);
        }
    }

    let mut total = retained
        .iter()
        .fold(0_u64, |sum, session| sum.saturating_add(session.bytes));
    if total.saturating_add(required_bytes) <= max_bytes {
        return Ok(removed);
    }

    // Under quota pressure, evict only completed (unlocked) owned sessions,
    // oldest first across the whole logical cache class. Live sessions are
    // never candidates regardless of which backing workspace owns them.
    retained.sort_by_key(|session| session.modified);
    for session in retained.iter().filter(|session| !session.live) {
        if session_has_live_process_lease(&session.path)? {
            continue;
        }
        remove_sensitive_session(&session.path)?;
        total = total.saturating_sub(session.bytes);
        removed += 1;
        if total.saturating_add(required_bytes) <= max_bytes {
            return Ok(removed);
        }
    }

    Err(format!(
        "Временный cache занят активными сессиями: требуется {} байт при общей квоте {} байт и текущем защищённом объёме {} байт.",
        required_bytes, max_bytes, total
    ))
}

pub(crate) fn enforce_retained_workspace_quota(
    workspace: &Path,
    retention: Duration,
    max_bytes: u64,
    required_bytes: u64,
) -> Result<usize, String> {
    if max_bytes == 0 || required_bytes > max_bytes {
        return Err(format!(
            "Требуемый резерв retained workspace ({required_bytes} байт) превышает квоту {max_bytes} байт."
        ));
    }

    let now = SystemTime::now();
    let mut sessions = scan_owned_sessions(workspace)?;
    let mut removed = 0_usize;
    let mut retained = Vec::with_capacity(sessions.len());

    for session in sessions.drain(..) {
        let expired = now
            .duration_since(session.modified)
            .ok()
            .is_some_and(|age| age >= retention);
        let active = session.live || active_session_is_recent(&session.path, now)?;
        if !active && expired {
            remove_sensitive_session(&session.path)?;
            removed += 1;
        } else {
            retained.push(session);
        }
    }

    let total = retained
        .iter()
        .fold(0_u64, |sum, session| sum.saturating_add(session.bytes));
    if total.saturating_add(required_bytes) <= max_bytes {
        return Ok(removed);
    }

    // Retained learning inputs are user-selected working state, not a disposable
    // cache. Quota pressure must never evict an unexpired retained session merely
    // because its short activity heartbeat is stale. If expired inactive sessions
    // were insufficient to make room, fail admission and preserve the remaining
    // recoverable user state.
    Err(format!(
        "Retained workspace занят активными или ещё не просроченными сессиями: требуется {required_bytes} байт при квоте {max_bytes} байт и защищённом объёме {total} байт."
    ))
}

fn scan_owned_session_group(workspaces: &[PathBuf]) -> Result<Vec<OwnedSessionUsage>, String> {
    let mut unique = std::collections::BTreeSet::new();
    let mut sessions = Vec::new();
    for workspace in workspaces {
        if unique.insert(workspace.clone()) {
            sessions.extend(scan_owned_sessions(workspace)?);
        }
    }
    Ok(sessions)
}

fn scan_owned_sessions(workspace: &Path) -> Result<Vec<OwnedSessionUsage>, String> {
    if !validate_existing_workspace_root(workspace)? {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(workspace).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata_is_link_like(&metadata) {
            return Err(format!(
                "Квота временного cache не применяется: обнаружена ссылка/reparse point {}.",
                path.display()
            ));
        }
        if !metadata.is_dir() || !session_has_verified_ownership(&path)? {
            continue;
        }
        sessions.push(OwnedSessionUsage {
            bytes: owned_session_size(&path)?,
            modified: metadata.modified().unwrap_or(UNIX_EPOCH),
            live: session_has_live_process_lease(&path)?,
            path,
        });
    }
    Ok(sessions)
}

fn owned_session_size(root: &Path) -> Result<u64, String> {
    let metadata = std::fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if metadata_is_link_like(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Owned cache session имеет небезопасный тип: {}",
            root.display()
        ));
    }

    let mut total = 0_u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata_is_link_like(&metadata) {
                return Err(format!(
                    "Owned cache session содержит ссылку/reparse point: {}",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
pub(crate) fn cleanup_workspace(workspace: &Path, max_age: Duration) -> Result<usize, String> {
    if !validate_existing_workspace_root(workspace)? {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut owned_sessions = Vec::new();
    for entry in std::fs::read_dir(workspace).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;

        // Automatic cleanup is intentionally conservative: an unexpected link,
        // junction or reparse point inside an app-owned workspace makes ownership
        // ambiguous, so nothing is deleted in this pass.
        if metadata_is_link_like(&metadata) {
            return Err(format!(
                "Очистка временных данных остановлена: обнаружена ссылка/reparse point {}.",
                path.display()
            ));
        }

        // Unknown files/directories are not cache merely because they happen to be
        // located under the workspace root. Only sessions carrying the exact
        // ownership proof written by create_sensitive_session are eligible.
        if !metadata.is_dir() || !session_has_verified_ownership(&path)? {
            continue;
        }
        owned_sessions.push((path, metadata));
    }

    let mut removed = 0usize;
    for (path, metadata) in owned_sessions {
        if active_session_is_recent(&path, now)? {
            continue;
        }
        let old_enough = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if !old_enough {
            continue;
        }
        std::fs::remove_dir_all(&path).map_err(|error| {
            format!(
                "Не удалось удалить подтверждённую временную сессию {}: {error}",
                path.display()
            )
        })?;
        removed += 1;
    }
    Ok(removed)
}

fn validate_existing_workspace_root(workspace: &Path) -> Result<bool, String> {
    let metadata = match std::fs::symlink_metadata(workspace) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if metadata_is_link_like(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Временный workspace имеет небезопасный тип и не используется: {}",
            workspace.display()
        ));
    }
    Ok(true)
}

fn session_has_verified_ownership(path: &Path) -> Result<bool, String> {
    let marker = path.join(SESSION_OWNERSHIP_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if metadata_is_link_like(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Маркер ownership временной сессии имеет небезопасный тип: {}",
            marker.display()
        ));
    }
    if metadata.len() > 128 {
        return Err(format!(
            "Маркер ownership временной сессии имеет недопустимый размер: {}",
            marker.display()
        ));
    }
    let proof = std::fs::read(&marker).map_err(|error| error.to_string())?;
    Ok(proof == SESSION_OWNERSHIP_PROOF)
}

fn active_session_is_recent(path: &Path, now: SystemTime) -> Result<bool, String> {
    if session_has_live_process_lease(path)? {
        return Ok(true);
    }

    let marker = path.join(ACTIVE_SESSION_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if metadata_is_link_like(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Маркер активности временной сессии имеет небезопасный тип: {}",
            marker.display()
        ));
    }
    Ok(metadata
        .modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age < ACTIVE_SESSION_GRACE))
}

fn session_has_live_process_lease(path: &Path) -> Result<bool, String> {
    let marker = path.join(ACTIVE_SESSION_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if metadata_is_link_like(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Маркер активности временной сессии имеет небезопасный тип: {}",
            marker.display()
        ));
    }

    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&marker)
        .map_err(|error| format!("Не удалось проверить lease временной сессии: {error}"))?;
    match lease.try_lock_exclusive() {
        Ok(()) => {
            let _ = lease.unlock();
            Ok(false)
        }
        Err(error) if lock_error_is_contended(&error) => Ok(true),
        Err(error) => Err(format!(
            "Не удалось безопасно проверить lease временной сессии: {error}"
        )),
    }
}

fn lock_error_is_contended(error: &std::io::Error) -> bool {
    let expected = fs2::lock_contended_error();
    match (error.raw_os_error(), expected.raw_os_error()) {
        (Some(actual), Some(contended)) => actual == contended,
        _ => error.kind() == expected.kind(),
    }
}

#[derive(Debug)]
pub(crate) struct OwnedWorkspaceSession {
    root: PathBuf,
    active_lease: Option<File>,
}

impl OwnedWorkspaceSession {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for OwnedWorkspaceSession {
    fn drop(&mut self) {
        if let Some(lease) = self.active_lease.take() {
            let _ = lease.unlock();
            drop(lease);
        }
        let _ = remove_sensitive_session(&self.root);
    }
}

pub(crate) fn create_owned_workspace_session(
    workspace: &Path,
) -> Result<OwnedWorkspaceSession, String> {
    let (root, active_lease) = create_sensitive_session_with_lease(workspace)?;
    Ok(OwnedWorkspaceSession {
        root,
        active_lease: Some(active_lease),
    })
}

pub(crate) fn create_retained_workspace_session(workspace: &Path) -> Result<PathBuf, String> {
    create_sensitive_session(workspace)
}

pub(crate) fn create_completed_retained_workspace_file(
    workspace: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let file_component = Path::new(file_name);
    if file_component.components().count() != 1
        || !matches!(
            file_component.components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err("Имя retained-файла должно быть одним безопасным компонентом пути.".into());
    }
    let (root, active_lease) = create_sensitive_session_with_lease(workspace)?;
    let destination = root.join(file_component);
    let write_result = (|| -> Result<(), String> {
        if !session_has_verified_ownership(&root)? {
            return Err("Retained-сессия не содержит ownership proof.".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| format!("Не удалось создать retained-файл: {error}"))?;
        restrict_file_permissions(&destination)?;
        file.write_all(bytes)
            .map_err(|error| format!("Не удалось записать retained-файл: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Не удалось синхронизировать retained-файл: {error}"))?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = active_lease.unlock();
        drop(active_lease);
        let _ = remove_sensitive_session(&root);
        return Err(error);
    }

    if let Err(error) = active_lease
        .unlock()
        .map_err(|error| format!("Не удалось завершить lease retained-сессии: {error}"))
    {
        drop(active_lease);
        let _ = remove_sensitive_session(&root);
        return Err(error);
    }
    drop(active_lease);

    let active = root.join(ACTIVE_SESSION_MARKER);
    let finalize_result = (|| -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(&active).map_err(|error| {
            format!("Не удалось проверить active marker retained-сессии: {error}")
        })?;
        if metadata_is_link_like(&metadata) || !metadata.is_file() {
            return Err("Active marker retained-сессии имеет небезопасный тип.".into());
        }
        std::fs::remove_file(&active)
            .map_err(|error| format!("Не удалось завершить retained-сессию: {error}"))
    })();
    if let Err(error) = finalize_result {
        let _ = remove_sensitive_session(&root);
        return Err(error);
    }
    Ok(destination)
}

pub(crate) fn list_owned_workspace_files(
    workspace: &Path,
    file_name: &str,
    limit: usize,
) -> Result<Vec<PathBuf>, String> {
    let file_component = Path::new(file_name);
    if file_component.components().count() != 1
        || !matches!(
            file_component.components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err("Имя retained-файла должно быть одним безопасным компонентом пути.".into());
    }
    let mut sessions = scan_owned_sessions(workspace)?;
    sessions.sort_by_key(|session| std::cmp::Reverse(session.modified));
    let mut files = Vec::new();
    for session in sessions
        .into_iter()
        .filter(|session| !session.live)
        .take(limit.clamp(1, 1_000))
    {
        let path = session.path.join(file_component);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if metadata_is_link_like(&metadata) || !metadata.is_file() {
            return Err(format!(
                "Owned retained-файл имеет небезопасный тип: {}",
                path.display()
            ));
        }
        files.push(path);
    }
    Ok(files)
}

pub(crate) fn refresh_retained_workspace_session(
    workspace: &Path,
    path: &Path,
) -> Result<bool, String> {
    let Ok(relative) = path.strip_prefix(workspace) else {
        return Ok(false);
    };
    let Some(Component::Normal(session_name)) = relative.components().next() else {
        return Ok(false);
    };
    let session_name = session_name.to_string_lossy();
    if !session_name.starts_with("session-") {
        return Ok(false);
    }
    let session_root = workspace.join(session_name.as_ref());
    let metadata = std::fs::symlink_metadata(&session_root)
        .map_err(|error| format!("Учебная сессия недоступна: {error}"))?;
    if metadata_is_link_like(&metadata) || !metadata.is_dir() {
        return Err("Учебная сессия имеет небезопасный тип файла.".into());
    }
    if !session_has_verified_ownership(&session_root)? {
        return Err("Учебная сессия не содержит подтверждённый ownership marker.".into());
    }
    let marker = session_root.join(ACTIVE_SESSION_MARKER);
    std::fs::write(&marker, b"active")
        .map_err(|error| format!("Не удалось продлить учебную сессию: {error}"))?;
    restrict_file_permissions(&marker)?;
    Ok(true)
}

pub(super) fn create_sensitive_session_with_lease(
    workspace: &Path,
) -> Result<(PathBuf, File), String> {
    let root = create_sensitive_session(workspace)?;
    let marker = root.join(ACTIVE_SESSION_MARKER);
    let lease = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&marker)
        .map_err(|error| format!("Не удалось открыть lease временной сессии: {error}"))
        .and_then(|file| {
            file.lock_exclusive().map_err(|error| {
                format!("Не удалось зафиксировать lease временной сессии: {error}")
            })?;
            Ok(file)
        }) {
        Ok(lease) => lease,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&root);
            return Err(error);
        }
    };
    Ok((root, lease))
}

pub(super) fn create_sensitive_session(workspace: &Path) -> Result<PathBuf, String> {
    if !validate_existing_workspace_root(workspace)? {
        std::fs::create_dir_all(workspace).map_err(|error| error.to_string())?;
        if !validate_existing_workspace_root(workspace)? {
            return Err("Не удалось подтвердить безопасный временный workspace.".into());
        }
    }

    let root = workspace.join(format!("session-{}", Uuid::new_v4()));
    std::fs::create_dir(&root)
        .map_err(|error| format!("Не удалось создать защищённую временную сессию: {error}"))?;
    restrict_directory_permissions(&root)?;

    let ownership = root.join(SESSION_OWNERSHIP_MARKER);
    if let Err(error) = std::fs::write(&ownership, SESSION_OWNERSHIP_PROOF)
        .map_err(|error| format!("Не удалось создать ownership marker временной сессии: {error}"))
        .and_then(|_| restrict_file_permissions(&ownership))
    {
        let _ = std::fs::remove_dir_all(&root);
        return Err(error);
    }

    let marker = root.join(ACTIVE_SESSION_MARKER);
    if let Err(error) = std::fs::write(&marker, b"active")
        .map_err(|error| format!("Не удалось создать маркер временной сессии: {error}"))
        .and_then(|_| restrict_file_permissions(&marker))
    {
        let _ = std::fs::remove_dir_all(&root);
        return Err(error);
    }
    Ok(root)
}

#[cfg(unix)]
pub(super) fn restrict_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Не удалось ограничить доступ к временной папке: {error}"))
}

#[cfg(not(unix))]
pub(super) fn restrict_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Не удалось ограничить доступ к временному файлу: {error}"))
}

#[cfg(not(unix))]
pub(super) fn restrict_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) fn remove_sensitive_session(root: &Path) -> Result<(), String> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata_is_link_like(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Временная сессия имеет небезопасный тип и не удалена: {}",
            root.display()
        ));
    }
    if !session_has_verified_ownership(root)? {
        return Err(format!(
            "Временная сессия без подтверждённого ownership marker не удалена: {}",
            root.display()
        ));
    }
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_quota_is_shared_across_backing_workspaces() {
        let root = std::env::temp_dir().join(format!("dkk-cache-group-quota-{}", Uuid::new_v4()));
        let first_workspace = root.join("first");
        let second_workspace = root.join("second");

        let released = create_sensitive_session(&first_workspace).unwrap();
        std::fs::write(released.join("released.bin"), vec![b'r'; 128]).unwrap();

        let (live, lease) = create_sensitive_session_with_lease(&second_workspace).unwrap();
        std::fs::write(live.join("live.bin"), vec![b'l'; 128]).unwrap();
        let live_bytes = owned_session_size(&live).unwrap();

        let roots = vec![first_workspace.clone(), second_workspace.clone()];
        let removed = enforce_ephemeral_workspace_group_quota(
            &roots,
            Duration::from_secs(24 * 60 * 60),
            live_bytes + 1,
            1,
        )
        .unwrap();

        assert_eq!(removed, 1);
        assert!(!released.exists());
        assert!(live.is_dir());
        assert_eq!(owned_workspace_group_bytes(&roots).unwrap(), live_bytes);

        lease.unlock().unwrap();
        drop(lease);
        remove_sensitive_session(&live).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn quota_pressure_evicts_released_owned_cache_but_never_live_session() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-cache-quota-live-{}", Uuid::new_v4()));

        let released = create_sensitive_session(&workspace).unwrap();
        std::fs::write(released.join("released.bin"), vec![b'r'; 128]).unwrap();

        let (live, lease) = create_sensitive_session_with_lease(&workspace).unwrap();
        std::fs::write(live.join("live.bin"), vec![b'l'; 128]).unwrap();
        let live_bytes = owned_session_size(&live).unwrap();

        let removed = enforce_ephemeral_workspace_quota(
            &workspace,
            Duration::from_secs(24 * 60 * 60),
            live_bytes + 1,
            1,
        )
        .unwrap();

        assert_eq!(removed, 1);
        assert!(!released.exists());
        assert!(live.is_dir());

        lease.unlock().unwrap();
        drop(lease);
        remove_sensitive_session(&live).unwrap();
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn retained_quota_never_evicts_recent_unleased_session_under_pressure() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-retained-quota-recent-{}", Uuid::new_v4()));
        let retained = create_sensitive_session(&workspace).unwrap();
        std::fs::write(retained.join("retained.bin"), vec![b'r'; 128]).unwrap();
        let retained_bytes = owned_session_size(&retained).unwrap();

        let error = enforce_retained_workspace_quota(
            &workspace,
            Duration::from_secs(24 * 60 * 60),
            retained_bytes,
            1,
        )
        .unwrap_err();
        assert!(error.contains("активными или ещё не просроченными"));
        assert!(retained.is_dir());

        std::fs::remove_file(retained.join(ACTIVE_SESSION_MARKER)).unwrap();
        let unleased_bytes = owned_session_size(&retained).unwrap();
        let still_protected = enforce_retained_workspace_quota(
            &workspace,
            Duration::from_secs(24 * 60 * 60),
            unleased_bytes,
            1,
        )
        .unwrap_err();
        assert!(still_protected.contains("ещё не просроченными"));
        assert!(retained.is_dir());

        assert_eq!(
            enforce_retained_workspace_quota(&workspace, Duration::ZERO, unleased_bytes, 1)
                .unwrap(),
            1
        );
        assert!(!retained.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn quota_fails_closed_when_live_session_consumes_reserved_capacity() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-cache-quota-block-{}", Uuid::new_v4()));
        let (live, lease) = create_sensitive_session_with_lease(&workspace).unwrap();
        std::fs::write(live.join("live.bin"), vec![b'l'; 64]).unwrap();
        let live_bytes = owned_session_size(&live).unwrap();

        let error = enforce_ephemeral_workspace_quota(
            &workspace,
            Duration::from_secs(24 * 60 * 60),
            live_bytes,
            1,
        )
        .unwrap_err();

        assert!(error.contains("активными сессиями"));
        assert!(live.is_dir());

        lease.unlock().unwrap();
        drop(lease);
        remove_sensitive_session(&live).unwrap();
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn retained_file_listing_never_exposes_live_cross_process_session() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-retained-live-list-{}", Uuid::new_v4()));
        let (root, lease) = create_sensitive_session_with_lease(&workspace).unwrap();
        let trace = root.join("trace.json");
        std::fs::write(&trace, b"complete-but-not-committed").unwrap();

        assert!(list_owned_workspace_files(&workspace, "trace.json", 10)
            .unwrap()
            .is_empty());

        lease.unlock().unwrap();
        drop(lease);
        std::fs::remove_file(root.join(ACTIVE_SESSION_MARKER)).unwrap();
        assert_eq!(
            list_owned_workspace_files(&workspace, "trace.json", 10).unwrap(),
            vec![trace]
        );

        remove_sensitive_session(&root).unwrap();
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn completed_retained_file_is_owned_listable_and_quota_evictable() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-retained-completed-{}", Uuid::new_v4()));
        let path =
            create_completed_retained_workspace_file(&workspace, "trace.json", b"{\"ok\":true}")
                .unwrap();

        assert!(path.is_file());
        assert!(!path.parent().unwrap().join(ACTIVE_SESSION_MARKER).exists());
        assert_eq!(
            list_owned_workspace_files(&workspace, "trace.json", 10).unwrap(),
            vec![path.clone()]
        );
        assert!(owned_workspace_bytes(&workspace).unwrap() > 0);

        assert_eq!(
            enforce_ephemeral_workspace_quota(&workspace, Duration::ZERO, 1024, 0).unwrap(),
            1
        );
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn retained_file_listing_ignores_unknown_workspace_content() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-retained-list-unknown-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let unknown_dir = workspace.join("session-lookalike");
        std::fs::create_dir_all(&unknown_dir).unwrap();
        let unknown = unknown_dir.join("trace.json");
        std::fs::write(&unknown, b"user-owned").unwrap();

        assert!(list_owned_workspace_files(&workspace, "trace.json", 10)
            .unwrap()
            .is_empty());
        assert_eq!(std::fs::read(&unknown).unwrap(), b"user-owned");
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn unknown_workspace_entries_are_not_counted_or_deleted_by_cache_quota() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-cache-quota-unknown-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let unknown = workspace.join("user-owned.bin");
        std::fs::write(&unknown, vec![b'u'; 4096]).unwrap();

        assert_eq!(owned_workspace_bytes(&workspace).unwrap(), 0);
        assert_eq!(
            enforce_ephemeral_workspace_quota(&workspace, Duration::ZERO, 1024, 512,).unwrap(),
            0
        );
        assert_eq!(std::fs::metadata(&unknown).unwrap().len(), 4096);

        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn fs2_contended_error_is_recognized_for_a_real_held_lease() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-session-contended-{}", Uuid::new_v4()));
        let (root, lease) = create_sensitive_session_with_lease(&workspace).unwrap();
        let marker = root.join(ACTIVE_SESSION_MARKER);
        let second = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(marker)
            .unwrap();
        let error = second.try_lock_exclusive().unwrap_err();

        assert!(lock_error_is_contended(&error));

        lease.unlock().unwrap();
        drop(lease);
        remove_sensitive_session(&root).unwrap();
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn live_process_lease_is_visible_until_explicit_release() {
        let workspace = std::env::temp_dir().join(format!("dkk-session-lease-{}", Uuid::new_v4()));
        let (root, lease) = create_sensitive_session_with_lease(&workspace).unwrap();

        assert!(session_has_live_process_lease(&root).unwrap());

        lease.unlock().unwrap();
        drop(lease);
        assert!(!session_has_live_process_lease(&root).unwrap());

        remove_sensitive_session(&root).unwrap();
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn cleanup_never_evicts_session_while_os_lease_is_held() {
        let workspace =
            std::env::temp_dir().join(format!("dkk-session-cleanup-lease-{}", Uuid::new_v4()));
        let (root, lease) = create_sensitive_session_with_lease(&workspace).unwrap();

        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 0);
        assert!(root.is_dir());

        lease.unlock().unwrap();
        drop(lease);
        // The marker freshness still protects a just-released retained-style session.
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 0);

        std::fs::remove_file(root.join(ACTIVE_SESSION_MARKER)).unwrap();
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 1);
        assert!(!root.exists());
        let _ = std::fs::remove_dir_all(workspace);
    }
}
