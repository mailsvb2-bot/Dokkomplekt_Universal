use crate::traffic_guard::TrustedProxyConfig;
use std::net::{IpAddr, SocketAddr};

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
    pub order_recovery_secret: Option<String>,
    pub yookassa_shop_id: Option<String>,
    pub yookassa_secret_key: Option<String>,
    pub yookassa_api_base_url: String,
    pub global_concurrency_limit: usize,
    pub provider_concurrency_limit: usize,
    pub request_timeout_seconds: u64,
    pub order_create_limit_per_hour: u32,
    pub order_access_limit_per_minute: u32,
    pub provider_callback_limit_per_minute: u32,
    pub order_recovery_limit_per_minute: u32,
    pub trusted_proxies: TrustedProxyConfig,
}

impl ServerConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("DOKKOMPLEKT_LICENSE_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
            .parse()?;
        let strict_runtime = strict_runtime_required();
        let public_base_url = validate_public_base_url(
            &std::env::var("DOKKOMPLEKT_LICENSE_PUBLIC_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string()),
            strict_runtime,
        )?;
        let issuer_id = std::env::var("DOKKOMPLEKT_LICENSE_ISSUER")
            .unwrap_or_else(|_| "dokkomplekt-license-server".to_string());
        let issuer_key_b64 = non_empty_env("DOKKOMPLEKT_LICENSE_ISSUER_KEY_B64");
        let default_license_days = std::env::var("DOKKOMPLEKT_DEFAULT_LICENSE_DAYS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(365);
        let payment_provider_raw =
            std::env::var("DOKKOMPLEKT_PAYMENT_PROVIDER").unwrap_or_else(|_| "manual".to_string());
        let payment_provider =
            normalize_payment_provider(&payment_provider_raw).ok_or_else(|| {
                anyhow::anyhow!("unsupported payment provider: {payment_provider_raw}")
            })?;
        if strict_runtime && payment_provider != "yookassa" {
            anyhow::bail!(
                "production license server currently supports only the verified yookassa provider"
            );
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
        let order_recovery_secret = non_empty_env("DOKKOMPLEKT_ORDER_RECOVERY_SECRET");
        let yookassa_shop_id = non_empty_env("DOKKOMPLEKT_YOOKASSA_SHOP_ID");
        let yookassa_secret_key = non_empty_env("DOKKOMPLEKT_YOOKASSA_SECRET_KEY");
        let yookassa_api_base_url = validate_yookassa_api_base_url(
            &std::env::var("DOKKOMPLEKT_YOOKASSA_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.yookassa.ru".to_string()),
            strict_runtime && payment_provider == "yookassa",
        )?;
        let global_concurrency_limit =
            bounded_usize_env("DOKKOMPLEKT_GLOBAL_CONCURRENCY_LIMIT", 128, 8, 1_024);
        let provider_concurrency_limit =
            bounded_usize_env("DOKKOMPLEKT_PROVIDER_CONCURRENCY_LIMIT", 8, 1, 64);
        let request_timeout_seconds =
            bounded_u64_env("DOKKOMPLEKT_REQUEST_TIMEOUT_SECONDS", 30, 5, 120);
        let order_create_limit_per_hour =
            bounded_u32_env("DOKKOMPLEKT_ORDER_CREATE_LIMIT_PER_HOUR", 20, 1, 1_000);
        let order_access_limit_per_minute =
            bounded_u32_env("DOKKOMPLEKT_ORDER_ACCESS_LIMIT_PER_MINUTE", 120, 1, 10_000);
        let provider_callback_limit_per_minute = bounded_u32_env(
            "DOKKOMPLEKT_PROVIDER_CALLBACK_LIMIT_PER_MINUTE",
            120,
            1,
            10_000,
        );
        let order_recovery_limit_per_minute =
            bounded_u32_env("DOKKOMPLEKT_ORDER_RECOVERY_LIMIT_PER_MINUTE", 30, 1, 1_000);
        let trusted_proxy_cidrs = non_empty_env("DOKKOMPLEKT_TRUSTED_PROXY_CIDRS");
        let require_forwarded_for =
            boolean_env("DOKKOMPLEKT_TRUSTED_PROXY_REQUIRE_X_FORWARDED_FOR", true)?;
        let trusted_proxies =
            TrustedProxyConfig::parse(trusted_proxy_cidrs.as_deref(), require_forwarded_for)
                .map_err(anyhow::Error::msg)?;
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
        if strict_runtime && order_recovery_secret.is_none() {
            anyhow::bail!(
                "DOKKOMPLEKT_ORDER_RECOVERY_SECRET is required for legacy-order recovery"
            );
        }
        validate_distinct_server_secrets([
            provider_callback_secret.as_deref(),
            license_issue_secret.as_deref(),
            order_recovery_secret.as_deref(),
        ])?;
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
            order_recovery_secret,
            yookassa_shop_id,
            yookassa_secret_key,
            yookassa_api_base_url,
            global_concurrency_limit,
            provider_concurrency_limit,
            request_timeout_seconds,
            order_create_limit_per_hour,
            order_access_limit_per_minute,
            provider_callback_limit_per_minute,
            order_recovery_limit_per_minute,
            trusted_proxies,
        })
    }
}

