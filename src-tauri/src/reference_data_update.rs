//! Signed production-calendar updates.
//!
//! The bundled calendar remains the fail-closed fallback. A newer calendar may
//! replace it only when an Ed25519-signed package is downloaded from the pinned
//! HTTPS feed or imported by an administrator. The package is persisted in app
//! data and re-verified on every launch.

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use dokkomplekt_refdata::{
    install_production_calendar_override, parse_production_calendar, ProductionCalendar,
};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read as _;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024;
const PACKAGE_FILE: &str = "production_calendar_ru.signed.json";
const AUTO_CHECK_FILE: &str = "production_calendar_auto_update.json";
const AUTO_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
static CALENDAR_ACTIVE: AtomicBool = AtomicBool::new(false);
const TRUSTED_REFDATA_PUBKEY_B64: &str = match option_env!("DOKKOMPLEKT_REFDATA_PUBKEY_B64") {
    Some(key) => key,
    None => "jIswwPnOeUrKVFTPi9vZ9ZM7roY3iO2xXw0vWMSyVFY=",
};
const TRUSTED_REFDATA_URL: &str = match option_env!("DOKKOMPLEKT_REFDATA_URL") {
    Some(url) => url,
    None => "https://updates.dokkomplekt.invalid/reference-data/production-calendar-ru.json",
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReferenceDataPayload {
    schema: String,
    published_at: String,
    calendar_tsv_b64: String,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignedReferenceDataPackage {
    payload: ReferenceDataPayload,
    signature_alg: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceDataStatus {
    pub installed: bool,
    pub cached: bool,
    pub restart_required: bool,
    pub source: String,
    pub published_at: Option<String>,
    pub complete_years: Vec<i32>,
    pub listed_years: Vec<i32>,
    pub message: String,
}

#[derive(Debug, Clone)]
struct VerifiedPackage {
    package: SignedReferenceDataPackage,
    calendar: ProductionCalendar,
}

pub fn cached_package_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("reference-data").join(PACKAGE_FILE)
}

pub fn load_cached(app_data_dir: &Path) -> Result<ReferenceDataStatus, String> {
    let path = cached_package_path(app_data_dir);
    if !path.is_file() {
        return Ok(ReferenceDataStatus {
            installed: false,
            cached: false,
            restart_required: false,
            source: "bundled".into(),
            published_at: None,
            complete_years: Vec::new(),
            listed_years: Vec::new(),
            message: "Подписанное обновление календаря не установлено; используется встроенный fail-closed справочник.".into(),
        });
    }
    let bytes = read_limited_file(&path, MAX_PACKAGE_BYTES)?;
    let verified = verify_package(&bytes)?;
    install_verified(verified, "cached")
}

pub fn status(app_data_dir: &Path) -> Result<ReferenceDataStatus, String> {
    let path = cached_package_path(app_data_dir);
    if !path.is_file() {
        return Ok(ReferenceDataStatus {
            installed: false,
            cached: false,
            restart_required: false,
            source: "bundled".into(),
            published_at: None,
            complete_years: Vec::new(),
            listed_years: Vec::new(),
            message: "Подписанное обновление календаря не установлено; используется встроенный fail-closed справочник.".into(),
        });
    }
    let bytes = read_limited_file(&path, MAX_PACKAGE_BYTES)?;
    let verified = verify_package(&bytes)?;
    let complete_years = verified
        .calendar
        .listed_years()
        .filter(|year| verified.calendar.is_year_complete(*year))
        .collect::<Vec<_>>();
    let listed_years = verified.calendar.listed_years().collect::<Vec<_>>();
    let installed = CALENDAR_ACTIVE.load(Ordering::SeqCst);
    Ok(ReferenceDataStatus {
        installed,
        cached: true,
        restart_required: !installed,
        source: "cached".into(),
        published_at: Some(verified.package.payload.published_at),
        complete_years,
        listed_years,
        message: if installed {
            "Подписанный производственный календарь активен.".into()
        } else {
            "Подписанный производственный календарь проверен и будет активирован после перезапуска."
                .into()
        },
    })
}

pub fn import_package(app_data_dir: &Path, source: &Path) -> Result<ReferenceDataStatus, String> {
    let bytes = read_limited_file(source, MAX_PACKAGE_BYTES)?;
    import_package_bytes(app_data_dir, &bytes)
}

pub fn import_package_bytes(
    app_data_dir: &Path,
    bytes: &[u8],
) -> Result<ReferenceDataStatus, String> {
    if bytes.is_empty() {
        return Err("Пакет календаря пуст".into());
    }
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err("Пакет календаря превышает допустимый размер".into());
    }
    let verified = verify_package(bytes)?;
    persist_package(app_data_dir, bytes)?;
    install_verified(verified, "imported")
}

pub fn automatic_feed_configured() -> bool {
    reference_data_url().ok().is_some_and(|raw| {
        reqwest::Url::parse(&raw)
            .ok()
            .filter(|url| {
                url.scheme() == "https"
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
            })
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| !crate::is_forbidden_public_download_host(&host))
    })
}

