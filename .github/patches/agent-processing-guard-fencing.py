from pathlib import Path

main_path = Path('src-tauri/src/main.rs')
main = main_path.read_text(encoding='utf-8')

# 1) Add nonce-aware helper paths and owner/release verification.
marker = '''struct ProcessingGuard {
    marker: PathBuf,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_thread: Option<std::thread::JoinHandle<()>>,
}
'''
replacement = '''fn processing_owner_nonce(owner_text: &str) -> Option<&str> {
    owner_text
        .lines()
        .find_map(|line| line.strip_prefix("nonce="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn processing_heartbeat_path(marker: &Path, nonce: &str) -> PathBuf {
    marker.join(format!("heartbeat-{nonce}"))
}

fn processing_release_path(marker: &Path, nonce: &str) -> PathBuf {
    marker.join(format!("released-{nonce}"))
}

fn processing_owner_matches(marker: &Path, expected_nonce: &str) -> bool {
    std::fs::read_to_string(marker.join("owner"))
        .ok()
        .and_then(|text| processing_owner_nonce(&text).map(str::to_owned))
        .is_some_and(|nonce| nonce == expected_nonce)
}

fn processing_release_matches(marker: &Path, expected_nonce: &str) -> bool {
    std::fs::read_to_string(processing_release_path(marker, expected_nonce))
        .ok()
        .is_some_and(|text| {
            text.lines()
                .any(|line| line.strip_prefix("nonce=") == Some(expected_nonce))
        })
}

fn processing_claim_heartbeat_path(marker: &Path, owner_text: &str) -> PathBuf {
    let schema = owner_text
        .lines()
        .find_map(|line| line.strip_prefix("schema="))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_default();
    if schema >= 3 {
        if let Some(nonce) = processing_owner_nonce(owner_text) {
            return processing_heartbeat_path(marker, nonce);
        }
    }
    marker.join("heartbeat")
}

struct ProcessingGuard {
    marker: PathBuf,
    owner_nonce: String,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_thread: Option<std::thread::JoinHandle<()>>,
}
'''
if main.count(marker) != 1:
    raise SystemExit(f'ProcessingGuard struct marker mismatch: {main.count(marker)}')
main = main.replace(marker, replacement, 1)

# 2) Existing heartbeat path is now derived per owner after reading the owner nonce.
old = '''        let owner_path = marker.join("owner");
        let heartbeat_path = marker.join("heartbeat");
        let current_host = processing_lock_host_id();
'''
new = '''        let owner_path = marker.join("owner");
        let current_host = processing_lock_host_id();
'''
if main.count(old) != 1:
    raise SystemExit('processing guard path marker mismatch')
main = main.replace(old, new, 1)

