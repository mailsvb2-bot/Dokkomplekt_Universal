from pathlib import Path

ui_path = Path("src-tauri/src/universal_intake.rs")
ui = ui_path.read_text(encoding="utf-8")

insert_marker = "\nfn read_file_limited(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, String> {\n"
if ui.count(insert_marker) != 1:
    raise SystemExit("universal intake insertion marker mismatch")

snapshot_code = r'''
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

'''
ui = ui.replace(insert_marker, "\n" + snapshot_code + insert_marker, 1)

test_marker = "#[cfg(test)]\nmod tests {\n    use super::*;\n"
if ui.count(test_marker) != 1:
    raise SystemExit("universal intake test marker mismatch")

tests = r'''#[cfg(test)]
mod tests {
    use super::*;

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
'''
ui = ui.replace(test_marker, tests, 1)
ui_path.write_text(ui, encoding="utf-8")

runtime_path = Path("src-tauri/src/subsystems/automation_runtime.rs")
runtime = runtime_path.read_text(encoding="utf-8")

old = '''fn finalize_processed_source(
    source: &Path,
    source_sha256: &str,
    privacy: &PrivacyPreferences,
    preserve_source_after_success: bool,
) -> Result<serde_json::Value, String> {
    let marker = workspace_hygiene::processed_marker_path(source);
'''
new = '''fn finalize_processed_source(
    source: &Path,
    source_sha256: &str,
    privacy: &PrivacyPreferences,
    preserve_source_after_success: bool,
) -> Result<serde_json::Value, String> {
    if !universal_intake::current_source_matches(source, source_sha256)? {
        return Ok(serde_json::json!({
            "action": "source_changed_or_missing_after_publication_preserved",
            "expected_source_sha256": source_sha256,
        }));
    }
    let marker = workspace_hygiene::processed_marker_path(source);
'''
if runtime.count(old) != 1:
    raise SystemExit("finalize_processed_source marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''fn processing_job_key(source_sha256: &str, processing_fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"dokkomplekt-processing-job-v2\\0");
    hasher.update(source_sha256.as_bytes());
    hasher.update(b"\\0");
    hasher.update(processing_fingerprint.as_bytes());
    hex::encode(hasher.finalize())
}

fn perform_created_documents_intake(
'''
new = '''fn processing_job_key(source_sha256: &str, processing_fingerprint: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"dokkomplekt-processing-job-v2\\0");
    hasher.update(source_sha256.as_bytes());
    hasher.update(b"\\0");
    hasher.update(processing_fingerprint.as_bytes());
    hex::encode(hasher.finalize())
}

fn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {
    match universal_intake::current_source_matches(source, source_sha256) {
        Ok(true) => Ok(()),
        Ok(false) => Err(
            "Исходный файл изменился во время обработки. Устаревший комплект не опубликован; новая версия будет обработана отдельно."
                .into(),
        ),
        Err(error) => Err(format!(
            "Не удалось повторно проверить исходный файл перед публикацией: {error}"
        )),
    }
}

