use reqwest::blocking::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Removed in 18.3.0. Keeping the name only lets us reject stale deployments
/// explicitly instead of silently falling back to clear-text database transport.
const LEGACY_DATABASE_ENV: &str = "DOKKOMPLEKT_QUEUE_DATABASE_URL";
const MTLS_ENDPOINT_ENV: &str = "DOKKOMPLEKT_QUEUE_MTLS_URL";
const MTLS_CA_ENV: &str = "DOKKOMPLEKT_QUEUE_MTLS_CA_PEM";
const MTLS_IDENTITY_ENV: &str = "DOKKOMPLEKT_QUEUE_MTLS_IDENTITY_PEM";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
const MAX_PEM_BYTES: u64 = 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

pub(crate) enum QueueAcquireResult {
    Disabled,
    Acquired(Box<CentralQueueLease>),
    Busy,
    Completed,
}

enum QueueDecision {
    Acquired,
    Busy,
    Completed,
}

pub(crate) struct CentralQueueLease {
    client: MtlsQueueClient,
    source_sha256: String,
    worker_id: String,
    heartbeat_stop: Arc<AtomicBool>,
    completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QueueStatus {
    pub mode: String,
    pub configured: bool,
    pub reachable: bool,
    pub message: String,
}

#[derive(Debug, Clone)]
struct MtlsQueueClient {
    client: HttpClient,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct AcquireRequest<'a> {
    source_sha256: &'a str,
    worker_id: &'a str,
    allow_completed_reissue: bool,
}

#[derive(Debug, Serialize)]
struct LeaseRequest<'a> {
    source_sha256: &'a str,
    worker_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct QueueServiceResponse {
    decision: String,
    #[serde(default)]
    message: Option<String>,
}

impl MtlsQueueClient {
    fn from_env() -> Result<Option<Self>, String> {
        reject_legacy_database_transport()?;
        let Some(raw_url) = env_utf8(MTLS_ENDPOINT_ENV)? else {
            return Ok(None);
        };
        let ca_path = required_env_path(MTLS_CA_ENV)?;
        let identity_path = required_env_path(MTLS_IDENTITY_ENV)?;
        ensure_private_identity_permissions(&identity_path)?;
        let ca_pem = read_small_file(&ca_path, "CA-сертификат очереди")?;
        let identity_pem = read_small_file(&identity_path, "клиентский сертификат очереди")?;
        let root = reqwest::Certificate::from_pem(&ca_pem)
            .map_err(|error| format!("Некорректный PEM CA очереди: {error}"))?;
        let identity = reqwest::Identity::from_pem(&identity_pem).map_err(|error| {
            format!("Некорректный combined PEM клиентского сертификата/ключа очереди: {error}")
        })?;
        let parsed = reqwest::Url::parse(raw_url.trim())
            .map_err(|error| format!("Некорректный {MTLS_ENDPOINT_ENV}: {error}"))?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(format!(
                "{MTLS_ENDPOINT_ENV} должен быть чистым HTTPS URL без credentials/query/fragment."
            ));
        }
        crate::ensure_rustls_crypto_provider();
        let client = HttpClient::builder()
            .https_only(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .add_root_certificate(root)
            .identity(identity)
            .build()
            .map_err(|error| format!("Не удалось создать mTLS-клиент очереди: {error}"))?;
        Ok(Some(Self {
            client,
            base_url: parsed.as_str().trim_end_matches('/').to_string(),
        }))
    }

    fn acquire(
        &self,
        source_sha256: &str,
        worker_id: &str,
        allow_completed_reissue: bool,
    ) -> Result<QueueDecision, String> {
        let response = self.post(
            "/v1/queue/acquire",
            &AcquireRequest {
                source_sha256,
                worker_id,
                allow_completed_reissue,
            },
        )?;
        match response.decision.as_str() {
            "acquired" => Ok(QueueDecision::Acquired),
            "busy" => Ok(QueueDecision::Busy),
            "completed" => Ok(QueueDecision::Completed),
            other => Err(format!(
                "mTLS queue service вернул неизвестное решение {other:?}: {}",
                response.message.unwrap_or_default()
            )),
        }
    }

