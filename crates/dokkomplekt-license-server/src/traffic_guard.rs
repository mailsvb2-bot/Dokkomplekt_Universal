use axum::{
    body::Body,
    extract::ConnectInfo,
    http::Request,
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateLimitScope {
    OrderCreation,
    OrderAccess,
    ProviderCallback,
    LicenseIssue,
}

#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

#[derive(Debug, Clone)]
pub struct TrafficGuard {
    windows: Arc<Mutex<HashMap<(IpAddr, RateLimitScope), FixedWindow>>>,
    max_entries: usize,
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
            windows: Arc::new(Mutex::new(HashMap::new())),
            max_entries: max_entries.max(1),
        }
    }

    pub fn check(
        &self,
        ip: IpAddr,
        scope: RateLimitScope,
        limit: u32,
        window: Duration,
    ) -> bool {
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
        let Ok(mut windows) = self.windows.lock() else {
            return false;
        };
        windows.retain(|_, value| value.expires_at > now);
        let key = (ip, scope);
        if let Some(value) = windows.get_mut(&key) {
            if value.count >= limit {
                return false;
            }
            value.count = value.count.saturating_add(1);
            return true;
        }
        if windows.len() >= self.max_entries {
            return false;
        }
        windows.insert(
            key,
            FixedWindow {
                expires_at: now + window,
                count: 1,
            },
        );
        true
    }
}

pub async fn attach_client_ip(mut request: Request<Body>, next: Next) -> Response {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|value| value.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn scopes_and_addresses_have_independent_budgets() {
        let guard = TrafficGuard::new(8);
        let first: IpAddr = "192.0.2.10".parse().unwrap();
        let second: IpAddr = "192.0.2.11".parse().unwrap();
        let now = Instant::now();
        assert!(guard.check_at(first, RateLimitScope::OrderAccess, 1, Duration::from_secs(60), now));
        assert!(!guard.check_at(first, RateLimitScope::OrderAccess, 1, Duration::from_secs(60), now));
        assert!(guard.check_at(first, RateLimitScope::OrderCreation, 1, Duration::from_secs(60), now));
        assert!(guard.check_at(second, RateLimitScope::OrderAccess, 1, Duration::from_secs(60), now));
    }
}
