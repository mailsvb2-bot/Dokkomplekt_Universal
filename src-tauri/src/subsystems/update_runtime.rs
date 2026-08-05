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

fn is_forbidden_update_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 224
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
    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
    {
        return Err("Локальный адрес запрещён для обновлений".to_string());
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
            .any(|address| is_forbidden_update_ip(address.ip()))
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