# 3) New claims use schema 3 and nonce-specific heartbeats; the heartbeat thread fences itself.
old = '''                    let owner = format!(
                        "schema=2\\nhost={current_host}\\npid={}\\ncreated_unix={}\\nnonce={nonce}\\n",
                        std::process::id(),
                        unix_now_seconds(),
                    );
                    if let Err(error) = std::fs::write(&owner_path, &owner) {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(format!("Не удалось записать владельца блокировки: {error}"));
                    }
                    if let Err(error) =
                        std::fs::write(&heartbeat_path, unix_now_seconds().to_string())
                    {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(format!(
                            "Не удалось запустить heartbeat блокировки: {error}"
                        ));
                    }
                    let verified = std::fs::read_to_string(&owner_path)
                        .ok()
                        .is_some_and(|text| text.lines().any(|line| line == expected_nonce));
                    if !verified {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(
                            "Сетевая папка не подтвердила владельца блокировки источника.".into(),
                        );
                    }
                    let heartbeat_stop = Arc::new(AtomicBool::new(false));
                    let thread_stop = Arc::clone(&heartbeat_stop);
                    let thread_path = heartbeat_path.clone();
                    let heartbeat_thread = std::thread::spawn(move || {
                        while !thread_stop.load(Ordering::SeqCst) {
                            for _ in 0..30 {
                                if thread_stop.load(Ordering::SeqCst) {
                                    return;
                                }
                                std::thread::sleep(Duration::from_secs(1));
                            }
                            if std::fs::write(&thread_path, unix_now_seconds().to_string()).is_err()
                            {
                                return;
                            }
                        }
                    });
                    return Ok(Some(Self {
                        marker,
                        heartbeat_stop,
                        heartbeat_thread: Some(heartbeat_thread),
                    }));
'''
new = '''                    let owner = format!(
                        "schema=3\\nhost={current_host}\\npid={}\\ncreated_unix={}\\nnonce={nonce}\\n",
                        std::process::id(),
                        unix_now_seconds(),
                    );
                    if let Err(error) = std::fs::write(&owner_path, &owner) {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(format!("Не удалось записать владельца блокировки: {error}"));
                    }
                    let heartbeat_path = processing_heartbeat_path(&marker, &nonce);
                    if let Err(error) =
                        std::fs::write(&heartbeat_path, unix_now_seconds().to_string())
                    {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(format!(
                            "Не удалось запустить heartbeat блокировки: {error}"
                        ));
                    }
                    let verified = processing_owner_matches(&marker, &nonce);
                    if !verified {
                        let _ = std::fs::remove_dir_all(&marker);
                        return Err(
                            "Сетевая папка не подтвердила владельца блокировки источника.".into(),
                        );
                    }
                    let heartbeat_stop = Arc::new(AtomicBool::new(false));
                    let thread_stop = Arc::clone(&heartbeat_stop);
                    let thread_marker = marker.clone();
                    let thread_nonce = nonce.clone();
                    let thread_path = heartbeat_path.clone();
                    let heartbeat_thread = std::thread::spawn(move || {
                        while !thread_stop.load(Ordering::SeqCst) {
                            for _ in 0..30 {
                                if thread_stop.load(Ordering::SeqCst) {
                                    return;
                                }
                                std::thread::sleep(Duration::from_secs(1));
                            }
                            if !processing_owner_matches(&thread_marker, &thread_nonce) {
                                return;
                            }
                            if std::fs::write(&thread_path, unix_now_seconds().to_string()).is_err()
                            {
                                return;
                            }
                            if !processing_owner_matches(&thread_marker, &thread_nonce) {
                                return;
                            }
                        }
                    });
                    return Ok(Some(Self {
                        marker,
                        owner_nonce: nonce,
                        heartbeat_stop,
                        heartbeat_thread: Some(heartbeat_thread),
                    }));
'''
if main.count(old) != 1:
    raise SystemExit('new claim heartbeat marker mismatch')
main = main.replace(old, new, 1)

