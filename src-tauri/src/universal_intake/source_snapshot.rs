use super::{
    create_sensitive_session, restrict_file_permissions, safe_file_name, validate_source_file_size,
    UploadedSourceSession, MAX_SOURCE_FILE_BYTES,
};
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
struct SourceFileSignature {
    size_bytes: u64,
    modified_unix_ms: u128,
    sha256: String,
}

#[derive(Debug)]
pub struct StableSourceSnapshot {
    _session: UploadedSourceSession,
    path: PathBuf,
    size_bytes: u64,
    modified_unix_ms: u128,
    sha256: String,
}

impl StableSourceSnapshot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn modified_unix_ms(&self) -> u128 {
        self.modified_unix_ms
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

fn source_file_signature(path: &Path) -> Result<SourceFileSignature, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("Источник не является файлом: {}", path.display()));
    }
    if metadata.len() > MAX_SOURCE_FILE_BYTES {
        return Err(format!(
            "Источник превышает безопасный предел {} МБ: {}",
            MAX_SOURCE_FILE_BYTES / (1024 * 1024),
            path.display()
        ));
    }
    let modified_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "Размер источника переполнен.".to_string())?;
        if total > MAX_SOURCE_FILE_BYTES {
            return Err(format!(
                "Источник вырос во время чтения и превысил безопасный предел {} МБ.",
                MAX_SOURCE_FILE_BYTES / (1024 * 1024)
            ));
        }
        hasher.update(&buffer[..read]);
    }
    Ok(SourceFileSignature {
        size_bytes: total,
        modified_unix_ms,
        sha256: hex::encode(hasher.finalize()),
    })
}

fn copy_source_limited(source: &Path, destination: &Path) -> Result<(u64, String), String> {
    let mut input = File::open(source).map_err(|error| error.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| error.to_string())?;
    restrict_file_permissions(destination)?;

    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "Размер snapshot источника переполнен.".to_string())?;
        if total > MAX_SOURCE_FILE_BYTES {
            return Err(format!(
                "Источник вырос во время snapshot и превысил безопасный предел {} МБ.",
                MAX_SOURCE_FILE_BYTES / (1024 * 1024)
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        hasher.update(&buffer[..read]);
    }
    output.flush().map_err(|error| error.to_string())?;
    Ok((total, hex::encode(hasher.finalize())))
}

/// Captures one immutable, private copy of a live watcher source.
///
/// The source is hashed before and after the bounded copy. The snapshot is
/// accepted only when all three views (before/copy/after) are byte-identical.
/// This prevents a writer from changing a document between deduplication,
/// recognition, trust-report hashing and publication.
pub fn capture_stable_source(
    source: &Path,
    workspace: &Path,
) -> Result<StableSourceSnapshot, String> {
    const ATTEMPTS: usize = 6;
    const RETRY_DELAY: Duration = Duration::from_millis(200);

    validate_source_file_size(source)?;
    let source_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("source")
        .to_string();
    let mut last_error = String::new();

    for attempt in 0..ATTEMPTS {
        let before = match source_file_signature(source) {
            Ok(signature) => signature,
            Err(error) => {
                last_error = error;
                if attempt + 1 < ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                    continue;
                }
                break;
            }
        };

        let root = create_sensitive_session(workspace)?;
        let session = UploadedSourceSession { source: None, root };
        let snapshot_path = session.root.join(safe_file_name(&source_name));
        let copied = copy_source_limited(source, &snapshot_path);
        let after = source_file_signature(source);

        match (copied, after) {
            (Ok((copied_size, copied_sha256)), Ok(after))
                if before.size_bytes == after.size_bytes
                    && before.sha256 == after.sha256
                    && copied_size == after.size_bytes
                    && copied_sha256 == after.sha256 =>
            {
                return Ok(StableSourceSnapshot {
                    _session: session,
                    path: snapshot_path,
                    size_bytes: after.size_bytes,
                    modified_unix_ms: after.modified_unix_ms,
                    sha256: after.sha256,
                });
            }
            (Ok((copied_size, copied_sha256)), Ok(after)) => {
                last_error = format!(
                    "источник изменился во время snapshot (до={} байт/{}, копия={} байт/{}, после={} байт/{})",
                    before.size_bytes,
                    before.sha256,
                    copied_size,
                    copied_sha256,
                    after.size_bytes,
                    after.sha256
                );
            }
            (Err(error), _) | (_, Err(error)) => {
                last_error = error;
            }
        }

        drop(session);
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(RETRY_DELAY);
        }
    }

    Err(format!(
        "Источник продолжает изменяться и не может быть безопасно обработан. Повторите после завершения записи файла. Последняя причина: {last_error}"
    ))
}

/// Returns false when the live path disappeared or no longer contains the
/// bytes that were captured into the immutable snapshot.
pub fn current_source_matches(source: &Path, expected_sha256: &str) -> Result<bool, String> {
    match std::fs::metadata(source) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    }
    Ok(source_file_signature(source)?.sha256 == expected_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal_intake::cleanup_workspace;
    use uuid::Uuid;

    #[test]
    fn stable_source_snapshot_is_immutable_and_detects_live_replacement() {
        let root = std::env::temp_dir().join(format!("dkk-source-snapshot-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("case.txt");
        std::fs::write(&source, b"version-one").unwrap();

        let snapshot = capture_stable_source(&source, &workspace).unwrap();
        assert_eq!(snapshot.size_bytes(), b"version-one".len() as u64);
        assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"version-one");
        assert!(current_source_matches(&source, snapshot.sha256()).unwrap());
        assert_eq!(cleanup_workspace(&workspace, Duration::ZERO).unwrap(), 0);

        std::fs::write(&source, b"version-two").unwrap();
        assert!(!current_source_matches(&source, snapshot.sha256()).unwrap());
        assert_eq!(std::fs::read(snapshot.path()).unwrap(), b"version-one");

        drop(snapshot);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_live_source_never_matches_snapshot() {
        let root = std::env::temp_dir().join(format!("dkk-source-missing-{}", Uuid::new_v4()));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("case.txt");
        std::fs::write(&source, b"stable").unwrap();

        let snapshot = capture_stable_source(&source, &workspace).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert!(!current_source_matches(&source, snapshot.sha256()).unwrap());

        drop(snapshot);
        let _ = std::fs::remove_dir_all(root);
    }
}
