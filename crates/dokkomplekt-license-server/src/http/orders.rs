use crate::order_access::{authorize_order, generate_order_access_token};
use crate::provider_manual::ManualProvider;
use crate::provider_sbp::SbpProvider;
use crate::provider_yookassa::YooKassaProvider;
use crate::providers::{CreatePaymentRequest, CreatePaymentResponse, PaymentProvider};
use crate::state::{AppState, OrderRecord, OrderStatus};
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
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub plan: String,
    pub amount_rub: Option<u64>,
    pub machine_hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateOrderResponse {
    pub order_id: Uuid,
    pub status: OrderStatus,
    pub provider: String,
    pub amount_rub: u64,
    pub payment_url: String,
    pub qr_url: String,
    pub order_access_token: String,
    pub payment_state: String,
}

#[derive(Debug, Serialize)]
pub struct RetryPaymentResponse {
    pub order_id: Uuid,
    pub provider: String,
    pub payment_url: String,
    pub qr_url: String,
    pub payment_state: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/orders", post(create_order))
        .route("/api/orders/:order_id/payment", post(retry_payment))
}

async fn create_order(
    Extension(client_ip): Extension<ClientIp>,
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, StatusCode> {
    if !state.traffic_guard.check(
        client_ip.0,
        RateLimitScope::OrderCreation,
        state.config.order_create_limit_per_hour,
        Duration::from_secs(60 * 60),
    ) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let plan = normalize_order_plan(&request.plan).ok_or(StatusCode::BAD_REQUEST)?;
    let amount_rub = tariff_amount_rub(plan).ok_or(StatusCode::BAD_REQUEST)?;
    if matches!(request.amount_rub, Some(client_amount) if client_amount != amount_rub) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let machine_hash = request
        .machine_hash
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.len() <= 256 && !value.chars().any(char::is_control))
        .map(str::to_string)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let order_id = Uuid::new_v4();
    let (order_access_token, access_token_hash) = generate_order_access_token();
    let record = OrderRecord {
        id: order_id,
        plan: plan.to_string(),
        amount_rub,
        status: OrderStatus::WaitingPayment,
        machine_hash: Some(machine_hash),
        access_token_hash: Some(access_token_hash),
        created_at: OffsetDateTime::now_utc(),
    };
    state
        .store
        .create_order_async(record.clone())
        .await
        .map_err(store_error_status)?;
    let provider = state.config.payment_provider.clone();
    let (payment_url, qr_url, payment_state) = match create_provider_payment(&state, &record).await
    {
        Ok(payment) => (
            payment.confirmation_url,
            payment.qr_url.unwrap_or_default(),
            "ready".to_string(),
        ),
        Err(error) => {
            tracing::warn!(
                order_id = %order_id,
                error = %error.message(),
                "payment provider call failed; order remains recoverable through authenticated retry"
            );
            (String::new(), String::new(), "retry_required".to_string())
        }
    };
    Ok(Json(CreateOrderResponse {
        order_id,
        status: record.status,
        provider,
        amount_rub,
        payment_url,
        qr_url,
        order_access_token,
        payment_state,
    }))
}

