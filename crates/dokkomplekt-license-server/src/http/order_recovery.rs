use crate::order_access::{bearer_secret_matches, generate_order_access_token};
use crate::state::{AppState, OrderStatus};
use crate::storage::StoreError;
use crate::traffic_guard::{ClientIp, RateLimitScope};
use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RecoverOrderAccessRequest {
    pub machine_hash: String,
    #[serde(default)]
    pub bind_missing_machine: bool,
}

#[derive(Debug, Serialize)]
pub struct RecoverOrderAccessResponse {
    pub order_id: Uuid,
    pub status: OrderStatus,
    pub machine_hash: String,
    pub order_access_token: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/api/admin/orders/:order_id/recover-access",
        post(recover_order_access),
    )
}

async fn recover_order_access(
    Extension(client_ip): Extension<ClientIp>,
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RecoverOrderAccessRequest>,
) -> Result<Json<RecoverOrderAccessResponse>, StatusCode> {
    if !state.traffic_guard.check(
        client_ip.0,
        RateLimitScope::OrderRecovery,
        state.config.order_recovery_limit_per_minute,
        Duration::from_secs(60),
    ) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    if !bearer_secret_matches(&headers, state.config.order_recovery_secret.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let machine_hash = request.machine_hash.trim();
    if machine_hash.is_empty()
        || machine_hash.len() > 256
        || machine_hash.chars().any(char::is_control)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (order_access_token, access_token_hash) = generate_order_access_token();
    let recovered = state
        .store
        .recover_legacy_order_access_async(
            order_id,
            machine_hash.to_string(),
            access_token_hash,
            request.bind_missing_machine,
        )
        .await
        .map_err(store_error_status)?;
    Ok(Json(RecoverOrderAccessResponse {
        order_id,
        status: recovered.status,
        machine_hash: recovered
            .machine_hash
            .unwrap_or_else(|| machine_hash.to_string()),
        order_access_token,
    }))
}

fn store_error_status(error: StoreError) -> StatusCode {
    match error {
        StoreError::Conflict => StatusCode::CONFLICT,
        StoreError::Invalid(_) => StatusCode::BAD_REQUEST,
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::Poisoned => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