# 4) Existing claim inspection respects owner-specific heartbeat/release files.
old = '''                    let text = std::fs::read_to_string(&owner_path).unwrap_or_default();
                    let owner_host = text
                        .lines()
                        .find_map(|line| line.strip_prefix("host="))
                        .map(str::to_owned);
                    let pid = text
                        .lines()
                        .find_map(|line| line.strip_prefix("pid="))
                        .and_then(|value| value.parse::<u32>().ok());
                    let lease_age = std::fs::metadata(&heartbeat_path)
                        .or_else(|_| std::fs::metadata(&marker))
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| {
                            std::time::SystemTime::now().duration_since(modified).ok()
                        });
                    let same_host = owner_host.as_deref() == Some(current_host.as_str());
                    if same_host && pid.is_some_and(process_is_alive) {
                        return Ok(None);
                    }
                    if !same_host
                        && owner_host.is_some()
                        && lease_age.is_none_or(|age| age <= REMOTE_LEASE_TIMEOUT)
                    {
                        return Ok(None);
                    }
                    if owner_host.is_none()
                        && lease_age.is_none_or(|age| age <= LEGACY_LEASE_TIMEOUT)
                    {
                        return Ok(None);
                    }
                    std::fs::remove_dir_all(&marker).map_err(|remove_error| {
                        format!("Не удалось удалить истёкшую блокировку источника: {remove_error}")
                    })?;
'''
new = '''                    let text = std::fs::read_to_string(&owner_path).unwrap_or_default();
                    let owner_host = text
                        .lines()
                        .find_map(|line| line.strip_prefix("host="))
                        .map(str::to_owned);
                    let owner_nonce = processing_owner_nonce(&text).map(str::to_owned);
                    let pid = text
                        .lines()
                        .find_map(|line| line.strip_prefix("pid="))
                        .and_then(|value| value.parse::<u32>().ok());
                    let heartbeat_path = processing_claim_heartbeat_path(&marker, &text);
                    let lease_age = std::fs::metadata(&heartbeat_path)
                        .or_else(|_| std::fs::metadata(&marker))
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|modified| {
                            std::time::SystemTime::now().duration_since(modified).ok()
                        });
                    let explicitly_released = owner_nonce
                        .as_deref()
                        .is_some_and(|nonce| processing_release_matches(&marker, nonce));
                    let same_host = owner_host.as_deref() == Some(current_host.as_str());
                    if !explicitly_released && same_host && pid.is_some_and(process_is_alive) {
                        return Ok(None);
                    }
                    if !explicitly_released
                        && !same_host
                        && owner_host.is_some()
                        && lease_age.is_none_or(|age| age <= REMOTE_LEASE_TIMEOUT)
                    {
                        return Ok(None);
                    }
                    if !explicitly_released
                        && owner_host.is_none()
                        && lease_age.is_none_or(|age| age <= LEGACY_LEASE_TIMEOUT)
                    {
                        return Ok(None);
                    }
                    let quarantine = claims_dir.join(format!(
                        ".{source_sha256}.reclaim-{}",
                        Uuid::new_v4()
                    ));
                    match std::fs::rename(&marker, &quarantine) {
                        Ok(()) => {
                            std::fs::remove_dir_all(&quarantine).map_err(|remove_error| {
                                format!("Не удалось очистить перехваченную блокировку источника: {remove_error}")
                            })?;
                        }
                        Err(rename_error)
                            if rename_error.kind() == std::io::ErrorKind::NotFound =>
                        {
                            continue;
                        }
                        Err(rename_error) => {
                            return Err(format!(
                                "Не удалось атомарно перехватить истёкшую блокировку источника: {rename_error}"
                            ));
                        }
                    }
'''
if main.count(old) != 1:
    raise SystemExit('existing claim inspection marker mismatch')
main = main.replace(old, new, 1)

