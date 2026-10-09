#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateArtifactManifest {
    platform: String,
    url: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateManifestPayload {
    schema: String,
    version: String,
    published_at: String,
    #[serde(default)]
    notes: Option<String>,
    platforms: Vec<UpdateArtifactManifest>,
}

#[derive(Debug, Clone, Deserialize)]
struct SignedUpdateManifest {
    payload: UpdateManifestPayload,
    signature_alg: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateCheckResponse {
    available: bool,
    current_version: String,
    latest_version: String,
    platform: String,
    message: String,
    notes: Option<String>,
    verified_package_path: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
}

fn parse_semver(raw: &str) -> Result<Version, String> {
    let trimmed = raw.trim();
    let value = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(value).map_err(|error| format!("Некорректная SemVer-версия «{raw}»: {error}"))
}

fn current_update_platform() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return "windows-x86_64";
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return "windows-aarch64";
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return "linux-x86_64";
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return "linux-aarch64";
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return "macos-x86_64";
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return "macos-aarch64";
    }
    #[allow(unreachable_code)]
    "unsupported"
}

pub(crate) fn is_forbidden_public_download_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if let Ok(ip) = normalized.parse::<IpAddr>() {
        return is_forbidden_public_download_ip(ip);
    }
    const RESERVED_EXACT: &[&str] = &[
        "localhost",
        "localhost.localdomain",
        "example.com",
        "example.net",
        "example.org",
    ];
    const RESERVED_SUFFIXES: &[&str] = &[
        ".localhost",
        ".invalid",
        ".test",
        ".example",
        ".local",
        ".example.com",
        ".example.net",
        ".example.org",
    ];
    RESERVED_EXACT.contains(&normalized.as_str())
        || RESERVED_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
        || !normalized.contains('.')
        || normalized.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

pub(crate) fn is_forbidden_public_download_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, third, _] = ip.octets();
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || first == 0
                || (first == 100 && (second & 0b1100_0000) == 64)
                || (first == 192 && second == 0 && third == 0)
                || (first == 198 && (second & 0b1111_1110) == 18)
                || first >= 240
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || (octets[0] == 0x20
                    && octets[1] == 0x01
                    && octets[2] == 0x0d
                    && octets[3] == 0xb8)
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| is_forbidden_public_download_ip(IpAddr::V4(mapped)))
        }
    }
}

#[derive(Debug, Clone)]
struct ValidatedUpdateUrl {
    url: reqwest::Url,
    host: String,
    addresses: Vec<SocketAddr>,
}

fn validate_update_url(raw: &str) -> Result<ValidatedUpdateUrl, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Некорректный URL обновления".to_string())?;
    if url.scheme() != "https" {
        return Err("Обновления разрешены только по HTTPS".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL обновления не должен содержать credentials".to_string());
    }
    if url.fragment().is_some() {
        return Err("URL обновления не должен содержать fragment".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "В URL обновления отсутствует host".to_string())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if is_forbidden_public_download_host(&host) {
        return Err("Placeholder, local или некорректный host запрещён для обновлений".to_string());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "Не определён HTTPS-порт".to_string())?;
    let mut addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "Не удалось безопасно разрешить адрес сервера обновлений".to_string())?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| is_forbidden_public_download_ip(address.ip()))
    {
        return Err("Private, loopback и служебные IP запрещены для обновлений".to_string());
    }
    Ok(ValidatedUpdateUrl {
        url,
        host,
        addresses,
    })
}

fn pinned_update_client(
    validated: &ValidatedUpdateUrl,
) -> Result<reqwest::blocking::Client, String> {
    // Pin the exact public addresses that passed the SSRF filter.  Without this,
    // reqwest would resolve the hostname again during connect and a DNS-rebinding
    // response could switch from a public address to loopback/private infrastructure.
    crate::ensure_rustls_crypto_provider();
    reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(45))
        .resolve_to_addrs(&validated.host, &validated.addresses)
        .build()
        .map_err(|error| error.to_string())
}

fn verify_update_manifest(manifest: &SignedUpdateManifest) -> Result<(), String> {
    if !manifest
        .signature_alg
        .trim()
        .eq_ignore_ascii_case("ed25519")
    {
        return Err("Неподдерживаемый алгоритм подписи update manifest".to_string());
    }
    if manifest.payload.schema != "dokkomplekt.update.v1" {
        return Err("Неподдерживаемая схема update manifest".to_string());
    }
    let key_bytes = BASE64_STANDARD
        .decode(TRUSTED_UPDATE_PUBKEY_B64.trim())
        .map_err(|_| "Некорректный встроенный update public key".to_string())?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Update public key должен содержать 32 байта".to_string())?;
    let key = VerifyingKey::from_bytes(&key_array)
        .map_err(|_| "Некорректный встроенный update public key".to_string())?;
    let signature_bytes = BASE64_STANDARD
        .decode(manifest.signature.trim())
        .map_err(|_| "Некорректная подпись update manifest".to_string())?;
    let signature = Ed25519Signature::from_slice(&signature_bytes)
        .map_err(|_| "Некорректная длина подписи update manifest".to_string())?;
    let payload_value =
        serde_json::to_value(&manifest.payload).map_err(|error| error.to_string())?;
    let canonical = canonical_json_bytes(&payload_value)?;
    key.verify(&canonical, &signature)
        .map_err(|_| "Подпись update manifest не прошла проверку".to_string())
}

fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    fn write_value(value: &serde_json::Value, output: &mut Vec<u8>) -> Result<(), String> {
        match value {
            serde_json::Value::Null => output.extend_from_slice(b"null"),
            serde_json::Value::Bool(value) => {
                output.extend_from_slice(if *value { b"true" } else { b"false" });
            }
            serde_json::Value::Number(value) => {
                output.extend_from_slice(value.to_string().as_bytes())
            }
            serde_json::Value::String(value) => {
                serde_json::to_writer(&mut *output, value).map_err(|error| error.to_string())?;
            }
            serde_json::Value::Array(values) => {
                output.push(b'[');
                for (index, item) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    write_value(item, output)?;
                }
                output.push(b']');
            }
            serde_json::Value::Object(values) => {
                output.push(b'{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    serde_json::to_writer(&mut *output, key).map_err(|error| error.to_string())?;
                    output.push(b':');
                    write_value(&values[key], output)?;
                }
                output.push(b'}');
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn safe_update_file_name(url: &reqwest::Url) -> Result<String, String> {
    let name = url
        .path_segments()
        .and_then(Iterator::last)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "В URL обновления отсутствует имя файла".to_string())?;
    if name.len() > 128
        || name.starts_with('.')
        || name.contains("..")
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return Err("Небезопасное имя файла обновления".to_string());
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|number| (1..=9).contains(&number));
    if reserved {
        return Err("Зарезервированное имя файла обновления".to_string());
    }
    Ok(name.to_string())
}