    fn renew(&self, source_sha256: &str, worker_id: &str) -> Result<(), String> {
        self.require_ok("/v1/queue/renew", source_sha256, worker_id)
    }

    fn complete(&self, source_sha256: &str, worker_id: &str) -> Result<(), String> {
        self.require_ok("/v1/queue/complete", source_sha256, worker_id)
    }

    fn retryable(&self, source_sha256: &str, worker_id: &str) -> Result<(), String> {
        self.require_ok("/v1/queue/retryable", source_sha256, worker_id)
    }

    fn health(&self) -> Result<(), String> {
        let url = format!("{}/v1/health", self.base_url);
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|error| format!("mTLS queue service недоступен: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "mTLS queue service вернул HTTP {}.",
                response.status()
            ));
        }
        Ok(())
    }

    fn require_ok(&self, route: &str, source_sha256: &str, worker_id: &str) -> Result<(), String> {
        let response = self.post(
            route,
            &LeaseRequest {
                source_sha256,
                worker_id,
            },
        )?;
        if response.decision == "ok" {
            Ok(())
        } else {
            Err(response
                .message
                .unwrap_or_else(|| format!("mTLS queue service отклонил операцию {route}")))
        }
    }

    fn post<T: Serialize>(&self, route: &str, payload: &T) -> Result<QueueServiceResponse, String> {
        let url = format!("{}{route}", self.base_url);
        let response = self
            .client
            .post(url)
            .json(payload)
            .send()
            .map_err(|error| format!("Ошибка mTLS queue service: {error}"))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("mTLS queue service вернул слишком большой ответ.".into());
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.starts_with("application/json") {
            return Err("mTLS queue service вернул ответ не в формате application/json.".into());
        }
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|error| format!("Не удалось прочитать ответ mTLS queue service: {error}"))?;
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("mTLS queue service вернул слишком большой ответ.".into());
        }
        let parsed = serde_json::from_slice::<QueueServiceResponse>(&body)
            .map_err(|error| format!("Некорректный JSON mTLS queue service: {error}"))?;
        if !status.is_success() {
            return Err(parsed
                .message
                .unwrap_or_else(|| format!("mTLS queue service вернул HTTP {status}")));
        }
        Ok(parsed)
    }
}

impl CentralQueueLease {
    pub(crate) fn acquire_from_env(
        source_sha256: &str,
        allow_completed_reissue: bool,
    ) -> Result<QueueAcquireResult, String> {
        validate_sha256(source_sha256)?;
        let Some(client) = MtlsQueueClient::from_env()? else {
            return Ok(QueueAcquireResult::Disabled);
        };
        let worker_id = worker_id();
        match client.acquire(source_sha256, &worker_id, allow_completed_reissue)? {
            QueueDecision::Completed => Ok(QueueAcquireResult::Completed),
            QueueDecision::Busy => Ok(QueueAcquireResult::Busy),
            QueueDecision::Acquired => {
                let heartbeat_stop = Arc::new(AtomicBool::new(false));
                spawn_heartbeat(
                    client.clone(),
                    source_sha256.to_string(),
                    worker_id.clone(),
                    Arc::clone(&heartbeat_stop),
                );
                Ok(QueueAcquireResult::Acquired(Box::new(Self {
                    client,
                    source_sha256: source_sha256.to_string(),
                    worker_id,
                    heartbeat_stop,
                    completed: false,
                })))
            }
        }
    }

    pub(crate) fn renew(&mut self) -> Result<(), String> {
        self.client.renew(&self.source_sha256, &self.worker_id)
    }

    pub(crate) fn complete(&mut self) -> Result<(), String> {
        self.client.complete(&self.source_sha256, &self.worker_id)?;
        self.completed = true;
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn spawn_heartbeat(
    client: MtlsQueueClient,
    source_sha256: String,
    worker_id: String,
    stop: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let ticks = HEARTBEAT_INTERVAL.as_secs().max(1);
            for _ in 0..ticks {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            if client.renew(&source_sha256, &worker_id).is_err() {
                return;
            }
        }
    });
}