async fn retry_payment(
    Extension(client_ip): Extension<ClientIp>,
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<RetryPaymentResponse>, StatusCode> {
    if !state.traffic_guard.check(
        client_ip.0,
        RateLimitScope::OrderAccess,
        state.config.order_access_limit_per_minute,
        Duration::from_secs(60),
    ) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let order = state
        .store
        .get_order_async(order_id)
        .await
        .map_err(store_error_status)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !authorize_order(&headers, order.access_token_hash.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !matches!(
        order.status,
        OrderStatus::WaitingPayment | OrderStatus::Draft
    ) {
        return Err(StatusCode::CONFLICT);
    }
    let payment = create_provider_payment(&state, &order)
        .await
        .map_err(|error| match error {
            ProviderCallError::Busy => StatusCode::TOO_MANY_REQUESTS,
            ProviderCallError::Provider(message) => {
                tracing::warn!(order_id = %order_id, error = %message, "payment retry failed");
                StatusCode::BAD_GATEWAY
            }
        })?;
    Ok(Json(RetryPaymentResponse {
        order_id,
        provider: state.config.payment_provider.clone(),
        payment_url: payment.confirmation_url,
        qr_url: payment.qr_url.unwrap_or_default(),
        payment_state: "ready".to_string(),
    }))
}

#[derive(Debug)]
enum ProviderCallError {
    Busy,
    Provider(String),
}

impl ProviderCallError {
    fn message(&self) -> &str {
        match self {
            Self::Busy => "provider concurrency limit reached",
            Self::Provider(message) => message,
        }
    }
}

async fn create_provider_payment(
    state: &AppState,
    order: &OrderRecord,
) -> Result<CreatePaymentResponse, ProviderCallError> {
    let request = CreatePaymentRequest {
        order_id: order.id,
        amount_rub: order.amount_rub,
        description: format!("Dokkomplekt Universal — {}", order.plan),
        return_url: Some(format!(
            "{}/payment/return/{}",
            state.config.public_base_url.trim_end_matches('/'),
            order.id
        )),
    };
    match state.config.payment_provider.as_str() {
        "manual" => ManualProvider {
            public_base_url: state.config.public_base_url.clone(),
        }
        .create_payment(request)
        .map_err(|error| ProviderCallError::Provider(error.to_string())),
        "sbp" => {
            let permit = state
                .provider_gate
                .clone()
                .try_acquire_owned()
                .map_err(|_| ProviderCallError::Busy)?;
            let provider = SbpProvider {
                public_base_url: state.config.public_base_url.clone(),
                api_base_url: state.config.yookassa_api_base_url.clone(),
                shop_id: state.config.yookassa_shop_id.clone().unwrap_or_default(),
                secret_key: state.config.yookassa_secret_key.clone().unwrap_or_default(),
            };
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                provider.create_payment(request)
            })
            .await
            .map_err(|error| ProviderCallError::Provider(format!("SBP task failed: {error}")))?
            .map_err(|error| ProviderCallError::Provider(error.to_string()))
        }
        "yookassa" => {
            let permit = state
                .provider_gate
                .clone()
                .try_acquire_owned()
                .map_err(|_| ProviderCallError::Busy)?;
            let provider = YooKassaProvider {
                public_base_url: state.config.public_base_url.clone(),
                api_base_url: state.config.yookassa_api_base_url.clone(),
                shop_id: state.config.yookassa_shop_id.clone().unwrap_or_default(),
                secret_key: state.config.yookassa_secret_key.clone().unwrap_or_default(),
            };
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                provider.create_payment(request)
            })
            .await
            .map_err(|error| ProviderCallError::Provider(format!("YooKassa task failed: {error}")))?
            .map_err(|error| ProviderCallError::Provider(error.to_string()))
        }
        "bank_invoice" => Err(ProviderCallError::Provider(
            "bank invoice provider is not implemented and cannot create payments".to_string(),
        )),
        other => Err(ProviderCallError::Provider(format!(
            "unsupported payment provider: {other}"
        ))),
    }
}

fn store_error_status(error: StoreError) -> StatusCode {
    match error {
        StoreError::Conflict => StatusCode::CONFLICT,
        StoreError::Invalid(_) => StatusCode::BAD_REQUEST,
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::Poisoned => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn normalize_order_plan(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "doctor_start" => Some("doctor_start"),
        "doctor_pro" => Some("doctor_pro"),
        "department" => Some("department"),
        "clinic" => Some("clinic"),
        "enterprise" => Some("enterprise"),
        "trial" => None,
        _ => None,
    }
}

pub fn tariff_amount_rub(plan: &str) -> Option<u64> {
    match plan {
        "doctor_start" => Some(1_490),
        "doctor_pro" => Some(3_900),
        "department" => Some(14_900),
        "clinic" => Some(49_000),
        "enterprise" => Some(900_000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_tariffs_are_server_side_only() {
        assert_eq!(normalize_order_plan(" Doctor_Pro "), Some("doctor_pro"));
        assert_eq!(tariff_amount_rub("doctor_pro"), Some(3_900));
        assert_eq!(tariff_amount_rub("clinic"), Some(49_000));
        assert!(normalize_order_plan("trial").is_none());
        assert!(normalize_order_plan("unknown").is_none());
    }
}