fn fetch_limited_bytes(
    client: &reqwest::blocking::Client,
    url: &ValidatedUpdateUrl,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url.url.clone())
        .send()
        .map_err(|error| format!("Ошибка загрузки: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Сервер обновлений вернул HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes)
    {
        return Err("Ответ сервера обновлений превышает допустимый размер".to_string());
    }
    let mut bytes = Vec::new();
    response
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось прочитать ответ сервера обновлений: {error}"))?;
    if bytes.len() as u64 > max_bytes {
        return Err("Ответ сервера обновлений превышает допустимый размер".to_string());
    }
    Ok(bytes)
}

fn download_and_verify_update(
    app: &tauri::AppHandle,
    artifact: &UpdateArtifactManifest,
    version: &str,
) -> Result<PathBuf, String> {
    if artifact.size_bytes == 0 || artifact.size_bytes > MAX_UPDATE_ARTIFACT_BYTES {
        return Err("Некорректный размер пакета обновления".to_string());
    }
    let expected_hash = artifact.sha256.trim().to_ascii_lowercase();
    if expected_hash.len() != 64
        || !expected_hash
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("Некорректный SHA-256 пакета обновления".to_string());
    }
    let url = validate_update_url(&artifact.url)?;
    let file_name = safe_update_file_name(&url.url)?;
    let client = pinned_update_client(&url)?;
    let mut response = client
        .get(url.url.clone())
        .send()
        .map_err(|error| format!("Ошибка загрузки пакета: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Сервер пакета обновления вернул HTTP {}",
            response.status()
        ));
    }
    if let Some(content_length) = response.content_length() {
        if content_length != artifact.size_bytes {
            return Err("Content-Length пакета не совпадает с подписанным manifest".to_string());
        }
    }
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let target_dir = base.join("verified-updates").join(version);
    std::fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;
    let final_path = target_dir.join(&file_name);
    let temp_path = target_dir.join(format!(".{file_name}.{}.download-part", Uuid::new_v4()));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| error.to_string())?;
    let transfer = (|| -> Result<(u64, String), String> {
        let mut digest = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > artifact.size_bytes || total > MAX_UPDATE_ARTIFACT_BYTES {
                return Err("Пакет обновления превышает подписанный размер".to_string());
            }
            digest.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok((total, hex::encode(digest.finalize())))
    })();
    drop(output);
    let (total, actual_hash) = match transfer {
        Ok(result) => result,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error);
        }
    };
    if total != artifact.size_bytes {
        let _ = std::fs::remove_file(&temp_path);
        return Err("Фактический размер пакета не совпадает с manifest".to_string());
    }
    if actual_hash != expected_hash {
        let _ = std::fs::remove_file(&temp_path);
        return Err("SHA-256 пакета обновления не совпадает с manifest".to_string());
    }
    if final_path.exists() {
        let metadata = std::fs::symlink_metadata(&final_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_dir() {
            let _ = std::fs::remove_file(&temp_path);
            return Err("Путь проверенного обновления занят каталогом".to_string());
        }
        if let Err(error) = std::fs::remove_file(&final_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error.to_string());
        }
    }
    if let Err(error) = std::fs::rename(&temp_path, &final_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error.to_string());
    }
    Ok(final_path)
}

// The verified installer is a shared file: serialize check/download and apply
// across renderers and independently launched application processes.
static UPDATE_OPERATION_LOCK: Mutex<()> = Mutex::new(());
const UPDATE_OPERATION_LOCK_FILE: &str = ".dokkomplekt-update.lock";

struct UpdateOperationGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    // Dropping the file handle releases the platform's cross-process lock.
    _file: std::fs::File,
}

fn lock_update_operation_process() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    UPDATE_OPERATION_LOCK
        .try_lock()
        .map_err(|error| match error {
            std::sync::TryLockError::WouldBlock => {
                "Проверка или установка обновления уже выполняется.".to_string()
            }
            std::sync::TryLockError::Poisoned(_) => {
                "Механизм обновления заблокирован после ошибки. Перезапустите программу.".to_string()
            }
        })
}

fn lock_update_operation_file(path: &Path) -> Result<std::fs::File, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if !metadata.is_file()
                || crate::publication_metadata_is_link_or_reparse(&metadata) =>
        {
            return Err("Файл блокировки обновления имеет небезопасный тип".to_string());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Не удалось проверить блокировку обновления: {error}")),
        _ => {}
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .map_err(|error| format!("Не удалось открыть блокировку обновления: {error}"))?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
        "Проверка или установка обновления уже выполняется в другом экземпляре программы."
            .to_string()
    })?;
    Ok(file)
}

fn lock_update_operation(app: &tauri::AppHandle) -> Result<UpdateOperationGuard, String> {
    let process = lock_update_operation_process()?;
    let root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("Не удалось подготовить каталог обновления: {error}"))?;
    let file = lock_update_operation_file(&root.join(UPDATE_OPERATION_LOCK_FILE))?;
    Ok(UpdateOperationGuard {
        _process: process,
        _file: file,
    })
}

#[tauri::command]
fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateCheckResponse, String> {
    let _update_guard = lock_update_operation(&app)?;
    let manifest_url = validate_update_url(TRUSTED_UPDATE_MANIFEST_URL)?;
    let client = pinned_update_client(&manifest_url)?;
    let manifest_bytes = fetch_limited_bytes(&client, &manifest_url, MAX_UPDATE_MANIFEST_BYTES)?;
    let manifest: SignedUpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Некорректный update manifest: {error}"))?;
    verify_update_manifest(&manifest)?;
    let current_raw = env!("CARGO_PKG_VERSION");
    let current = parse_semver(current_raw)?;
    let latest = parse_semver(&manifest.payload.version)?;
    let platform = current_update_platform().to_string();
    if latest <= current {
        return Ok(UpdateCheckResponse {
            available: false,
            current_version: current_raw.to_string(),
            latest_version: manifest.payload.version,
            platform,
            message: "Установлена актуальная версия".to_string(),
            notes: manifest.payload.notes,
            verified_package_path: None,
            sha256: None,
            size_bytes: None,
        });
    }
    let artifact = manifest
        .payload
        .platforms
        .iter()
        .find(|artifact| artifact.platform == platform)
        .ok_or_else(|| format!("В manifest нет пакета для платформы {platform}"))?;
    let verified_path = download_and_verify_update(&app, artifact, &manifest.payload.version)?;
    Ok(UpdateCheckResponse {
        available: true,
        current_version: current_raw.to_string(),
        latest_version: manifest.payload.version,
        platform,
        message: "Обновление скачано, подпись manifest, размер и SHA-256 проверены".to_string(),
        notes: manifest.payload.notes,
        verified_package_path: Some(verified_path.to_string_lossy().to_string()),
        sha256: Some(artifact.sha256.to_ascii_lowercase()),
        size_bytes: Some(artifact.size_bytes),
    })
}

