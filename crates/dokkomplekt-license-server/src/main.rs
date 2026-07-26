mod config;
mod http;
mod issuer;
#[path = "http/license_issue.rs"]
mod license_issue;
mod memory_store;
mod provider_manual;
mod provider_sbp;
mod provider_yookassa;
mod providers;
mod state;
mod storage;

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod http_integration_tests;

use anyhow::Context;
use axum::Router;
use config::ServerConfig;
use state::AppState;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Reqwest is built without a default crypto provider so the workspace can
/// explicitly use the Rust 1.85-compatible ring backend instead of AWS-LC.
pub(crate) fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn build_app(state: AppState) -> Router {
    Router::new()
        .merge(http::health::router())
        .merge(http::orders::router())
        .merge(http::activations::router())
        .merge(license_issue::router())
        .merge(http::webhooks::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = ServerConfig::from_env()?;
    let state =
        AppState::try_new(config.clone()).context("failed to initialize license server state")?;
    let app = build_app(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.bind_addr))?;
    tracing::info!(
        "dokkomplekt service listening on {}",
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}
