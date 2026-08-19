/// Compute a stable local identity for a user/source/runtime file without
/// logging or persisting its contents. Shared by document publication, watcher
/// handoff verification and zero-touch intake.
fn file_content_signature(path: &Path) -> Result<(u64, u128, String), String> {
    use std::io::Read as _;
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let modified_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok((
        metadata.len(),
        modified_unix_ms,
        hex::encode(hasher.finalize()),
    ))
}