include!("template_picker.rs");


#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateRecoveryState {
    schema: String,
    status: String,
    from_version: String,
    target_version: String,
    package_path: String,
    package_sha256: String,
    package_size_bytes: u64,
    backup_dir: String,
    created_at: String,
    #[serde(default)]
    verified_at: Option<String>,
    #[serde(default)]
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateApplyResponse {
    prepared: bool,
    target_version: String,
    recovery_state_path: String,
    backup_dir: String,
    message: String,
}

const UPDATE_RECOVERY_SCHEMA: &str = "dokkomplekt.update-recovery.v1";
const UPDATE_BACKUP_OWNERSHIP_SCHEMA: &str = "dokkomplekt.update-backup.v1";
const UPDATE_BACKUP_OWNERSHIP_FILE: &str = ".dokkomplekt-update-backup.json";
pub(crate) const UPDATE_BACKUP_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const UPDATE_BACKUP_MAX_ENTRIES: u64 = 8;
pub(crate) const UPDATE_BACKUP_RETENTION_SECONDS: u64 = 180 * 24 * 60 * 60;
static UPDATE_BACKUP_POLICY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateBackupOwnership {
    schema: String,
    target_version: String,
    created_at_unix: i64,
}

#[derive(Debug, Clone, Copy)]
struct UpdateBackupPolicy {
    max_bytes: u64,
    max_entries: u64,
    retention_seconds: u64,
}

impl UpdateBackupPolicy {
    fn production() -> Self {
        Self {
            max_bytes: UPDATE_BACKUP_QUOTA_BYTES,
            max_entries: UPDATE_BACKUP_MAX_ENTRIES,
            retention_seconds: UPDATE_BACKUP_RETENTION_SECONDS,
        }
    }
}

#[derive(Debug)]
struct OwnedUpdateBackup {
    path: PathBuf,
    created_at_unix: i64,
    bytes: u64,
}

fn lock_update_backup_policy() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    UPDATE_BACKUP_POLICY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Не удалось заблокировать политику backup обновлений".to_string())
}

fn validate_update_backup_root(root: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|error| format!("Не удалось проверить каталог update-backups: {error}"))?;
    if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Каталог update-backups имеет небезопасный тип: {}",
            root.display()
        ));
    }
    Ok(())
}

fn ensure_update_backup_root(data_dir: &Path) -> Result<PathBuf, String> {
    let root = data_dir.join("update-backups");
    match std::fs::symlink_metadata(&root) {
        Ok(_) => validate_update_backup_root(&root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&root)
                .map_err(|error| format!("Не удалось создать update-backups: {error}"))?;
            validate_update_backup_root(&root)?;
        }
        Err(error) => {
            return Err(format!(
                "Не удалось проверить каталог update-backups: {error}"
            ))
        }
    }
    Ok(root)
}

fn update_backup_path_size(path: &Path) -> Result<u64, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("Не удалось проверить backup {}: {error}", path.display()))?;
    if crate::publication_metadata_is_link_or_reparse(&metadata) {
        return Err(format!(
            "Backup содержит ссылку/reparse point и не может считаться управляемым: {}",
            path.display()
        ));
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path)
        .map_err(|error| format!("Не удалось прочитать backup {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        total = total.saturating_add(update_backup_path_size(&entry.path())?);
    }
    Ok(total)
}

fn write_update_backup_ownership_marker(
    backup_dir: &Path,
    target_version: &str,
    created_at_unix: i64,
) -> Result<(), String> {
    let marker = UpdateBackupOwnership {
        schema: UPDATE_BACKUP_OWNERSHIP_SCHEMA.to_string(),
        target_version: target_version.to_string(),
        created_at_unix,
    };
    let path = backup_dir.join(UPDATE_BACKUP_OWNERSHIP_FILE);
    let bytes = serde_json::to_vec_pretty(&marker).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("Не удалось создать ownership marker backup: {error}"))?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