fn boolean_env(name: &str, default: bool) -> anyhow::Result<bool> {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => anyhow::bail!("{name} must be a boolean"),
        },
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn validate_distinct_server_secrets<const N: usize>(
    secrets: [Option<&str>; N],
) -> anyhow::Result<()> {
    let values = secrets
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    for (index, left) in values.iter().enumerate() {
        if values[index + 1..].iter().any(|right| left == right) {
            anyhow::bail!("license-server control secrets must be distinct");
        }
    }
    Ok(())
}

fn bounded_usize_env(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

fn bounded_u64_env(name: &str, default: u64, minimum: u64, maximum: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

fn bounded_u32_env(name: &str, default: u32, minimum: u32, maximum: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

pub(crate) fn validate_yookassa_api_base_url(
    raw_value: &str,
    production: bool,
) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(raw_value.trim())?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("YooKassa API URL must not contain credentials, query or fragment");
    }
    if !matches!(url.path(), "" | "/") {
        anyhow::bail!("YooKassa API URL must point to the API origin, not to a nested path");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("YooKassa API URL has no host"))?;
    let official = url.scheme() == "https"
        && host.eq_ignore_ascii_case("api.yookassa.ru")
        && url.port_or_known_default() == Some(443);
    let loopback = !production
        && matches!(url.scheme(), "http" | "https")
        && (host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback()));
    if !official && !loopback {
        anyhow::bail!(
            "YooKassa credentials may be sent only to https://api.yookassa.ru; development overrides must use loopback"
        );
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn strict_runtime_required() -> bool {
    let mut development_mode = false;
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
        if matches!(value.as_str(), "development" | "dev" | "test" | "local") {
            development_mode = true;
        }
    }
    let explicit_insecure_opt_in = std::env::var("DOKKOMPLEKT_ALLOW_INSECURE_DEV")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
    !(development_mode && explicit_insecure_opt_in)
}

fn production_ipv4_is_forbidden(value: std::net::Ipv4Addr) -> bool {
    let [first, second, third, _] = value.octets();
    value.is_unspecified()
        || value.is_loopback()
        || value.is_private()
        || value.is_link_local()
        || value.is_multicast()
        || value.is_broadcast()
        || value.is_documentation()
        || first == 0
        || (first == 100 && (second & 0b1100_0000) == 64)
        || (first == 192 && second == 0 && third == 0)
        || (first == 198 && (second & 0b1111_1110) == 18)
        || first >= 240
}

fn production_public_host_is_forbidden(host: &str, ip: Option<IpAddr>) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
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
    if RESERVED_EXACT.contains(&normalized.as_str())
        || RESERVED_SUFFIXES
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
    {
        return true;
    }
    if ip.is_none()
        && (!normalized.contains('.')
            || normalized.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            }))
    {
        return true;
    }
    ip.is_some_and(|address| match address {
        IpAddr::V4(value) => production_ipv4_is_forbidden(value),
        IpAddr::V6(value) => {
            let segments = value.segments();
            value.is_unspecified()
                || value.is_loopback()
                || value.is_unique_local()
                || value.is_unicast_link_local()
                || value.is_multicast()
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || value
                    .to_ipv4_mapped()
                    .is_some_and(production_ipv4_is_forbidden)
        }
    })
}

