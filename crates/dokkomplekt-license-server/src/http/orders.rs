use crate::provider_manual::ManualProvider;
use crate::provider_sbp::SbpProvider;
use crate::provider_yookassa::YooKassaProvider;
use crate::providers::{CreatePaymentRequest, CreatePaymentResponse, PaymentProvider};
use crate::state::{AppState, OrderRecord, OrderStatus};
use crate::storage::StoreError;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
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
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/orders", post(create_order))
}

async fn create_order(
    State(state): State<AppState>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, StatusCode> {
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
        .map(str::to_string)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let order_id = Uuid::new_v4();
    let record = OrderRecord {
        id: order_id,
        plan: plan.to_string(),
        amount_rub,
        status: OrderStatus::WaitingPayment,
        machine_hash: Some(machine_hash),
        created_at: OffsetDateTime::now_utc(),
    };
    state
        .store
        .create_order_async(record.clone())
        .await
        .map_err(store_error_status)?;
    let provider = state.config.payment_provider.clone();
    let payment = create_provider_payment(&state, &record)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Json(CreateOrderResponse {
        order_id,
        status: record.status,
        provider,
        amount_rub,
        payment_url: payment.confirmation_url,
        qr_url: payment.qr_url.unwrap_or_default(),
    }))
}

async fn create_provider_payment(
    state: &AppState,
    order: &OrderRecord,
) -> Result<CreatePaymentResponse, String> {
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
        .map_err(|error| error.to_string()),
        "sbp" => SbpProvider {
            public_base_url: state.config.public_base_url.clone(),
        }
        .create_payment(request)
        .map_err(|error| error.to_string()),
        "yookassa" => {
            let provider = YooKassaProvider {
                public_base_url: state.config.public_base_url.clone(),
                api_base_url: state.config.yookassa_api_base_url.clone(),
                shop_id: state.config.yookassa_shop_id.clone().unwrap_or_default(),
                secret_key: state.config.yookassa_secret_key.clone().unwrap_or_default(),
            };
            tokio::task::spawn_blocking(move || provider.create_payment(request))
                .await
                .map_err(|error| format!("YooKassa task failed: {error}"))?
                .map_err(|error| error.to_string())
        }
        "bank_invoice" => Ok(CreatePaymentResponse {
            provider: crate::providers::ProviderKind::BankInvoice,
            provider_payment_id: format!("invoice-{}", order.id),
            confirmation_url: payment_url_for(
                &state.config.public_base_url,
                "bank_invoice",
                order.id,
            ),
            qr_url: None,
        }),
        other => Err(format!("unsupported payment provider: {other}")),
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

pub fn payment_url_for(base_url: &str, provider: &str, order_id: Uuid) -> String {
    format!(
        "{}/pay/{}/{}",
        base_url.trim_end_matches('/'),
        provider,
        order_id
    )
}

#[cfg(test)]
pub fn qr_url_for(base_url: &str, provider: &str, order_id: Uuid) -> String {
    match provider {
        "sbp" => format!(
            "{}/api/orders/{}/qr",
            base_url.trim_end_matches('/'),
            order_id
        ),
        _ => "".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yookassa_payment_url_uses_provider_namespace() {
        let order_id = Uuid::nil();
        assert_eq!(
            payment_url_for("https://lic.example/", "yookassa", order_id),
            "https://lic.example/pay/yookassa/00000000-0000-0000-0000-000000000000",
        );
    }

    #[test]
    fn sbp_gets_qr_url_and_manual_does_not() {
        let order_id = Uuid::nil();
        assert_eq!(
            qr_url_for("https://lic.example", "sbp", order_id),
            "https://lic.example/api/orders/00000000-0000-0000-0000-000000000000/qr",
        );
        assert_eq!(qr_url_for("https://lic.example", "manual", order_id), "");
    }

    #[test]
    fn order_tariffs_are_server_side_only() {
        assert_eq!(normalize_order_plan(" Doctor_Pro "), Some("doctor_pro"));
        assert_eq!(tariff_amount_rub("doctor_pro"), Some(3_900));
        assert_eq!(tariff_amount_rub("clinic"), Some(49_000));
        assert!(normalize_order_plan("trial").is_none());
        assert!(normalize_order_plan("unknown").is_none());
    }
}