fn collect_owned_update_backups(root: &Path) -> Result<Vec<OwnedUpdateBackup>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Не удалось прочитать update-backups: {error}")),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Не удалось проверить {}: {error}", path.display()))?;
        if crate::publication_metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
            continue;
        }
        let marker_path = path.join(UPDATE_BACKUP_OWNERSHIP_FILE);
        let marker_metadata = match std::fs::symlink_metadata(&marker_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось проверить ownership marker {}: {error}",
                    marker_path.display()
                ))
            }
        };
        if crate::publication_metadata_is_link_or_reparse(&marker_metadata)
            || !marker_metadata.is_file()
        {
            return Err(format!(
                "Ownership marker backup имеет небезопасный тип: {}",
                marker_path.display()
            ));
        }
        let marker: UpdateBackupOwnership = serde_json::from_slice(
            &std::fs::read(&marker_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| {
            format!(
                "Повреждён ownership marker backup {}: {error}",
                marker_path.display()
            )
        })?;
        if marker.schema != UPDATE_BACKUP_OWNERSHIP_SCHEMA {
            return Err(format!(
                "Неизвестная схема ownership marker backup: {}",
                marker_path.display()
            ));
        }
        backups.push(OwnedUpdateBackup {
            bytes: update_backup_path_size(&path)?,
            path,
            created_at_unix: marker.created_at_unix,
        });
    }
    backups.sort_by(|left, right| {
        left.created_at_unix
            .cmp(&right.created_at_unix)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(backups)
}

fn validate_owned_update_backup_for_removal(backup: &OwnedUpdateBackup) -> Result<(), String> {
    let directory_metadata = std::fs::symlink_metadata(&backup.path).map_err(|error| {
        format!(
            "Управляемый backup исчез перед очисткой {}: {error}",
            backup.path.display()
        )
    })?;
    if crate::publication_metadata_is_link_or_reparse(&directory_metadata)
        || !directory_metadata.is_dir()
    {
        return Err(format!(
            "Очистка backup остановлена: каталог изменил безопасный тип {}",
            backup.path.display()
        ));
    }

    let marker_path = backup.path.join(UPDATE_BACKUP_OWNERSHIP_FILE);
    let marker_metadata = std::fs::symlink_metadata(&marker_path).map_err(|error| {
        format!(
            "Ownership marker исчез перед очисткой {}: {error}",
            marker_path.display()
        )
    })?;
    if crate::publication_metadata_is_link_or_reparse(&marker_metadata)
        || !marker_metadata.is_file()
    {
        return Err(format!(
            "Очистка backup остановлена: ownership marker изменил безопасный тип {}",
            marker_path.display()
        ));
    }
    let marker: UpdateBackupOwnership = serde_json::from_slice(
        &std::fs::read(&marker_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        format!(
            "Ownership marker изменился перед очисткой {}: {error}",
            marker_path.display()
        )
    })?;
    if marker.schema != UPDATE_BACKUP_OWNERSHIP_SCHEMA
        || marker.created_at_unix != backup.created_at_unix
    {
        return Err(format!(
            "Очистка backup остановлена: ownership marker больше не соответствует кандидату {}",
            backup.path.display()
        ));
    }
    Ok(())
}

fn enforce_update_backup_policy_at(
    root: &Path,
    protected_backup: Option<&Path>,
    now_unix: i64,
    policy: UpdateBackupPolicy,
) -> Result<usize, String> {
    if policy.max_bytes == 0 || policy.max_entries == 0 || policy.retention_seconds == 0 {
        return Err("Некорректная политика хранения backup обновлений".to_string());
    }
    validate_update_backup_root(root)?;
    let backups = collect_owned_update_backups(root)?;
    if backups.is_empty() {
        return Ok(0);
    }
    let protected = match protected_backup {
        Some(path) => Some(
            path.canonicalize()
                .map_err(|error| format!("Защищённый backup недоступен: {error}"))?,
        ),
        None => None,
    };
    let fallback_protected_index = protected.is_none().then_some(backups.len() - 1);
    let cutoff = now_unix.saturating_sub(
        i64::try_from(policy.retention_seconds)
            .map_err(|_| "Retention backup превышает допустимый диапазон".to_string())?,
    );
    let mut victim = vec![false; backups.len()];
    let mut remaining_entries = backups.len() as u64;
    let mut remaining_bytes = backups
        .iter()
        .fold(0_u64, |total, backup| total.saturating_add(backup.bytes));

    for (index, backup) in backups.iter().enumerate() {
        let explicitly_protected = match &protected {
            Some(protected) => backup
                .path
                .canonicalize()
                .map(|path| path == *protected)
                .unwrap_or(false),
            None => false,
        };
        let is_protected =
            explicitly_protected || fallback_protected_index == Some(index);
        if !is_protected && backup.created_at_unix < cutoff {
            victim[index] = true;
            remaining_entries = remaining_entries.saturating_sub(1);
            remaining_bytes = remaining_bytes.saturating_sub(backup.bytes);
        }
    }

    if remaining_entries > policy.max_entries || remaining_bytes > policy.max_bytes {
        for (index, backup) in backups.iter().enumerate() {
            if victim[index] {
                continue;
            }
            let explicitly_protected = match &protected {
                Some(protected) => backup
                    .path
                    .canonicalize()
                    .map(|path| path == *protected)
                    .unwrap_or(false),
                None => false,
            };
            if explicitly_protected || fallback_protected_index == Some(index) {
                continue;
            }
            victim[index] = true;
            remaining_entries = remaining_entries.saturating_sub(1);
            remaining_bytes = remaining_bytes.saturating_sub(backup.bytes);
            if remaining_entries <= policy.max_entries && remaining_bytes <= policy.max_bytes {
                break;
            }
        }
    }

    // Preflight every victim before deleting anything so a changed marker/root blocks the
    // destructive phase rather than producing a partially cleaned recovery set.
    for (index, backup) in backups.iter().enumerate() {
        if victim[index] {
            validate_owned_update_backup_for_removal(backup)?;
        }
    }

    let mut removed = 0_usize;
    for (index, backup) in backups.iter().enumerate() {
        if !victim[index] {
            continue;
        }
        // Re-check immediately before remove_dir_all to narrow the TOCTOU window and, in
        // particular, refuse a directory that was replaced by a symlink/junction/reparse point.
        validate_update_backup_root(root)?;
        validate_owned_update_backup_for_removal(backup)?;
        std::fs::remove_dir_all(&backup.path).map_err(|error| {
            format!(
                "Не удалось удалить управляемый backup {}: {error}",
                backup.path.display()
            )
        })?;
        removed = removed.saturating_add(1);
    }
    Ok(removed)
}

fn protected_update_backup_path(
    app: &tauri::AppHandle,
    root: &Path,
) -> Result<Option<PathBuf>, String> {
    let Some(state) = load_update_recovery_state(app)? else {
        return Ok(None);
    };
    let backup = PathBuf::from(state.backup_dir);
    let canonical = backup
        .canonicalize()
        .map_err(|error| format!("Recovery backup недоступен: {error}"))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Каталог update-backups недоступен: {error}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("Recovery marker указывает на backup вне доверенного update-backups".to_string());
    }
    Ok(Some(canonical))
}

fn enforce_update_backup_storage_policy_unlocked(
    app: &tauri::AppHandle,
) -> Result<usize, String> {
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let root = data_dir.join("update-backups");
    match std::fs::symlink_metadata(&root) {
        Ok(_) => validate_update_backup_root(&root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!(
                "Не удалось проверить каталог update-backups: {error}"
            ))
        }
    }
    let protected = protected_update_backup_path(app, &root)?;
    enforce_update_backup_policy_at(
        &root,
        protected.as_deref(),
        OffsetDateTime::now_utc().unix_timestamp(),
        UpdateBackupPolicy::production(),
    )
}

pub(crate) fn enforce_update_backup_storage_policy(
    app: &tauri::AppHandle,
) -> Result<usize, String> {
    let _guard = lock_update_backup_policy()?;
    enforce_update_backup_storage_policy_unlocked(app)
}

pub(crate) fn owned_update_backup_bytes(app: &tauri::AppHandle) -> Result<u64, String> {
    let _guard = lock_update_backup_policy()?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let backups = collect_owned_update_backups(&data_dir.join("update-backups"))?;
    Ok(backups
        .iter()
        .fold(0_u64, |total, backup| total.saturating_add(backup.bytes)))
}

fn update_recovery_state_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("update-recovery.json"))
}