fn perform_created_documents_intake(
'''
if runtime.count(old) != 1:
    raise SystemExit("processing key marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''    let source = resolve_user_path(app, &req.source_path)?;
    let privacy = load_privacy_preferences(app)?;
    let processed_markers = workspace_hygiene::processed_marker_candidates(&source);
    let (source_size, source_modified_ms, source_sha256) = file_content_signature(&source)?;
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
'''
new = '''    let source = resolve_user_path(app, &req.source_path)?;
    let privacy = load_privacy_preferences(app)?;
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("intake-work");
    let source_snapshot = universal_intake::capture_stable_source(&source, &workspace)?;
    let source_size = source_snapshot.size_bytes();
    let source_modified_ms = source_snapshot.modified_unix_ms();
    let source_sha256 = source_snapshot.sha256().to_string();
    let processed_markers = workspace_hygiene::processed_marker_candidates(&source);
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
'''
if runtime.count(old) != 1:
    raise SystemExit("intake source signature marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''    // Each dropped source is an independent case. Every accepted format is first
    // normalized into one bounded text representation; no previous case values are reused.
    let workspace = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("intake-work");
    let normalized = universal_intake::normalize_path(&source, &workspace, 0)?;
'''
new = '''    // Each dropped source is an independent case. Every accepted format is first
    // normalized from the immutable private snapshot, never from a live file that
    // Word, a scanner or a sync client may still be replacing underneath us.
    let normalized = universal_intake::normalize_path(source_snapshot.path(), &workspace, 0)?;
'''
if runtime.count(old) != 1:
    raise SystemExit("normalization workspace marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''                if privacy.copy_source_to_output {
                    std::fs::copy(&source, stage.join(&source_target_name))
                        .map_err(|e| format!("Не удалось скопировать исходник в комплект: {e}"))?;
                }
'''
new = '''                if privacy.copy_source_to_output {
                    std::fs::copy(source_snapshot.path(), stage.join(&source_target_name))
                        .map_err(|e| format!("Не удалось скопировать snapshot исходника в комплект: {e}"))?;
                }
'''
if runtime.count(old) != 1:
    raise SystemExit("copy source marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''            case_run.transition("publishing")?;
            if let Some(lease) = central_queue_lease.as_mut() {
'''
new = '''            if let Err(error) = ensure_source_snapshot_current(&source, &source_sha256) {
                let _ = std::fs::remove_dir_all(&stage);
                rollback_counter_reservations(app, &counter_reservations);
                rollback_generation_access(app, state, &permit);
                let _ = case_run.finish("superseded", None, &[], &[], Some(&error));
                let _ = append_audit_event(
                    app,
                    "intake_source_superseded",
                    &source_sha256,
                    &serde_json::json!({ "stage": "before_publication", "error": &error }),
                );
                return Err(error);
            }
            case_run.transition("publishing")?;
            if let Some(lease) = central_queue_lease.as_mut() {
'''
if runtime.count(old) != 1:
    raise SystemExit("pre-publication marker mismatch")
runtime = runtime.replace(old, new, 1)

old = '''            if let Err(error) = commit_generation_access(app, &permit) {
'''
new = '''            if let Err(error) = ensure_source_snapshot_current(&source, &source_sha256) {
                let _ = std::fs::remove_dir_all(&patient_dir);
                rollback_counter_reservations(app, &counter_reservations);
                rollback_generation_access(app, state, &permit);
                let _ = case_run.finish("superseded", None, &[], &[], Some(&error));
                let _ = append_audit_event(
                    app,
                    "intake_source_superseded",
                    &source_sha256,
                    &serde_json::json!({ "stage": "after_directory_publish", "error": &error }),
                );
                return Err(error);
            }
            if let Err(error) = commit_generation_access(app, &permit) {
'''
if runtime.count(old) != 1:
    raise SystemExit("post-publication marker mismatch")
runtime = runtime.replace(old, new, 1)

runtime_path.write_text(runtime, encoding="utf-8")

doc_path = Path("docs/CRASH_CONSISTENCY.md")
doc = doc_path.read_text(encoding="utf-8")
marker = "## Generated documents\n"
if doc.count(marker) != 1:
    raise SystemExit("crash consistency doc marker mismatch")
section = '''## Live source stability

Watcher intake first captures the source into a private active-session snapshot and proves that the bytes before, during, and after the copy are identical. Recognition, trust-report hashing and optional source-copy publication all use that immutable snapshot. The live source is checked again before publication and after the patient directory becomes visible; a changed source aborts the stale publication and rolls back explicit reservations. After a successful publication, destructive archive/delete hygiene is skipped when the live source no longer matches the processed SHA-256, so a newly replaced source is never deleted as if it were the old case.

'''
doc = doc.replace(marker, section + marker, 1)
doc_path.write_text(doc, encoding="utf-8")
