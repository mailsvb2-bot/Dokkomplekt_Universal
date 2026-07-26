use crate::state::{ActivationRecord, AppState, OrderStatus};
use crate::storage::StoreError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use dokkomplekt_license_core::{max_machines_for_plan, PlanId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ActivateMachineRequest {
    pub machine_hash: String,
}

#[derive(Debug, Serialize)]
pub struct ActivationResponse {
    pub activation_id: Uuid,
    pub order_id: Uuid,
    pub status: OrderStatus,
    pub message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/orders/:order_id/status", get(order_status))
        .route(
            "/api/orders/:order_id/activate-machine",
            post(activate_machine),
        )
}

async fn order_status(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
) -> Result<Json<ActivationResponse>, StatusCode> {
    let order = state
        .store
        .get_order_async(order_id)
        .await
        .map_err(store_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ActivationResponse {
        activation_id: Uuid::nil(),
        order_id,
        status: order.status,
        message: "order status".to_string(),
    }))
}

async fn activate_machine(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    Json(request): Json<ActivateMachineRequest>,
) -> Result<Json<ActivationResponse>, StatusCode> {
    let machine_hash = request.machine_hash.trim();
    if machine_hash.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let order = state
        .store
        .get_order_async(order_id)
        .await
        .map_err(store_error_status)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !matches!(order.status, OrderStatus::Paid | OrderStatus::LicenseIssued) {
        return Err(StatusCode::CONFLICT);
    }
    let plan = parse_plan(&order.plan).ok_or(StatusCode::BAD_REQUEST)?;
    let activation_id = Uuid::new_v4();
    let record = ActivationRecord {
        id: activation_id,
        order_id,
        machine_hash: machine_hash.to_string(),
        created_at: OffsetDateTime::now_utc(),
    };
    let outcome = state
        .store
        .create_activation_for_order_async(record, max_machines_for_plan(&plan))
        .await
        .map_err(store_error_status)?;
    Ok(Json(ActivationResponse {
        activation_id: outcome.activation.id,
        order_id,
        status: outcome.order.status,
        message: if outcome.reused {
            "already_activated".to_string()
        } else {
            "slot_available".to_string()
        },
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

fn parse_plan(value: &str) -> Option<PlanId> {
    match value.trim().to_ascii_lowercase().as_str() {
        "trial" => Some(PlanId::Trial),
        "doctor_start" => Some(PlanId::DoctorStart),
        "doctor_pro" => Some(PlanId::DoctorPro),
        "department" => Some(PlanId::Department),
        "clinic" => Some(PlanId::Clinic),
        "enterprise" => Some(PlanId::Enterprise),
        _ => None,
    }
}