fn sha256_file(path: &Path) -> Result<(u64, String), String> {
    let mut input = std::fs::File::open(path)
        .map_err(|error| format!("Не удалось открыть пакет обновления: {error}"))?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        digest.update(&buffer[..read]);
    }
    Ok((total, hex::encode(digest.finalize())))
}

fn verify_downloaded_package(
    package_path: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(package_path)
        .map_err(|error| format!("Проверенный пакет обновления недоступен: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Путь обновления должен указывать на обычный файл".to_string());
    }
    let (actual_size, actual_sha256) = sha256_file(package_path)?;
    if actual_size != expected_size_bytes {
        return Err("Размер пакета изменился после первичной проверки".to_string());
    }
    if actual_sha256 != expected_sha256.trim().to_ascii_lowercase() {
        return Err("SHA-256 пакета изменился после первичной проверки".to_string());
    }
    Ok(())
}

fn copy_update_backup_file(source: &Path, target_dir: &Path) -> Result<Option<PathBuf>, String> {
    match std::fs::symlink_metadata(source) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!(
                    "Небезопасный тип файла состояния для backup: {}",
                    source.display()
                ));
            }
            std::fs::create_dir_all(target_dir).map_err(|error| error.to_string())?;
            let name = source
                .file_name()
                .ok_or_else(|| "Не удалось определить имя файла backup".to_string())?;
            let target = target_dir.join(name);
            std::fs::copy(source, &target).map_err(|error| {
                format!(
                    "Не удалось создать backup {} -> {}: {error}",
                    source.display(),
                    target.display()
                )
            })?;
            Ok(Some(target))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn backup_update_state(app: &tauri::AppHandle, target_version: &str) -> Result<PathBuf, String> {
    let db_path = default_state_db_path(app)?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let backup_root = ensure_update_backup_root(&data_dir)?;
    let backup_dir = backup_root.join(format!("{}-{}", target_version, Uuid::new_v4()));
    std::fs::create_dir(&backup_dir).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let repo = repository_for(&db_path)?;
        repo.quick_integrity_check()
            .map_err(|error| format!("Локальная база не прошла integrity check: {error}"))?;
        let backup_db = backup_dir.join(
            db_path
                .file_name()
                .ok_or_else(|| "Не удалось определить имя локальной базы".to_string())?,
        );
        repo.backup_snapshot(&backup_db)
            .map_err(|error| format!("Не удалось создать консистентный SQLite snapshot: {error}"))?;

        let key_path = db_path.with_file_name(format!(
            "{}.key",
            db_path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(DEFAULT_STATE_DB)
        ));
        if copy_update_backup_file(&key_path, &backup_dir)?.is_none() {
            return Err("Не удалось создать обязательный backup локального ключа".to_string());
        }
        write_update_backup_ownership_marker(
            &backup_dir,
            target_version,
            OffsetDateTime::now_utc().unix_timestamp(),
        )?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&backup_dir);
        return Err(error);
    }
    Ok(backup_dir)
}

fn rollback_transaction_path(
    target: &Path,
    transaction_id: Uuid,
    suffix: &str,
) -> Result<PathBuf, String> {
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Некорректное имя файла rollback".to_string())?;
    Ok(target.with_file_name(format!("{name}.rollback-{transaction_id}.{suffix}")))
}

fn restore_renamed_originals(originals: &[(PathBuf, Option<PathBuf>)]) {
    for (target, old) in originals.iter().rev() {
        if let Some(old) = old {
            let _ = std::fs::rename(old, target);
        }
    }
}

fn restore_update_backup_files(db_path: &Path, backup_dir: &Path) -> Result<(), String> {
    let key_path = db_path.with_file_name(format!(
        "{}.key",
        db_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_STATE_DB)
    ));
    let targets = vec![
        db_path.to_path_buf(),
        key_path,
        PathBuf::from(format!("{}-wal", db_path.display())),
        PathBuf::from(format!("{}-shm", db_path.display())),
    ];

    let required = [&targets[0], &targets[1]];
    for target in required {
        let source = backup_dir.join(
            target
                .file_name()
                .ok_or_else(|| "Некорректное имя файла rollback".to_string())?,
        );
        let metadata = std::fs::symlink_metadata(&source)
            .map_err(|error| format!("Обязательный backup-файл недоступен {}: {error}", source.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("Обязательный backup-файл имеет небезопасный тип: {}", source.display()));
        }
    }

    let transaction_id = Uuid::new_v4();
    let mut staged = Vec::new();
    let mut originals = Vec::new();

    for target in &targets {
        let source = backup_dir.join(
            target
                .file_name()
                .ok_or_else(|| "Некорректное имя файла rollback".to_string())?,
        );
        match std::fs::symlink_metadata(&source) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(format!("Backup-файл имеет небезопасный тип: {}", source.display()));
                }
                let temp = rollback_transaction_path(target, transaction_id, "tmp")?;
                std::fs::copy(&source, &temp).map_err(|error| {
                    format!(
                        "Не удалось подготовить rollback {} -> {}: {error}",
                        source.display(),
                        temp.display()
                    )
                })?;
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&temp)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| format!("Не удалось синхронизировать rollback-файл {}: {error}", temp.display()))?;
                staged.push((target.clone(), Some(temp)));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                staged.push((target.clone(), None));
            }
            Err(error) => return Err(format!("Не удалось проверить backup-файл {}: {error}", source.display())),
        }
    }

    for target in &targets {
        match std::fs::symlink_metadata(target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    restore_renamed_originals(&originals);
                    for (_, temp) in &staged {
                        if let Some(temp) = temp {
                            let _ = std::fs::remove_file(temp);
                        }
                    }
                    return Err(format!("Текущий файл состояния имеет небезопасный тип: {}", target.display()));
                }
                let old = rollback_transaction_path(target, transaction_id, "old")?;
                if let Err(error) = std::fs::rename(target, &old) {
                    restore_renamed_originals(&originals);
                    for (_, temp) in &staged {
                        if let Some(temp) = temp {
                            let _ = std::fs::remove_file(temp);
                        }
                    }
                    return Err(format!(
                        "Не удалось отложить текущий файл состояния {}: {error}",
                        target.display()
                    ));
                }
                originals.push((target.clone(), Some(old)));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                originals.push((target.clone(), None));
            }
            Err(error) => {
                restore_renamed_originals(&originals);
                for (_, temp) in &staged {
                    if let Some(temp) = temp {
                        let _ = std::fs::remove_file(temp);
                    }
                }
                return Err(format!("Не удалось проверить текущий файл состояния {}: {error}", target.display()));
            }
        }
    }

    let apply_result = (|| -> Result<(), String> {
        for (target, temp) in &staged {
            if let Some(temp) = temp {
                std::fs::rename(temp, target).map_err(|error| {
                    format!("Не удалось восстановить файл состояния {}: {error}", target.display())
                })?;
            }
        }
        let repo = repository_for(db_path)?;
        repo.quick_integrity_check()
            .map_err(|error| format!("Восстановленная база не прошла integrity check: {error}"))?;
        Ok(())
    })();

    if let Err(error) = apply_result {
        for target in &targets {
            let _ = std::fs::remove_file(target);
        }
        restore_renamed_originals(&originals);
        for (_, temp) in &staged {
            if let Some(temp) = temp {
                let _ = std::fs::remove_file(temp);
            }
        }
        return Err(error);
    }

    for (_, old) in originals {
        if let Some(old) = old {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

fn restore_update_backup(app: &tauri::AppHandle, state: &UpdateRecoveryState) -> Result<(), String> {
    let app_data = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let trusted_root = app_data.join("update-backups");
    let canonical_root = trusted_root
        .canonicalize()
        .map_err(|error| format!("Каталог update-backups недоступен: {error}"))?;
    let backup_dir = PathBuf::from(&state.backup_dir)
        .canonicalize()
        .map_err(|error| format!("Каталог backup недоступен: {error}"))?;
    if !backup_dir.starts_with(&canonical_root) {
        return Err("Recovery marker указывает на backup вне доверенного update-backups".to_string());
    }
    let metadata = std::fs::symlink_metadata(&backup_dir)
        .map_err(|error| format!("Не удалось проверить каталог backup: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Каталог backup имеет небезопасный тип".to_string());
    }
    restore_update_backup_files(&default_state_db_path(app)?, &backup_dir)
}

fn ensure_no_active_case_runs(app: &tauri::AppHandle) -> Result<(), String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let active = repo
        .list_case_runs(10_000)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|run| !matches!(run.status.as_str(), "completed" | "failed" | "cancelled"))
        .map(|run| run.case_id)
        .collect::<Vec<_>>();
    if !active.is_empty() {
        return Err(format!(
            "Обновление отложено: есть незавершённые операции ({})",
            active.len()
        ));
    }
    Ok(())
}