pub(crate) fn validate_public_base_url(
    raw_value: &str,
    production: bool,
) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(raw_value.trim())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        anyhow::bail!(
            "public base URL must be an origin without credentials, path, query or fragment"
        );
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("public base URL has no host"))?;
    let ip = host.parse::<IpAddr>().ok();
    let loopback =
        host.eq_ignore_ascii_case("localhost") || ip.is_some_and(|address| address.is_loopback());
    if production {
        if url.scheme() != "https" || loopback {
            anyhow::bail!("production public base URL must use HTTPS and a non-loopback host");
        }
        if production_public_host_is_forbidden(host, ip) {
            anyhow::bail!(
                "production public base URL must use a real public DNS name or globally routable IP"
            );
        }
    } else if !matches!(url.scheme(), "http" | "https") || !loopback {
        anyhow::bail!("development public base URL must use an HTTP(S) loopback origin");
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
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
        .split(['/', '?', '#'])
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
pub(crate) fn postgres_test_database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ if std::env::var("DOKKOMPLEKT_REQUIRE_POSTGRES_TESTS").as_deref() == Ok("1") => {
            eprintln!("DATABASE_URL is required because DOKKOMPLEKT_REQUIRE_POSTGRES_TESTS=1");
            Some("postgresql://required-postgres-test-database-is-missing".to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        database_endpoint, normalize_payment_provider, validate_database_transport,
        validate_distinct_server_secrets, validate_public_base_url, validate_yookassa_api_base_url,
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

    #[test]
    fn independent_control_secrets_cannot_be_reused() {
        assert!(validate_distinct_server_secrets([
            Some("callback-secret"),
            Some("issue-secret"),
            Some("recovery-secret"),
        ])
        .is_ok());
        assert!(validate_distinct_server_secrets([
            Some("same-secret"),
            Some("issue-secret"),
            Some("same-secret"),
        ])
        .is_err());
    }

    #[test]
    fn public_base_url_is_https_origin_in_production_and_loopback_in_development() {
        assert_eq!(
            validate_public_base_url("https://licenses.dokkomplekt.ru/", true).unwrap(),
            "https://licenses.dokkomplekt.ru"
        );
        for invalid in [
            "http://licenses.dokkomplekt.ru",
            "https://127.0.0.1",
            "https://10.0.0.9",
            "https://192.0.2.1",
            "https://198.18.0.1",
            "https://100.64.0.1",
            "https://[2001:db8::1]",
            "https://[::ffff:127.0.0.1]",
            "https://license-server",
            "https://bad_host.dokkomplekt.ru",
            "https://licenses.example.org",
            "https://licenses.invalid",
            "https://licenses.test",
            "https://licenses.local",
            "https://licenses.dokkomplekt.ru/path",
        ] {
            assert!(
                validate_public_base_url(invalid, true).is_err(),
                "{invalid}"
            );
        }
        assert!(validate_public_base_url("http://127.0.0.1:8787", false).is_ok());
        assert!(validate_public_base_url("http://192.0.2.1:8787", false).is_err());
    }

    #[test]
    fn yookassa_origin_is_pinned_in_production_and_loopback_only_in_development() {
        assert_eq!(
            validate_yookassa_api_base_url("https://api.yookassa.ru/", true).unwrap(),
            "https://api.yookassa.ru"
        );
        assert!(validate_yookassa_api_base_url("http://api.yookassa.ru", true).is_err());
        assert!(validate_yookassa_api_base_url("https://evil.example", true).is_err());
        assert!(
            validate_yookassa_api_base_url("https://api.yookassa.ru@evil.example", true).is_err()
        );
        assert!(validate_yookassa_api_base_url("https://api.yookassa.ru/v3", true).is_err());
        assert!(validate_yookassa_api_base_url("http://127.0.0.1:18080", false).is_ok());
        assert!(validate_yookassa_api_base_url("http://192.0.2.10:18080", false).is_err());
    }
}
