use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_FORWARDED_HEADER_BYTES: usize = 2_048;
const MAX_FORWARDED_HOPS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateLimitScope {
    OrderCreation,
    OrderAccess,
    OrderRecovery,
    ProviderCallback,
    LicenseIssue,
}

#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

#[derive(Debug, Clone, Default)]
pub struct TrustedProxyConfig {
    networks: Arc<[TrustedProxyNetwork]>,
    require_forwarded_for_for_api: bool,
}

impl TrustedProxyConfig {
    pub fn parse(
        raw_cidrs: Option<&str>,
        require_forwarded_for_for_api: bool,
    ) -> Result<Self, String> {
        let mut networks = Vec::new();
        if let Some(raw_cidrs) = raw_cidrs.map(str::trim).filter(|value| !value.is_empty()) {
            for raw_network in raw_cidrs.split(',') {
                networks.push(TrustedProxyNetwork::parse(raw_network.trim())?);
            }
        }
        let has_networks = !networks.is_empty();
        Ok(Self {
            networks: networks.into(),
            require_forwarded_for_for_api: require_forwarded_for_for_api && has_networks,
        })
    }

    pub fn is_trusted(&self, address: IpAddr) -> bool {
        self.networks
            .iter()
            .any(|network| network.contains(address))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrustedProxyNetwork {
    network: IpAddr,
    prefix: u8,
}

impl TrustedProxyNetwork {
    fn parse(raw: &str) -> Result<Self, String> {
        if raw.is_empty() {
            return Err("trusted proxy CIDR is empty".to_string());
        }
        let (address_raw, prefix_raw) = raw
            .split_once('/')
            .map_or((raw, None), |(address, prefix)| (address, Some(prefix)));
        let network = address_raw
            .parse::<IpAddr>()
            .map_err(|_| format!("invalid trusted proxy address: {address_raw}"))?;
        let maximum = if network.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix_raw {
            Some(value) => value
                .parse::<u8>()
                .map_err(|_| format!("invalid trusted proxy prefix: {value}"))?,
            None => maximum,
        };
        if prefix > maximum {
            return Err(format!("trusted proxy prefix {prefix} exceeds {maximum}"));
        }
        Ok(Self { network, prefix })
    }

    fn contains(&self, candidate: IpAddr) -> bool {
        match (self.network, candidate) {
            (IpAddr::V4(network), IpAddr::V4(candidate)) => {
                let prefix = u32::from(self.prefix);
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                (u32::from(network) & mask) == (u32::from(candidate) & mask)
            }
            (IpAddr::V6(network), IpAddr::V6(candidate)) => {
                let prefix = u32::from(self.prefix);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                (u128::from(network) & mask) == (u128::from(candidate) & mask)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrafficGuard {
    state: Arc<Mutex<TrafficGuardState>>,
    max_entries: usize,
}

#[derive(Debug)]
struct TrafficGuardState {
    windows: HashMap<(IpAddr, RateLimitScope), FixedWindow>,
    overflow: HashMap<RateLimitScope, FixedWindow>,
    last_cleanup: Instant,
}

#[derive(Debug, Clone, Copy)]
struct FixedWindow {
    expires_at: Instant,
    count: u32,
}

impl Default for TrafficGuard {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl TrafficGuard {
    pub fn new(max_entries: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(TrafficGuardState {
                windows: HashMap::new(),
                overflow: HashMap::new(),
                last_cleanup: Instant::now(),
            })),
            max_entries: max_entries.max(1),
        }
    }

    pub fn check(&self, ip: IpAddr, scope: RateLimitScope, limit: u32, window: Duration) -> bool {
        self.check_at(ip, scope, limit, window, Instant::now())
    }

    fn check_at(
        &self,
        ip: IpAddr,
        scope: RateLimitScope,
        limit: u32,
        window: Duration,
        now: Instant,
    ) -> bool {
        if limit == 0 || window.is_zero() {
            return false;
        }
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        let cleanup_interval = window.min(Duration::from_secs(30));
        if state.windows.len() >= self.max_entries
            || now.saturating_duration_since(state.last_cleanup) >= cleanup_interval
        {
            state.windows.retain(|_, value| value.expires_at > now);
            state.overflow.retain(|_, value| value.expires_at > now);
            state.last_cleanup = now;
        }
        let key = (ip, scope);
        if let Some(value) = state.windows.get_mut(&key) {
            return consume_fixed_window(value, limit, window, now);
        }
        if state.windows.len() < self.max_entries {
            state.windows.insert(
                key,
                FixedWindow {
                    expires_at: now + window,
                    count: 1,
                },
            );
            return true;
        }
        // A cardinality attack must not turn the fixed-size table into a total
        // outage for every previously unseen client. New addresses share one
        // bounded overflow budget per endpoint scope until regular entries expire.
        match state.overflow.get_mut(&scope) {
            Some(value) => consume_fixed_window(value, limit, window, now),
            None => {
                state.overflow.insert(
                    scope,
                    FixedWindow {
                        expires_at: now + window,
                        count: 1,
                    },
                );
                true
            }
        }
    }
}

fn consume_fixed_window(
    value: &mut FixedWindow,
    limit: u32,
    window: Duration,
    now: Instant,
) -> bool {
    if value.expires_at <= now {
        *value = FixedWindow {
            expires_at: now + window,
            count: 1,
        };
        return true;
    }
    if value.count >= limit {
        return false;
    }
    value.count = value.count.saturating_add(1);
    true
}

pub async fn attach_client_ip(
    State(trusted_proxies): State<TrustedProxyConfig>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let peer_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|value| value.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let require_forwarded =
        trusted_proxies.require_forwarded_for_for_api && request.uri().path().starts_with("/api/");
    let client_ip = resolve_client_ip(
        peer_ip,
        request.headers(),
        &trusted_proxies,
        require_forwarded,
    )
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    request.extensions_mut().insert(ClientIp(client_ip));
    Ok(next.run(request).await)
}

fn resolve_client_ip(
    peer_ip: IpAddr,
    headers: &HeaderMap,
    trusted_proxies: &TrustedProxyConfig,
    require_forwarded: bool,
) -> Result<IpAddr, String> {
    if !trusted_proxies.is_trusted(peer_ip) {
        return Ok(peer_ip);
    }

    let mut forwarded = Vec::new();
    let mut total_bytes = 0_usize;
    for value in headers.get_all("x-forwarded-for") {
        let value = value
            .to_str()
            .map_err(|_| "X-Forwarded-For is not valid ASCII".to_string())?;
        total_bytes = total_bytes.saturating_add(value.len());
        if total_bytes > MAX_FORWARDED_HEADER_BYTES {
            return Err("X-Forwarded-For is too large".to_string());
        }
        for item in value.split(',') {
            if forwarded.len() >= MAX_FORWARDED_HOPS {
                return Err("X-Forwarded-For has too many hops".to_string());
            }
            forwarded.push(parse_forwarded_ip(item)?);
        }
    }
    if forwarded.is_empty() {
        if require_forwarded {
            return Err("trusted proxy request is missing X-Forwarded-For".to_string());
        }
        return Ok(peer_ip);
    }

    forwarded.push(peer_ip);
    let mut index = forwarded.len() - 1;
    while index > 0 && trusted_proxies.is_trusted(forwarded[index]) {
        index -= 1;
    }
    Ok(forwarded[index])
}

fn parse_forwarded_ip(raw: &str) -> Result<IpAddr, String> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err("invalid X-Forwarded-For address".to_string());
    }
    let value = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value);
    value
        .parse::<IpAddr>()
        .map_err(|_| format!("invalid X-Forwarded-For address: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn fixed_window_rejects_excess_and_resets_after_expiry() {
        let guard = TrafficGuard::new(8);
        let ip = "192.0.2.10".parse().unwrap();
        let start = Instant::now();
        assert!(guard.check_at(
            ip,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            start
        ));
        assert!(guard.check_at(
            ip,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            start
        ));
        assert!(!guard.check_at(
            ip,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            start
        ));
        assert!(guard.check_at(
            ip,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            start + Duration::from_secs(61)
        ));
    }

    #[test]
    fn cardinality_overflow_uses_a_bounded_shared_budget_instead_of_total_outage() {
        let guard = TrafficGuard::new(1);
        let now = Instant::now();
        let first: IpAddr = "192.0.2.1".parse().unwrap();
        let second: IpAddr = "192.0.2.2".parse().unwrap();
        let third: IpAddr = "192.0.2.3".parse().unwrap();
        assert!(guard.check_at(
            first,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            now
        ));
        assert!(guard.check_at(
            second,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            now
        ));
        assert!(guard.check_at(
            third,
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            now
        ));
        assert!(!guard.check_at(
            "192.0.2.4".parse().unwrap(),
            RateLimitScope::OrderCreation,
            2,
            Duration::from_secs(60),
            now
        ));
    }

    #[test]
    fn scopes_and_addresses_have_independent_budgets() {
        let guard = TrafficGuard::new(8);
        let first: IpAddr = "192.0.2.10".parse().unwrap();
        let second: IpAddr = "192.0.2.11".parse().unwrap();
        let now = Instant::now();
        assert!(guard.check_at(
            first,
            RateLimitScope::OrderAccess,
            1,
            Duration::from_secs(60),
            now
        ));
        assert!(!guard.check_at(
            first,
            RateLimitScope::OrderAccess,
            1,
            Duration::from_secs(60),
            now
        ));
        assert!(guard.check_at(
            first,
            RateLimitScope::OrderCreation,
            1,
            Duration::from_secs(60),
            now
        ));
        assert!(guard.check_at(
            second,
            RateLimitScope::OrderAccess,
            1,
            Duration::from_secs(60),
            now
        ));
    }

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_address() {
        let proxies = TrustedProxyConfig::parse(Some("10.0.0.0/8"), true).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.7"));
        let peer: IpAddr = "198.51.100.10".parse().unwrap();
        assert_eq!(
            resolve_client_ip(peer, &headers, &proxies, true).unwrap(),
            peer
        );
    }

    #[test]
    fn trusted_proxy_chain_selects_nearest_untrusted_client() {
        let proxies = TrustedProxyConfig::parse(Some("10.0.0.0/8,192.168.0.0/16"), true).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.99, 203.0.113.44, 192.168.1.9"),
        );
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        assert_eq!(
            resolve_client_ip(peer, &headers, &proxies, true).unwrap(),
            "203.0.113.44".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn trusted_proxy_api_request_requires_well_formed_forwarded_header() {
        let proxies = TrustedProxyConfig::parse(Some("127.0.0.1/32"), true).unwrap();
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(resolve_client_ip(peer, &HeaderMap::new(), &proxies, true).is_err());
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("attacker:1234"));
        assert!(resolve_client_ip(peer, &headers, &proxies, true).is_err());
    }

    #[test]
    fn cidr_parser_supports_ipv4_and_ipv6_boundaries() {
        let proxies = TrustedProxyConfig::parse(Some("10.0.0.0/24,2001:db8::/32"), true).unwrap();
        assert!(proxies.is_trusted("10.0.0.255".parse().unwrap()));
        assert!(!proxies.is_trusted("10.0.1.1".parse().unwrap()));
        assert!(proxies.is_trusted("2001:db8::1".parse().unwrap()));
        assert!(!proxies.is_trusted("2001:db9::1".parse().unwrap()));
        assert!(TrustedProxyConfig::parse(Some("10.0.0.1/33"), true).is_err());
    }
}