fn write_update_recovery_state(
    app: &tauri::AppHandle,
    state: &UpdateRecoveryState,
) -> Result<PathBuf, String> {
    let path = update_recovery_state_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temp, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

fn load_update_recovery_state(app: &tauri::AppHandle) -> Result<Option<UpdateRecoveryState>, String> {
    let path = update_recovery_state_path(app)?;
    match std::fs::read(&path) {
        Ok(bytes) => {
            let state: UpdateRecoveryState = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Повреждён recovery marker обновления: {error}"))?;
            if state.schema != UPDATE_RECOVERY_SCHEMA {
                return Err("Неизвестная схема recovery marker обновления".to_string());
            }
            Ok(Some(state))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn reconcile_pending_update(app: &tauri::AppHandle) -> Result<(), String> {
    let Some(mut state) = load_update_recovery_state(app)? else {
        return Ok(());
    };
    if state.status != "installer_started" && state.status != "prepared" {
        return Ok(());
    }

    let current = parse_semver(env!("CARGO_PKG_VERSION"))?;
    let target = parse_semver(&state.target_version)?;
    if current >= target {
        let repo = repository_for(&default_state_db_path(app)?)?;
        repo.quick_integrity_check()
            .map_err(|error| format!("После обновления повреждено локальное состояние: {error}"))?;
        state.status = "verified".to_string();
        state.verified_at = Some(OffsetDateTime::now_utc().to_string());
        state.last_error = None;
        write_update_recovery_state(app, &state)?;
    } else if state.status == "installer_started" {
        let live_state = repository_for(&default_state_db_path(app)?)
            .and_then(|repo| {
                repo.quick_integrity_check()
                    .map_err(|error| error.to_string())
            });
        match live_state {
            Ok(()) => {
                state.status = "recoverable_failure".to_string();
                state.last_error = Some(
                    "Installer был запущен, но приложение стартовало в прежней версии; текущая локальная база и ключ проверены и сохранены без отката, pre-update backup доступен для восстановления."
                        .to_string(),
                );
            }
            Err(live_error) => match restore_update_backup(app, &state) {
                Ok(()) => {
                    state.status = "rolled_back".to_string();
                    state.verified_at = Some(OffsetDateTime::now_utc().to_string());
                    state.last_error = Some(format!(
                        "Installer был запущен, приложение стартовало в прежней версии, а live state не прошёл проверку ({live_error}); локальное состояние автоматически восстановлено из pre-update backup."
                    ));
                }
                Err(rollback_error) => {
                    state.status = "recoverable_failure".to_string();
                    state.last_error = Some(format!(
                        "Installer был запущен, приложение стартовало в прежней версии; live state не прошёл проверку ({live_error}), автоматический rollback не выполнен: {rollback_error}. Backup сохранён."
                    ));
                }
            },
        }
        write_update_recovery_state(app, &state)?;
    } else if state.status == "prepared" {
        state.status = "recoverable_failure".to_string();
        state.last_error = Some(
            "Подготовка обновления была прервана до подтверждённого запуска installer; текущая локальная база не откатывалась, pre-update backup сохранён."
                .to_string(),
        );
        write_update_recovery_state(app, &state)?;
    }
    Ok(())
}

fn record_recoverable_update_failure(
    app: &tauri::AppHandle,
    recovery: &mut UpdateRecoveryState,
    detail: String,
) -> String {
    recovery.status = "recoverable_failure".to_string();
    recovery.last_error = Some(detail.clone());
    match write_update_recovery_state(app, recovery) {
        Ok(_) => detail,
        Err(marker_error) => format!(
            "{detail}; дополнительно не удалось записать recovery marker: {marker_error}"
        ),
    }
}

#[tauri::command]
fn get_update_recovery_status(
    app: tauri::AppHandle,
) -> Result<Option<UpdateRecoveryState>, String> {
    load_update_recovery_state(&app)
}

#[tauri::command]
fn apply_verified_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    package_path: String,
    target_version: String,
    sha256: String,
    size_bytes: u64,
) -> Result<UpdateApplyResponse, String> {
    let _update_guard = lock_update_operation(&app)?;
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "Не удалось заблокировать persistence на время подготовки обновления".to_string())?;

    if state.persistence_blocked.load(Ordering::SeqCst) {
        return Err("Persistence уже заблокирован из-за предыдущей критической ошибки".to_string());
    }

    let target = parse_semver(&target_version)?;
    let current = parse_semver(env!("CARGO_PKG_VERSION"))?;
    if target <= current {
        return Err("Нельзя применить обновление той же или более старой версии".to_string());
    }

    ensure_no_active_case_runs(&app)?;

    let package = PathBuf::from(package_path);
    verify_downloaded_package(&package, &sha256, size_bytes)?;

    let expected_root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("verified-updates")
        .join(&target_version);
    let canonical_package = package
        .canonicalize()
        .map_err(|error| format!("Не удалось канонизировать пакет обновления: {error}"))?;
    let canonical_root = expected_root
        .canonicalize()
        .map_err(|error| format!("Каталог проверенного обновления недоступен: {error}"))?;
    if !canonical_package.starts_with(&canonical_root) {
        return Err("Пакет находится вне доверенного каталога verified-updates".to_string());
    }

    let _backup_policy_guard = lock_update_backup_policy()?;
    enforce_update_backup_storage_policy_unlocked(&app)?;
    let backup_dir = backup_update_state(&app, &target_version)?;
    let mut recovery = UpdateRecoveryState {
        schema: UPDATE_RECOVERY_SCHEMA.to_string(),
        status: "prepared".to_string(),
        from_version: env!("CARGO_PKG_VERSION").to_string(),
        target_version: target_version.clone(),
        package_path: canonical_package.display().to_string(),
        package_sha256: sha256.trim().to_ascii_lowercase(),
        package_size_bytes: size_bytes,
        backup_dir: backup_dir.display().to_string(),
        created_at: OffsetDateTime::now_utc().to_string(),
        verified_at: None,
        last_error: None,
    };
    let recovery_path = write_update_recovery_state(&app, &recovery)?;
    if let Err(error) = enforce_update_backup_storage_policy_unlocked(&app) {
        return Err(record_recoverable_update_failure(
            &app,
            &mut recovery,
            format!("Backup обновления создан, но безопасная quota/retention очистка не завершена: {error}"),
        ));
    }
    drop(_backup_policy_guard);
    #[cfg(not(target_os = "windows"))]
    let _ = &recovery_path;

    #[cfg(target_os = "windows")]
    let mut watcher = match state.watcher.lock() {
        Ok(watcher) => watcher,
        Err(_) => {
            let detail = "Не удалось заблокировать watcher перед запуском обновления".to_string();
            return Err(record_recoverable_update_failure(
                &app,
                &mut recovery,
                detail,
            ));
        }
    };
    #[cfg(not(target_os = "windows"))]
    let watcher = match state.watcher.lock() {
        Ok(watcher) => watcher,
        Err(_) => {
            let detail = "Не удалось заблокировать watcher перед запуском обновления".to_string();
            return Err(record_recoverable_update_failure(
                &app,
                &mut recovery,
                detail,
            ));
        }
    };

    if let Err(error) =
        verify_downloaded_package(&canonical_package, &recovery.package_sha256, size_bytes)
    {
        return Err(record_recoverable_update_failure(
            &app,
            &mut recovery,
            error,
        ));
    }

    #[cfg(target_os = "windows")]
    {
        let child = match std::process::Command::new(&canonical_package)
            .arg("/S")
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let detail = format!("Не удалось запустить проверенный installer: {error}");
                return Err(record_recoverable_update_failure(
                    &app,
                    &mut recovery,
                    detail,
                ));
            }
        };
        if let Some(handle) = watcher.take() {
            handle.stop.store(true, Ordering::SeqCst);
        }
        recovery.status = "installer_started".to_string();
        recovery.last_error = None;
        if let Err(marker_error) = write_update_recovery_state(&app, &recovery) {
            state.persistence_blocked.store(true, Ordering::SeqCst);
            app.exit(1);
            return Err(format!(
                "Installer запущен, но не удалось зафиксировать installer_started: {marker_error}. Приложение аварийно завершает работу, чтобы не конкурировать с installer."
            ));
        }
        state.persistence_blocked.store(true, Ordering::SeqCst);
        let response = UpdateApplyResponse {
            prepared: true,
            target_version,
            recovery_state_path: recovery_path.display().to_string(),
            backup_dir: backup_dir.display().to_string(),
            message: format!(
                "Проверенный installer запущен (pid {}). Приложение завершает работу; backup и recovery marker сохранены.",
                child.id()
            ),
        };
        app.exit(0);
        Ok(response)
    }

    #[cfg(not(target_os = "windows"))]
    {
        drop(watcher);
        recovery.status = "prepared".to_string();
        recovery.last_error = Some(
            "Автоматическое применение пакета пока разрешено только для Windows NSIS; backup сохранён."
                .to_string(),
        );
        write_update_recovery_state(&app, &recovery)?;
        Err("Автоматическое применение обновления пока поддерживается только для Windows NSIS".to_string())
    }
}


