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

#[tauri::command]
fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateCheckResponse, String> {
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
    {
        let repo = repository_for(&db_path)?;
        repo.quick_integrity_check()
            .map_err(|error| format!("Локальная база не прошла integrity check: {error}"))?;
    }
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let backup_dir = data_dir
        .join("update-backups")
        .join(format!("{}-{}", target_version, Uuid::new_v4()));
    std::fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;

    copy_update_backup_file(&db_path, &backup_dir)?;
    let key_path = db_path.with_file_name(format!(
        "{}.key",
        db_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(DEFAULT_STATE_DB)
    ));
    copy_update_backup_file(&key_path, &backup_dir)?;
    copy_update_backup_file(&PathBuf::from(format!("{}-wal", db_path.display())), &backup_dir)?;
    copy_update_backup_file(&PathBuf::from(format!("{}-shm", db_path.display())), &backup_dir)?;
    Ok(backup_dir)
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
        state.status = "recoverable_failure".to_string();
        state.last_error = Some(
            "Installer был запущен, но приложение стартовало в прежней версии; backup сохранён."
                .to_string(),
        );
        write_update_recovery_state(app, &state)?;
    }
    Ok(())
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
    #[cfg(not(target_os = "windows"))]
    let _ = &recovery_path;

    if let Ok(mut watcher) = state.watcher.lock() {
        if let Some(handle) = watcher.take() {
            handle.stop.store(true, Ordering::SeqCst);
        }
    }

    verify_downloaded_package(&canonical_package, &recovery.package_sha256, size_bytes)?;

    #[cfg(target_os = "windows")]
    {
        let child = std::process::Command::new(&canonical_package)
            .arg("/S")
            .spawn()
            .map_err(|error| format!("Не удалось запустить проверенный installer: {error}"))?;
        recovery.status = "installer_started".to_string();
        recovery.last_error = None;
        write_update_recovery_state(&app, &recovery)?;
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
        return Ok(response);
    }

    #[cfg(not(target_os = "windows"))]
    {
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