# 5) Add synchronous fencing check and make Drop release-by-nonce instead of deleting a path that may belong to a successor.
old = '''        Err("Не удалось восстановить блокировку источника после сбоя.".into())
    }
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.heartbeat_thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_dir_all(&self.marker);
    }
}

/// Current UTC year from the system clock, std-only (civil-from-days algorithm).
'''
new = '''        Err("Не удалось восстановить блокировку источника после сбоя.".into())
    }

    fn ensure_current(&self) -> Result<(), String> {
        if !processing_owner_matches(&self.marker, &self.owner_nonce) {
            return Err(
                "Блокировка обработки была передана другому экземпляру; устаревший результат не опубликован."
                    .into(),
            );
        }
        std::fs::write(
            processing_heartbeat_path(&self.marker, &self.owner_nonce),
            unix_now_seconds().to_string(),
        )
        .map_err(|error| format!("Не удалось продлить блокировку обработки: {error}"))?;
        if !processing_owner_matches(&self.marker, &self.owner_nonce) {
            return Err(
                "Блокировка обработки изменилась во время продления; устаревший результат не опубликован."
                    .into(),
            );
        }
        Ok(())
    }
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.heartbeat_thread.take() {
            let _ = thread.join();
        }
        if processing_owner_matches(&self.marker, &self.owner_nonce) {
            let _ = std::fs::write(
                processing_release_path(&self.marker, &self.owner_nonce),
                format!(
                    "nonce={}\\nreleased_unix={}\\n",
                    self.owner_nonce,
                    unix_now_seconds()
                ),
            );
        }
    }
}

#[cfg(test)]
mod processing_guard_fencing_tests {
    use super::*;

    fn test_source(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-processing-guard-{label}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create processing guard test root");
        (root.clone(), root.join("source.docx"))
    }

    fn stop_heartbeat(guard: &mut ProcessingGuard) {
        guard.heartbeat_stop.store(true, Ordering::SeqCst);
        if let Some(thread) = guard.heartbeat_thread.take() {
            thread.join().expect("join processing guard heartbeat");
        }
    }

    fn replace_owner(marker: &Path, nonce: &str) {
        let _ = std::fs::remove_dir_all(marker);
        std::fs::create_dir_all(marker).expect("recreate processing marker");
        std::fs::write(
            marker.join("owner"),
            format!(
                "schema=3\\nhost=replacement-host\\npid=424242\\ncreated_unix={}\\nnonce={nonce}\\n",
                unix_now_seconds()
            ),
        )
        .expect("write replacement owner");
        std::fs::write(
            processing_heartbeat_path(marker, nonce),
            unix_now_seconds().to_string(),
        )
        .expect("write replacement heartbeat");
    }

    #[test]
    fn processing_guard_drop_does_not_delete_replacement_owner() {
        let (root, source) = test_source("drop-fencing");
        let mut guard = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        stop_heartbeat(&mut guard);
        let marker = guard.marker.clone();
        replace_owner(&marker, "replacement-nonce");
        drop(guard);
        assert!(marker.is_dir(), "old guard deleted successor marker");
        assert!(processing_owner_matches(&marker, "replacement-nonce"));
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }

    #[test]
    fn processing_guard_detects_lost_ownership_before_publish() {
        let (root, source) = test_source("lost-owner");
        let mut guard = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        stop_heartbeat(&mut guard);
        let marker = guard.marker.clone();
        replace_owner(&marker, "replacement-nonce");
        assert!(guard.ensure_current().is_err());
        drop(guard);
        assert!(processing_owner_matches(&marker, "replacement-nonce"));
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }

    #[test]
    fn processing_guard_released_claim_can_be_reacquired_immediately() {
        let (root, source) = test_source("released-reacquire");
        let first = ProcessingGuard::acquire(&source, "same-job")
            .expect("acquire first guard")
            .expect("first guard must be acquired");
        let first_marker = first.marker.clone();
        let first_nonce = first.owner_nonce.clone();
        drop(first);
        assert!(processing_release_matches(&first_marker, &first_nonce));
        let second = ProcessingGuard::acquire(&source, "same-job")
            .expect("reacquire released guard")
            .expect("released guard must be immediately reclaimable");
        assert_ne!(second.owner_nonce, first_nonce);
        drop(second);
        std::fs::remove_dir_all(root).expect("cleanup processing guard test root");
    }
}

/// Current UTC year from the system clock, std-only (civil-from-days algorithm).
'''
if main.count(old) != 1:
    raise SystemExit('ProcessingGuard drop marker mismatch')
main = main.replace(old, new, 1)
main_path.write_text(main, encoding='utf-8')

# 6) Fence local file-system claims at both publication boundaries.
runtime_path = Path('src-tauri/src/subsystems/automation_runtime.rs')
runtime = runtime_path.read_text(encoding='utf-8')

old = '''fn ensure_generation_inputs_current(
    source: &Path,
    source_sha256: &str,
    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,
) -> Result<(), String> {
    ensure_source_snapshot_current(source, source_sha256)?;
    template_snapshot::ensure_all_current(template_snapshots)
}
'''
new = '''fn ensure_generation_inputs_current(
    source: &Path,
    source_sha256: &str,
    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,
    processing_guard: Option<&ProcessingGuard>,
) -> Result<(), String> {
    ensure_source_snapshot_current(source, source_sha256)?;
    template_snapshot::ensure_all_current(template_snapshots)?;
    if let Some(guard) = processing_guard {
        guard.ensure_current()?;
    }
    Ok(())
}
'''
if runtime.count(old) != 1:
    raise SystemExit('generation input fencing helper marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''    let _processing_guard = if central_queue_lease.is_some() {
'''
new = '''    let processing_guard = if central_queue_lease.is_some() {
'''
if runtime.count(old) != 1:
    raise SystemExit('processing guard binding marker mismatch')
runtime = runtime.replace(old, new, 1)

old_call = '''ensure_generation_inputs_current(&source, &source_sha256, &template_snapshots)'''
new_call = '''ensure_generation_inputs_current(
                &source,
                &source_sha256,
                &template_snapshots,
                processing_guard.as_ref(),
            )'''
if runtime.count(old_call) != 2:
    raise SystemExit(f'expected two generation publication checks, found {runtime.count(old_call)}')
runtime = runtime.replace(old_call, new_call)
runtime_path.write_text(runtime, encoding='utf-8')