fn reference_data_url() -> Result<String, String> {
    let raw = std::env::var("DOKKOMPLEKT_REFDATA_URL")
        .unwrap_or_else(|_| TRUSTED_REFDATA_URL.to_string());
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Err("URL подписанного календарного feed не настроен".into())
    } else {
        Ok(trimmed.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoUpdateRecord {
    checked_unix_seconds: u64,
    success: bool,
    error: Option<String>,
}

pub fn maybe_auto_update(app_data_dir: &Path) -> Result<Option<ReferenceDataStatus>, String> {
    if !automatic_feed_configured() {
        return Ok(None);
    }
    let status_path = app_data_dir.join("reference-data").join(AUTO_CHECK_FILE);
    if status_path
        .metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed < AUTO_CHECK_INTERVAL)
    {
        return Ok(None);
    }
    let result = download_and_install(app_data_dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let record = AutoUpdateRecord {
        checked_unix_seconds: now,
        success: result.is_ok(),
        error: result.as_ref().err().cloned(),
    };
    if let Some(parent) = status_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = status_path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if status_path.exists() {
        fs::remove_file(&status_path).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, &status_path).map_err(|error| error.to_string())?;
    result.map(Some)
}

pub fn download_and_install(app_data_dir: &Path) -> Result<ReferenceDataStatus, String> {
    let feed_url = reference_data_url()?;
    let validated = validate_https_url(&feed_url)?;
    crate::ensure_rustls_crypto_provider();
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(45))
        .resolve_to_addrs(&validated.host, &validated.addresses)
        .build()
        .map_err(|error| format!("Не удалось создать клиент обновления календаря: {error}"))?;
    let response = client
        .get(validated.url)
        .send()
        .map_err(|error| format!("Не удалось скачать подписанный календарь: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Сервер календаря вернул HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PACKAGE_BYTES)
    {
        return Err("Пакет календаря превышает допустимый размер".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_PACKAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось прочитать пакет календаря: {error}"))?;
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err("Пакет календаря превышает допустимый размер".into());
    }
    let verified = verify_package(&bytes)?;
    persist_package(app_data_dir, &bytes)?;
    install_verified(verified, "signed-feed")
}

fn install_verified(
    verified: VerifiedPackage,
    source: &str,
) -> Result<ReferenceDataStatus, String> {
    let complete_years = verified
        .calendar
        .listed_years()
        .filter(|year| verified.calendar.is_year_complete(*year))
        .collect::<Vec<_>>();
    let listed_years = verified.calendar.listed_years().collect::<Vec<_>>();
    let activation = install_production_calendar_override(verified.calendar);
    let active = activation.is_ok();
    if active {
        CALENDAR_ACTIVE.store(true, Ordering::SeqCst);
    }
    let restart_required = !active && CALENDAR_ACTIVE.load(Ordering::SeqCst);
    if !active && !restart_required {
        return Err(activation
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "Не удалось активировать календарь".into()));
    }
    Ok(ReferenceDataStatus {
        installed: active || CALENDAR_ACTIVE.load(Ordering::SeqCst),
        cached: true,
        restart_required,
        source: source.into(),
        published_at: Some(verified.package.payload.published_at),
        complete_years,
        listed_years,
        message: if restart_required {
            "Новый подписанный календарь сохранён и будет активирован после перезапуска приложения."
                .into()
        } else {
            "Подписанный производственный календарь проверен Ed25519 и активирован.".into()
        },
    })
}

fn persist_package(app_data_dir: &Path, bytes: &[u8]) -> Result<(), String> {
    let destination = cached_package_path(app_data_dir);
    let parent = destination
        .parent()
        .ok_or_else(|| "Некорректный путь хранилища календаря".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".{PACKAGE_FILE}.tmp-{}", std::process::id()));
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| error.to_string())?;
    }
    fs::rename(&temporary, &destination).map_err(|error| error.to_string())
}

