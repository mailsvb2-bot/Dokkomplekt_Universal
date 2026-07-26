use crate::provider_yookassa::YooKassaProvider;
use crate::providers::{PaymentProvider as PaymentProviderApi, ProviderPaymentStatus};
use crate::state::AppState;
use crate::storage::{
    PaymentEventRecord, PaymentEventStatus, PaymentEventWriteOutcome, PaymentProvider, StoreError,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ProviderCallbackRequest {
    pub order_id: Uuid,
    pub provider_event_id: String,
    pub provider_payment_id: Option<String>,
    pub provider: Option<String>,
    pub status: String,
    pub amount_rub: u64,
    pub callback_secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderCallbackResponse {
    pub accepted: bool,
    pub duplicate: bool,
    pub order_id: Uuid,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/provider/callback", post(provider_callback))
        .route("/api/provider/yookassa/callback", post(yookassa_callback))
}

async fn provider_callback(
    State(state): State<AppState>,
    Json(event): Json<ProviderCallbackRequest>,
) -> Result<Json<ProviderCallbackResponse>, StatusCode> {
    let event_id = event.provider_event_id.trim();
    if event_id.is_empty() || event.amount_rub == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !callback_secret_matches(
        state.config.provider_callback_secret.as_deref(),
        event.callback_secret.as_deref(),
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let provider = normalize_callback_provider(event.provider.as_deref().unwrap_or("manual"))
        .ok_or(StatusCode::BAD_REQUEST)?;
    // Provider-native payloads use dedicated endpoints. The generic endpoint is
    // intentionally limited to manual back-office confirmation.
    if !matches!(&provider, PaymentProvider::Manual) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let status = normalize_payment_status(&event.status).ok_or(StatusCode::BAD_REQUEST)?;
    record_verified_event(
        &state,
        event.order_id,
        provider,
        event_id.to_string(),
        event.provider_payment_id,
        status,
        event.amount_rub,
    )
    .await
}

async fn yookassa_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ProviderCallbackResponse>, StatusCode> {
    let supplied_secret = headers
        .get("x-dokkomplekt-callback-secret")
        .and_then(|value| value.to_str().ok());
    if !callback_secret_matches(
        state.config.provider_callback_secret.as_deref(),
        supplied_secret,
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let provider = YooKassaProvider {
        public_base_url: state.config.public_base_url.clone(),
        api_base_url: state.config.yookassa_api_base_url.clone(),
        shop_id: state.config.yookassa_shop_id.clone().unwrap_or_default(),
        secret_key: state.config.yookassa_secret_key.clone().unwrap_or_default(),
    };
    let event = provider
        .parse_callback(&body)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let status = match event.status {
        ProviderPaymentStatus::Pending => PaymentEventStatus::Pending,
        ProviderPaymentStatus::Succeeded => PaymentEventStatus::Succeeded,
        ProviderPaymentStatus::Cancelled => PaymentEventStatus::Cancelled,
        ProviderPaymentStatus::Rejected => PaymentEventStatus::Rejected,
    };
    record_verified_event(
        &state,
        event.order_id,
        PaymentProvider::YooKassa,
        event.provider_event_id,
        event.provider_payment_id,
        status,
        event.amount_rub,
    )
    .await
}

async fn record_verified_event(
    state: &AppState,
    order_id: Uuid,
    provider: PaymentProvider,
    provider_event_id: String,
    provider_payment_id: Option<String>,
    status: PaymentEventStatus,
    amount_rub: u64,
) -> Result<Json<ProviderCallbackResponse>, StatusCode> {
    if provider_event_id.trim().is_empty() || amount_rub == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let order = state
        .store
        .get_order_async(order_id)
        .await
        .map_err(store_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if order.amount_rub != amount_rub {
        return Err(StatusCode::BAD_REQUEST);
    }
    let record = PaymentEventRecord {
        id: Uuid::new_v4(),
        order_id,
        provider,
        provider_event_id,
        provider_payment_id,
        status,
        amount_rub,
        received_at: OffsetDateTime::now_utc(),
    };
    let outcome = state
        .store
        .record_payment_event_for_order_async(record)
        .await
        .map_err(store_error_status)?;
    Ok(Json(ProviderCallbackResponse {
        accepted: true,
        duplicate: matches!(outcome, PaymentEventWriteOutcome::Duplicate),
        order_id,
    }))
}

pub fn callback_secret_matches(
    configured_secret: Option<&str>,
    supplied_secret: Option<&str>,
) -> bool {
    let Some(expected) = configured_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        // Fail closed in every environment. A missing server secret must never
        // turn the public callback endpoint into an unauthenticated payment API.
        return false;
    };
    supplied_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|actual| constant_time_eq(actual.as_bytes(), expected.as_bytes()))
}
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn store_error_status(error: StoreError) -> StatusCode {
    match error {
        StoreError::Conflict => StatusCode::CONFLICT,
        StoreError::Invalid(_) => StatusCode::BAD_REQUEST,
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::Poisoned => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn normalize_payment_status(value: &str) -> Option<PaymentEventStatus> {
    match value.trim().to_ascii_lowercase().as_str() {
        "succeeded" => Some(PaymentEventStatus::Succeeded),
        "pending" => Some(PaymentEventStatus::Pending),
        "cancelled" | "canceled" => Some(PaymentEventStatus::Cancelled),
        "rejected" => Some(PaymentEventStatus::Rejected),
        _ => None,
    }
}

pub fn normalize_callback_provider(value: &str) -> Option<PaymentProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "manual" => Some(PaymentProvider::Manual),
        "yookassa" => Some(PaymentProvider::YooKassa),
        "sbp" => Some(PaymentProvider::Sbp),
        "bank_invoice" => Some(PaymentProvider::BankInvoice),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_secret_fails_closed_when_server_secret_is_missing() {
        assert!(!callback_secret_matches(None, None));
        assert!(!callback_secret_matches(None, Some("attacker")));
        assert!(!callback_secret_matches(Some("secret"), None));
        assert!(callback_secret_matches(Some("secret"), Some("secret")));
        assert!(!callback_secret_matches(Some("secret"), Some("Secret")));
    }

    #[test]
    fn payment_status_values_are_normalized() {
        assert!(matches!(
            normalize_payment_status("succeeded"),
            Some(PaymentEventStatus::Succeeded)
        ));
        assert!(matches!(
            normalize_payment_status(" pending "),
            Some(PaymentEventStatus::Pending)
        ));
        assert!(matches!(
            normalize_payment_status("canceled"),
            Some(PaymentEventStatus::Cancelled)
        ));
        assert!(matches!(
            normalize_payment_status("cancelled"),
            Some(PaymentEventStatus::Cancelled)
        ));
        assert!(matches!(
            normalize_payment_status("rejected"),
            Some(PaymentEventStatus::Rejected)
        ));
    }

    #[test]
    fn unknown_payment_status_is_rejected() {
        assert!(normalize_payment_status("unexpected-state").is_none());
    }

    #[test]
    fn callback_provider_values_are_normalized() {
        assert!(matches!(
            normalize_callback_provider(" manual "),
            Some(PaymentProvider::Manual)
        ));
        assert!(matches!(
            normalize_callback_provider("YooKassa"),
            Some(PaymentProvider::YooKassa)
        ));
        assert!(matches!(
            normalize_callback_provider("SBP"),
            Some(PaymentProvider::Sbp)
        ));
        assert!(matches!(
            normalize_callback_provider("bank_invoice"),
            Some(PaymentProvider::BankInvoice)
        ));
    }

    #[test]
    fn unknown_callback_provider_is_rejected() {
        assert!(normalize_callback_provider("unknown-pay").is_none());
    }
}