impl Drop for CentralQueueLease {
    fn drop(&mut self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        if !self.completed {
            let _ = self.client.retryable(&self.source_sha256, &self.worker_id);
        }
    }
}

pub(crate) fn status() -> QueueStatus {
    match MtlsQueueClient::from_env() {
        Err(message) => QueueStatus {
            mode: "configuration_error".into(),
            configured: true,
            reachable: false,
            message,
        },
        Ok(Some(client)) => match client.health() {
            Ok(()) => QueueStatus {
                mode: "central_mtls".into(),
                configured: true,
                reachable: true,
                message: "Центральная очередь доступна по взаимно аутентифицированному TLS."
                    .into(),
            },
            Err(message) => QueueStatus {
                mode: "central_mtls".into(),
                configured: true,
                reachable: false,
                message,
            },
        },
        Ok(None) => QueueStatus {
            mode: "shared_filesystem".into(),
            configured: false,
            reachable: true,
            message: format!(
                "Используется файловая SHA-256 очередь. Для нескольких компьютеров задайте {MTLS_ENDPOINT_ENV}, {MTLS_CA_ENV} и {MTLS_IDENTITY_ENV}."
            ),
        },
    }
}

fn reject_legacy_database_transport() -> Result<(), String> {
    if env_utf8(LEGACY_DATABASE_ENV)?.is_some() {
        return Err(format!(
            "{LEGACY_DATABASE_ENV} больше не поддерживается: прямой PostgreSQL-транспорт удалён из desktop-приложения. Запустите scripts/queue_mtls_service.py и настройте {MTLS_ENDPOINT_ENV}, {MTLS_CA_ENV}, {MTLS_IDENTITY_ENV}."
        ));
    }
    Ok(())
}

fn env_utf8(name: &str) -> Result<Option<String>, String> {
    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };
    let value = value
        .into_string()
        .map_err(|_| format!("{name} содержит не-UTF-8 значение."))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{name} задан пустым значением."));
    }
    Ok(Some(trimmed.to_string()))
}

fn required_env_path(name: &str) -> Result<PathBuf, String> {
    let value = env_utf8(name)?.ok_or_else(|| format!("Для mTLS очереди требуется {name}."))?;
    let path = PathBuf::from(value);
    if !path.is_absolute() || !path.is_file() {
        return Err(format!(
            "{name} должен указывать на существующий абсолютный файл."
        ));
    }
    Ok(path)
}

fn read_small_file(path: &Path, title: &str) -> Result<Vec<u8>, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Не удалось прочитать {title}: {error}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_PEM_BYTES {
        return Err(format!(
            "{title} имеет недопустимый размер {} байт.",
            metadata.len()
        ));
    }
    fs::read(path).map_err(|error| format!("Не удалось прочитать {title}: {error}"))
}

#[cfg(unix)]
fn ensure_private_identity_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(path)
        .map_err(|error| format!("Не удалось проверить права клиентского ключа: {error}"))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "Клиентский PEM {} доступен группе/остальным; установите chmod 600.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_identity_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("source_sha256 должен быть 64-символьным SHA-256 в hex.".into())
    }
}

fn worker_id() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".into());
    format!("{host}:{}:{}", std::process::id(), Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_contract_rejects_short_or_non_hex_values() {
        assert!(validate_sha256(&"a".repeat(64)).is_ok());
        assert!(validate_sha256("abc").is_err());
        assert!(validate_sha256(&"z".repeat(64)).is_err());
    }

    #[test]
    fn legacy_database_transport_is_named_only_for_fail_closed_migration() {
        assert_eq!(LEGACY_DATABASE_ENV, "DOKKOMPLEKT_QUEUE_DATABASE_URL");
        assert!(MTLS_ENDPOINT_ENV.contains("MTLS"));
    }
}
