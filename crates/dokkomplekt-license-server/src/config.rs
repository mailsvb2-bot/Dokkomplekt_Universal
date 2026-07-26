use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub public_base_url: String,
    pub issuer_id: String,
    pub issuer_key_b64: Option<String>,
    pub default_license_days: i64,
    pub payment_provider: String,
    pub storage_mode: String,
    pub database_url: Option<String>,
    pub provider_callback_secret: Option<String>,
    pub license_issue_secret: Option<String>,
    pub yookassa_shop_id: Option<String>,
    pub yookassa_secret_key: Option<String>,
    pub yookassa_api_base_url: String,
}

impl ServerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("DOKKOMPLEKT_LICENSE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
            .parse()?;
        let public_base_url = std::env::var("DOKKOMPLEKT_LICENSE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string());
        let issuer_id = std::env::var("DOKKOMPLEKT_LICENSE_ISSUER")
            .unwrap_or_else(|_| "dokkomplekt-license-server".to_string());
        let issuer_key_b64 = non_empty_env("DOKKOMPLEKT_LICENSE_ISSUER_KEY_B64");
        let default_license_days = std::env::var("DOKKOMPLEKT_DEFAULT_LICENSE_DAYS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(365);
        let payment_provider = normalize_payment_provider(
            &std::env::var("DOKKOMPLEKT_PAYMENT_PROVIDER").unwrap_or_else(|_| "manual".to_string()),
        )
        .unwrap_or_else(|| "manual".to_string());
        let strict_runtime = strict_runtime_required();
        if strict_runtime && payment_provider == "manual" {
            anyhow::bail!("manual payment provider is not allowed for license server runtime");
        }
        let database_url = non_empty_env("DATABASE_URL");
        if let Some(database_url) = database_url.as_deref() {
            validate_database_transport(database_url, strict_runtime)?;
        }
        if database_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .is_none()
            && strict_runtime
        {
            anyhow::bail!("PostgreSQL connection is required for license server runtime");
        }
        let provider_callback_secret = non_empty_env("DOKKOMPLEKT_PROVIDER_CALLBACK_SECRET");
        let license_issue_secret = non_empty_env("DOKKOMPLEKT_LICENSE_ISSUE_SECRET");
        let yookassa_shop_id = non_empty_env("DOKKOMPLEKT_YOOKASSA_SHOP_ID");
        let yookassa_secret_key = non_empty_env("DOKKOMPLEKT_YOOKASSA_SECRET_KEY");
        let yookassa_api_base_url = std::env::var("DOKKOMPLEKT_YOOKASSA_API_BASE_URL")
            .unwrap_or_else(|_| "https://api.yookassa.ru".to_string());
        if payment_provider == "yookassa"
            && (yookassa_shop_id.is_none() || yookassa_secret_key.is_none())
        {
            anyhow::bail!(
                "DOKKOMPLEKT_YOOKASSA_SHOP_ID and DOKKOMPLEKT_YOOKASSA_SECRET_KEY are required for YooKassa"
            );
        }
        if strict_runtime && issuer_key_b64.is_none() {
            anyhow::bail!(
                "DOKKOMPLEKT_LICENSE_ISSUER_KEY_B64 is required for license server runtime"
            );
        }
        if strict_runtime && provider_callback_secret.is_none() {
            anyhow::bail!(
                "DOKKOMPLEKT_PROVIDER_CALLBACK_SECRET is required for license server runtime"
            );
        }
        if strict_runtime && license_issue_secret.is_none() {
            anyhow::bail!(
                "DOKKOMPLEKT_LICENSE_ISSUE_SECRET is required for license server runtime"
            );
        }
        let storage_mode = match database_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            Some(_) => "postgres".to_string(),
            None => "memory".to_string(),
        };
        Ok(Self {
            bind_addr,
            public_base_url,
            issuer_id,
            issuer_key_b64,
            default_license_days,
            payment_provider,
            storage_mode,
            database_url,
            provider_callback_secret,
            license_issue_secret,
            yookassa_shop_id,
            yookassa_secret_key,
            yookassa_api_base_url,
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn strict_runtime_required() -> bool {
    for name in [
        "DOKKOMPLEKT_ENV",
        "DOKKOMPLEKT_LICENSE_ENV",
        "APP_ENV",
        "RUST_ENV",
        "ENV",
    ] {
        let value = std::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if matches!(value.as_str(), "production" | "prod") {
            return true;
        }
    }
    false
}

pub(crate) fn validate_database_transport(
    database_url: &str,
    strict_runtime: bool,
) -> anyhow::Result<()> {
    let endpoint = database_endpoint(database_url)?;
    match endpoint {
        DatabaseEndpoint::UnixSocket => Ok(()),
        DatabaseEndpoint::Loopback if !strict_runtime => Ok(()),
        DatabaseEndpoint::Loopback => anyhow::bail!(
            "production license server requires PostgreSQL through a local Unix-domain socket; loopback TCP still uses the intentionally local-only NoTls connector"
        ),
        DatabaseEndpoint::Remote(host) => anyhow::bail!(
            "remote PostgreSQL host {host:?} is forbidden: this build never sends license-server credentials through the NoTls connector. Co-locate PostgreSQL and use a Unix socket, or add a separately audited TLS database connector"
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DatabaseEndpoint {
    UnixSocket,
    Loopback,
    Remote(String),
}

fn database_endpoint(database_url: &str) -> anyhow::Result<DatabaseEndpoint> {
    let value = database_url.trim();
    if value.is_empty() {
        anyhow::bail!("DATABASE_URL is empty");
    }
    if value.starts_with("postgres://") || value.starts_with("postgresql://") {
        return database_endpoint_from_url(value);
    }
    database_endpoint_from_keyword_dsn(value)
}

fn database_endpoint_from_url(value: &str) -> anyhow::Result<DatabaseEndpoint> {
    let without_scheme = value
        .split_once("://")
        .map(|(_, remainder)| remainder)
        .ok_or_else(|| anyhow::anyhow!("DATABASE_URL has no PostgreSQL scheme"))?;
    let authority = without_scheme
        .split(|character| matches!(character, '/' | '?' | '#'))
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        if let Some(host) = query_parameter(value, "host") {
            return classify_database_host(&host);
        }
        return Ok(DatabaseEndpoint::UnixSocket);
    }
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed
            .split_once(']')
            .map(|(host, _)| host)
            .ok_or_else(|| anyhow::anyhow!("DATABASE_URL contains an invalid IPv6 host"))?
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    classify_database_host(host)
}

fn database_endpoint_from_keyword_dsn(value: &str) -> anyhow::Result<DatabaseEndpoint> {
    let host = value
        .split_whitespace()
        .find_map(|part| part.strip_prefix("host="))
        .map(|host| host.trim_matches(|character| matches!(character, '\'' | '"')));
    match host {
        Some(host) => classify_database_host(host),
        None => Ok(DatabaseEndpoint::UnixSocket),
    }
}

fn query_parameter(value: &str, name: &str) -> Option<String> {
    let query = value
        .split_once('?')?
        .1
        .split('#')
        .next()
        .unwrap_or_default();
    query.split('&').find_map(|part| {
        let (key, raw_value) = part.split_once('=')?;
        (key == name).then(|| raw_value.replace("%2F", "/").replace("%2f", "/"))
    })
}

fn classify_database_host(host: &str) -> anyhow::Result<DatabaseEndpoint> {
    let normalized = host
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'));
    if normalized.is_empty() || normalized.starts_with('/') {
        return Ok(DatabaseEndpoint::UnixSocket);
    }
    if normalized.eq_ignore_ascii_case("localhost") {
        return Ok(DatabaseEndpoint::Loopback);
    }
    if normalized
        .parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
    {
        return Ok(DatabaseEndpoint::Loopback);
    }
    Ok(DatabaseEndpoint::Remote(normalized.to_string()))
}

pub fn normalize_payment_provider(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Some("manual".to_string()),
        "yookassa" => Some("yookassa".to_string()),
        "sbp" => Some("sbp".to_string()),
        "bank_invoice" => Some("bank_invoice".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        database_endpoint, normalize_payment_provider, validate_database_transport,
        DatabaseEndpoint,
    };

    #[test]
    fn payment_provider_names_are_normalized() {
        assert_eq!(
            normalize_payment_provider(" manual ").as_deref(),
            Some("manual")
        );
        assert_eq!(
            normalize_payment_provider("YooKassa").as_deref(),
            Some("yookassa")
        );
        assert_eq!(normalize_payment_provider("SBP").as_deref(), Some("sbp"));
        assert_eq!(
            normalize_payment_provider("bank_invoice").as_deref(),
            Some("bank_invoice")
        );
    }

    #[test]
    fn unknown_payment_provider_is_rejected() {
        assert!(normalize_payment_provider("unsupported").is_none());
    }

    #[test]
    fn database_transport_accepts_unix_socket_and_dev_loopback_only() {
        assert_eq!(
            database_endpoint("postgresql:///dokkomplekt?host=/var/run/postgresql").unwrap(),
            DatabaseEndpoint::UnixSocket
        );
        assert_eq!(
            database_endpoint("host=/run/postgresql dbname=dokkomplekt").unwrap(),
            DatabaseEndpoint::UnixSocket
        );
        assert!(validate_database_transport(
            "postgresql://user:secret@127.0.0.1:5432/dokkomplekt",
            false
        )
        .is_ok());
        assert!(validate_database_transport(
            "postgresql://user:secret@[::1]:5432/dokkomplekt",
            false
        )
        .is_ok());
    }

    #[test]
    fn database_transport_rejects_remote_and_production_loopback_notls() {
        assert!(validate_database_transport(
            "postgresql://user:secret@db.example.com/dokkomplekt",
            false
        )
        .is_err());
        assert!(validate_database_transport(
            "postgresql://user:secret@localhost/dokkomplekt",
            true
        )
        .is_err());
        assert!(validate_database_transport(
            "postgresql:///dokkomplekt?host=/var/run/postgresql",
            true
        )
        .is_ok());
    }
}
