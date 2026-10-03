use super::metadata_is_link_like;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

pub(super) const ACTIVE_SESSION_MARKER: &str = ".active";
pub(super) const SESSION_OWNERSHIP_MARKER: &str = ".dokkomplekt-owned-session";
const SESSION_OWNERSHIP_PROOF: &[u8] = b"dokkomplekt-owned-session-v1";
const ACTIVE_SESSION_GRACE: Duration = Duration::from_secs(30 * 60);

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

pub(crate) fn create_retained_workspace_session(workspace: &Path) -> Result<PathBuf, String> {
    create_sensitive_session(workspace)
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