fn verify_package(bytes: &[u8]) -> Result<VerifiedPackage, String> {
    let package: SignedReferenceDataPackage = serde_json::from_slice(bytes)
        .map_err(|error| format!("Некорректный JSON календаря: {error}"))?;
    if package.payload.schema != "dokkomplekt.reference-data.v1" {
        return Err("Неподдерживаемая схема пакета календаря".into());
    }
    if !package.signature_alg.eq_ignore_ascii_case("ed25519") {
        return Err("Пакет календаря должен быть подписан Ed25519".into());
    }
    let key_bytes = BASE64_STANDARD
        .decode(TRUSTED_REFDATA_PUBKEY_B64.trim())
        .map_err(|_| "Некорректный встроенный public key календаря".to_string())?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Public key календаря должен содержать 32 байта".to_string())?;
    let key = VerifyingKey::from_bytes(&key_array)
        .map_err(|_| "Некорректный public key календаря".to_string())?;
    let signature_bytes = BASE64_STANDARD
        .decode(package.signature.trim())
        .map_err(|_| "Некорректная base64-подпись календаря".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "Некорректная длина подписи календаря".to_string())?;
    let payload = serde_json::to_value(&package.payload).map_err(|error| error.to_string())?;
    let canonical = canonical_json_bytes(&payload)?;
    key.verify(&canonical, &signature)
        .map_err(|_| "Подпись календаря не прошла проверку".to_string())?;

    let calendar_bytes = BASE64_STANDARD
        .decode(package.payload.calendar_tsv_b64.trim())
        .map_err(|_| "calendar_tsv_b64 не является корректным base64".to_string())?;
    let actual_sha256 = hex::encode(Sha256::digest(&calendar_bytes));
    if !actual_sha256.eq_ignore_ascii_case(package.payload.sha256.trim()) {
        return Err("SHA-256 календаря не совпадает с подписанным manifest".into());
    }
    let calendar_text = String::from_utf8(calendar_bytes)
        .map_err(|_| "Производственный календарь должен быть UTF-8 TSV".to_string())?;
    let calendar = parse_production_calendar(&calendar_text)?;
    Ok(VerifiedPackage { package, calendar })
}

fn read_limited_file(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes {
        return Err(format!("Файл {} слишком большой", path.display()));
    }
    fs::read(path).map_err(|error| error.to_string())
}

#[derive(Debug)]
struct ValidatedUrl {
    url: reqwest::Url,
    host: String,
    addresses: Vec<SocketAddr>,
}

fn validate_https_url(raw: &str) -> Result<ValidatedUrl, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Некорректный URL календаря".to_string())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Календарь разрешено загружать только по HTTPS без credentials/fragment".into(),
        );
    }
    let host = url
        .host_str()
        .ok_or_else(|| "В URL календаря отсутствует host".to_string())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if crate::is_forbidden_public_download_host(&host) {
        return Err(
            "Placeholder, local или некорректный host запрещён для календарного feed".into(),
        );
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "Не определён HTTPS-порт календаря".to_string())?;
    let mut addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "Не удалось разрешить адрес сервера календаря".to_string())?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| forbidden_ip(address.ip())) {
        return Err("Private/loopback/служебные IP запрещены для календарного feed".into());
    }
    Ok(ValidatedUrl {
        url,
        host,
        addresses,
    })
}

fn forbidden_ip(ip: IpAddr) -> bool {
    crate::is_forbidden_public_download_ip(ip)
}

fn canonical_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    fn write(value: &serde_json::Value, out: &mut Vec<u8>) -> Result<(), String> {
        match value {
            serde_json::Value::Null => out.extend_from_slice(b"null"),
            serde_json::Value::Bool(value) => {
                out.extend_from_slice(if *value { b"true" } else { b"false" })
            }
            serde_json::Value::Number(value) => out.extend_from_slice(value.to_string().as_bytes()),
            serde_json::Value::String(value) => {
                serde_json::to_writer(&mut *out, value).map_err(|error| error.to_string())?
            }
            serde_json::Value::Array(values) => {
                out.push(b'[');
                for (index, item) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(b',');
                    }
                    write(item, out)?;
                }
                out.push(b']');
            }
            serde_json::Value::Object(values) => {
                out.push(b'{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.iter().enumerate() {
                    if index > 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, key).map_err(|error| error.to_string())?;
                    out.push(b':');
                    write(&values[*key], out)?;
                }
                out.push(b'}');
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    write(value, &mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses_are_rejected() {
        assert!(forbidden_ip("127.0.0.1".parse().expect("ip")));
        assert!(forbidden_ip("10.0.0.1".parse().expect("ip")));
        assert!(forbidden_ip("100.64.0.1".parse().expect("ip")));
        assert!(forbidden_ip("198.18.0.1".parse().expect("ip")));
        assert!(forbidden_ip("::ffff:127.0.0.1".parse().expect("ip")));
        assert!(!forbidden_ip("1.1.1.1".parse().expect("ip")));
        assert!(crate::is_forbidden_public_download_host(
            "updates.example.com"
        ));
        assert!(!crate::is_forbidden_public_download_host(
            "updates.dokkomplekt.ru"
        ));
    }

    #[test]
    fn canonical_json_is_key_order_independent() {
        let left = serde_json::json!({"b": 2, "a": 1});
        let right = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(
            canonical_json_bytes(&left).expect("left"),
            canonical_json_bytes(&right).expect("right")
        );
    }
}
