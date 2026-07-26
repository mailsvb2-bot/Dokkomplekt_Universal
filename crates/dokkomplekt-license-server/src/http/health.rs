use crate::state::AppState;
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    storage_mode: String,
    storage_backend: &'static str,
    database_configured: bool,
    database_connected: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

async fn healthz(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(health_response(&state).await)
}

async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let response = health_response(&state).await;
    let ready = response.database_configured && response.database_connected;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(response))
}

async fn health_response(state: &AppState) -> HealthResponse {
    let database_configured = state
        .config
        .database_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let database_connected = state.store.database_ready_async().await;
    HealthResponse {
        status: "ok",
        service: "dokkomplekt-license-server",
        storage_mode: state.config.storage_mode.clone(),
        storage_backend: state.store.backend_name(),
        database_configured,
        database_connected,
    }
}
