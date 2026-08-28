fn shared_queue_root(source: &Path) -> PathBuf {
    source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".dokkomplekt-queue")
}

fn shared_completion_receipt(source: &Path, source_sha256: &str) -> PathBuf {
    shared_queue_root(source)
        .join("completed")
        .join(format!("{source_sha256}.done"))
}

fn shared_completion_receipt_matches(
    source: &Path,
    processing_job_sha256: &str,
) -> Result<bool, String> {
    let path = shared_completion_receipt(source, processing_job_sha256);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Не удалось безопасно проверить общую квитанцию завершения {}: {error}",
                path.display()
            ))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Общая квитанция завершения имеет недопустимый тип: {}",
            path.display()
        ));
    }
    let body = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "Не удалось прочитать общую квитанцию завершения {}: {error}",
            path.display()
        )
    })?;
    let schema_matches = body.lines().any(|line| line.trim() == "schema=1");
    let job_matches = body
        .lines()
        .any(|line| line.trim() == format!("sha256={processing_job_sha256}"));
    if schema_matches && job_matches {
        Ok(true)
    } else {
        Err(format!(
            "Общая квитанция завершения повреждена или не соответствует processing job: {}",
            path.display()
        ))
    }
}

fn mark_shared_completion(source: &Path, source_sha256: &str) -> Result<PathBuf, String> {
    let completed_dir = shared_queue_root(source).join("completed");
    std::fs::create_dir_all(&completed_dir)
        .map_err(|error| format!("Не удалось создать общую очередь завершённых дел: {error}"))?;
    let final_path = shared_completion_receipt(source, source_sha256);
    let temporary = completed_dir.join(format!(".{source_sha256}.{}.tmp", Uuid::new_v4()));
    std::fs::write(
        &temporary,
        format!(
            "schema=1\nsha256={source_sha256}\ncompleted_unix={}\nhost={}\n",
            unix_now_seconds(),
            processing_lock_host_id(),
        ),
    )
    .map_err(|error| format!("Не удалось записать квитанцию общей очереди: {error}"))?;
    match std::fs::rename(&temporary, &final_path) {
        Ok(()) => Ok(final_path),
        Err(_error) if final_path.is_file() => {
            let _ = std::fs::remove_file(&temporary);
            Ok(final_path)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!(
                "Не удалось опубликовать квитанцию общей очереди: {error}"
            ))
        }
    }
}

