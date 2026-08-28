fn processing_lock_host_id() -> String {
    let raw = stable_machine_guid()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| format!("{}-unknown", std::env::consts::OS));
    let mut hasher = Sha256::new();
    hasher.update(std::env::consts::OS.as_bytes());
    hasher.update(b"\0");
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())[..24].to_string()
}

fn unix_now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

fn processing_owner_nonce(owner_text: &str) -> Option<&str> {
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

impl ProcessingGuard {
    fn acquire(source: &Path, source_sha256: &str) -> Result<Option<Self>, String> {
        const REMOTE_LEASE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
        const LEGACY_LEASE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
        // The claim is content-addressed and lives in the shared folder, so two
        // computers, aliases or renamed copies of the same source contend for the
        // same lease. Directory creation is atomic on normal SMB/NFS servers.
        let claims_dir = shared_queue_root(source).join("claims");
        std::fs::create_dir_all(&claims_dir)
            .map_err(|error| format!("Не удалось создать общую очередь обработки: {error}"))?;
        let marker = claims_dir.join(format!("{source_sha256}.lock"));
        let owner_path = marker.join("owner");
        let current_host = processing_lock_host_id();
        for _ in 0..2 {
            match std::fs::create_dir(&marker) {
                Ok(()) => {
                    let nonce = Uuid::new_v4().to_string();
                    let owner = format!(
                        "schema=3\nhost={current_host}\npid={}\ncreated_unix={}\nnonce={nonce}\n",
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
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let text = std::fs::read_to_string(&owner_path).unwrap_or_default();
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
                    let quarantine =
                        claims_dir.join(format!(".{source_sha256}.reclaim-{}", Uuid::new_v4()));
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
                }
                Err(error) => {
                    return Err(format!(
                        "Не удалось установить блокировку источника: {error}"
                    ));
                }
            }
        }
        Err("Не удалось восстановить блокировку источника после сбоя.".into())
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
                    "nonce={}\nreleased_unix={}\n",
                    self.owner_nonce,
                    unix_now_seconds()
                ),
            );
        }
    }
}