#[cfg(test)]
mod fpr19_update_lifecycle_tests {
    use super::*;

    #[test]
    fn update_operation_gate_blocks_overlap_and_releases_for_retry() {
        let first = lock_update_operation_process().expect("first update must claim the gate");
        let error = match lock_update_operation_process() {
            Ok(_) => panic!("concurrent update unexpectedly bypassed the gate"),
            Err(error) => error,
        };
        assert!(error.contains("уже выполняется"), "{error}");
        drop(first);
        let _retry = lock_update_operation_process().expect("the released gate permits a retry");
    }

    #[test]
    fn update_operation_file_rejects_second_process_handle_until_release() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-update-operation-lock-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(UPDATE_OPERATION_LOCK_FILE);
        let first = lock_update_operation_file(&path).unwrap();
        assert!(
            lock_update_operation_file(&path).is_err(),
            "independent file handles must not simultaneously own the update lock"
        );
        drop(first);
        let retry = lock_update_operation_file(&path).unwrap();
        drop(retry);
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn owned_backup_fixture(
        root: &Path,
        name: &str,
        created_at_unix: i64,
        payload_bytes: usize,
    ) -> PathBuf {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("state.bin"), vec![7_u8; payload_bytes]).unwrap();
        write_update_backup_ownership_marker(&path, "99.0.0", created_at_unix).unwrap();
        path
    }

    #[test]
    fn update_backup_policy_preserves_active_recovery_and_unowned_legacy() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-update-backup-policy-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let now = 2_000_000_000_i64;
        let old = owned_backup_fixture(
            &root,
            "old-owned",
            now - (200 * 24 * 60 * 60),
            128,
        );
        let active = owned_backup_fixture(&root, "active-owned", now - 60, 128);
        let legacy = root.join("legacy-unowned");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("user-or-legacy.bin"), b"preserve").unwrap();

        let removed = enforce_update_backup_policy_at(
            &root,
            Some(&active),
            now,
            UpdateBackupPolicy {
                max_bytes: 1024 * 1024,
                max_entries: 8,
                retention_seconds: 90 * 24 * 60 * 60,
            },
        )
        .unwrap();

        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(active.exists());
        assert!(legacy.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn update_backup_policy_keeps_newest_as_sole_recovery_when_marker_is_absent() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-update-backup-fallback-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let now = 2_000_000_000_i64;
        let old = owned_backup_fixture(&root, "old", now - 20, 256);
        let newest = owned_backup_fixture(&root, "newest", now - 10, 256);

        let removed = enforce_update_backup_policy_at(
            &root,
            None,
            now,
            UpdateBackupPolicy {
                max_bytes: 1,
                max_entries: 1,
                retention_seconds: 1,
            },
        )
        .unwrap();

        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(newest.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_update_backup_marker_blocks_destructive_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-update-backup-corrupt-marker-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let now = 2_000_000_000_i64;
        let valid = owned_backup_fixture(&root, "valid-old", now - 1000, 64);
        let corrupt = root.join("corrupt");
        std::fs::create_dir_all(&corrupt).unwrap();
        std::fs::write(corrupt.join(UPDATE_BACKUP_OWNERSHIP_FILE), b"{broken").unwrap();

        let error = enforce_update_backup_policy_at(
            &root,
            None,
            now,
            UpdateBackupPolicy {
                max_bytes: 1,
                max_entries: 1,
                retention_seconds: 1,
            },
        )
        .unwrap_err();

        assert!(error.contains("Повреждён ownership marker"));
        assert!(valid.exists());
        assert!(corrupt.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn update_backup_root_rejects_symlink_before_cleanup_or_creation() {
        use std::os::unix::fs::symlink;

        let sandbox = std::env::temp_dir().join(format!(
            "dokkomplekt-update-backup-root-link-{}",
            Uuid::new_v4()
        ));
        let external = sandbox.join("external");
        let root_link = sandbox.join("update-backups");
        std::fs::create_dir_all(&external).unwrap();
        symlink(&external, &root_link).unwrap();

        let error = validate_update_backup_root(&root_link).unwrap_err();
        assert!(error.contains("небезопасный тип"));

        let _ = std::fs::remove_file(root_link);
        let _ = std::fs::remove_dir_all(sandbox);
    }

    #[test]
    fn changed_update_backup_marker_is_rejected_before_removal() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-update-backup-marker-race-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let backup = owned_backup_fixture(&root, "owned", 100, 64);
        let candidate = collect_owned_update_backups(&root).unwrap().remove(0);
        let marker_path = backup.join(UPDATE_BACKUP_OWNERSHIP_FILE);
        let changed = UpdateBackupOwnership {
            schema: UPDATE_BACKUP_OWNERSHIP_SCHEMA.to_string(),
            target_version: "99.0.0".to_string(),
            created_at_unix: 101,
        };
        std::fs::write(&marker_path, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();

        let error = validate_owned_update_backup_for_removal(&candidate).unwrap_err();
        assert!(error.contains("больше не соответствует кандидату"));
        assert!(backup.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_restores_required_state_and_removes_files_absent_from_backup() {
        let root = std::env::temp_dir().join(format!("dokkomplekt-update-rollback-{}", Uuid::new_v4()));
        let live = root.join("live");
        let backup = root.join("backup");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let db = live.join(DEFAULT_STATE_DB);
        let key = live.join(format!("{DEFAULT_STATE_DB}.key"));
        let wal = PathBuf::from(format!("{}-wal", db.display()));
        let shm = PathBuf::from(format!("{}-shm", db.display()));

        {
            let repo = repository_for(&db).unwrap();
            repo.quick_integrity_check().unwrap();
        }
        std::fs::copy(&db, backup.join(DEFAULT_STATE_DB)).unwrap();
        std::fs::copy(&key, backup.join(format!("{DEFAULT_STATE_DB}.key"))).unwrap();
        let original_db = std::fs::read(&db).unwrap();
        let original_key = std::fs::read(&key).unwrap();

        std::fs::write(&db, b"corrupted database").unwrap();
        std::fs::write(&key, b"corrupted key").unwrap();
        std::fs::write(&wal, b"new wal").unwrap();
        std::fs::write(&shm, b"new shm").unwrap();

        restore_update_backup_files(&db, &backup).unwrap();

        assert_eq!(std::fs::read(&db).unwrap(), original_db);
        assert_eq!(std::fs::read(&key).unwrap(), original_key);
        assert!(!wal.exists());
        assert!(!shm.exists());
        repository_for(&db).unwrap().quick_integrity_check().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_refuses_missing_required_backup_before_touching_live_state() {
        let root = std::env::temp_dir().join(format!("dokkomplekt-update-rollback-missing-{}", Uuid::new_v4()));
        let live = root.join("live");
        let backup = root.join("backup");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let db = live.join(DEFAULT_STATE_DB);
        let key = live.join(format!("{DEFAULT_STATE_DB}.key"));
        std::fs::write(&db, b"live-db").unwrap();
        std::fs::write(&key, b"live-key").unwrap();
        std::fs::write(backup.join(DEFAULT_STATE_DB), b"backup-db").unwrap();

        let error = restore_update_backup_files(&db, &backup).expect_err("missing backup key must fail closed");
        assert!(error.contains("Обязательный backup-файл недоступен"), "{error}");
        assert_eq!(std::fs::read(&db).unwrap(), b"live-db");
        assert_eq!(std::fs::read(&key).unwrap(), b"live-key");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn downloaded_package_is_reverified_before_install() {
        let root = std::env::temp_dir().join(format!("dokkomplekt-update-verify-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let package = root.join("Dokkomplekt-setup.exe");
        std::fs::write(&package, b"verified-installer").unwrap();
        let (size, hash) = sha256_file(&package).unwrap();

        verify_downloaded_package(&package, &hash, size).expect("initial package must verify");

        std::fs::write(&package, b"tampered-installer").unwrap();
        let error = verify_downloaded_package(&package, &hash, size)
            .expect_err("tampered package must fail closed");
        assert!(
            error.contains("Размер пакета изменился") || error.contains("SHA-256 пакета изменился"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn downloaded_package_rejects_symlink_or_non_file_path() {
        let root = std::env::temp_dir().join(format!("dokkomplekt-update-type-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let error = verify_downloaded_package(&root, &"0".repeat(64), 1)
            .expect_err("directory must not be accepted as installer");
        assert!(error.contains("обычный файл"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }
}
