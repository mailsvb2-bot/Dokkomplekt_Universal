// Crash-safe publication lock shared by file and directory publishers.

const PUBLICATION_LOCK_ORPHAN_GRACE: Duration = Duration::from_secs(24 * 60 * 60);

struct PublicationLock {
    path: PathBuf,
    token: String,
    _file: std::fs::File,
}

impl Drop for PublicationLock {
    fn drop(&mut self) {
        if publication_lock_field(&self.path, "token").as_deref() == Some(self.token.as_str()) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn publication_lock_is_old_enough(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= PUBLICATION_LOCK_ORPHAN_GRACE)
}

fn publication_lock_field(path: &Path, key: &str) -> Option<String> {
    let body = std::fs::read_to_string(path).ok()?;
    let prefix = format!("{key}=");
    body.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::trim))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn publication_lock_pid(path: &Path) -> Option<u32> {
    publication_lock_field(path, "pid")?.parse::<u32>().ok()
}

fn try_acquire_publication_lock(path: &Path) -> Result<Option<PublicationLock>, String> {
    use std::io::Write as _;
    let current_host = processing_lock_host_id();
    for _ in 0..3 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                let token = Uuid::new_v4().to_string();
                let payload = format!(
                    "schema=1\nhost={current_host}\npid={}\ntoken={token}\ncreated_unix={}\n",
                    std::process::id(),
                    unix_now_seconds()
                );
                if let Err(error) = file
                    .write_all(payload.as_bytes())
                    .and_then(|_| file.sync_all())
                {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                    return Err(format!(
                        "Не удалось записать блокировку публикации: {error}"
                    ));
                }
                return Ok(Some(PublicationLock {
                    path: path.to_path_buf(),
                    token,
                    _file: file,
                }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(path).map_err(|error| {
                    format!("Не удалось проверить существующую блокировку публикации: {error}")
                })?;
                if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                    return Err("Блокировка публикации имеет небезопасный тип файла.".into());
                }
                let lock_host = publication_lock_field(path, "host");
                let reclaim = match (lock_host.as_deref(), publication_lock_pid(path)) {
                    (Some(host), Some(pid)) if host == current_host => !process_is_alive(pid),
                    _ => publication_lock_is_old_enough(path),
                };
                if !reclaim {
                    return Ok(None);
                }
                match std::fs::remove_file(path) {
                    Ok(()) => continue,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        return Err(format!(
                            "Не удалось удалить устаревшую блокировку публикации: {error}"
                        ))
                    }
                }
            }
            Err(error) => return Err(format!("Не удалось создать блокировку публикации: {error}")),
        }
    }
    Err("Не удалось получить блокировку публикации после очистки устаревшего состояния.".into())
}
