use crate::issuer::{issue_license, IssueLicenseInput};
use crate::state::{AppState, OrderStatus};
use crate::storage::{LicenseRecord, StoreError};
use crate::traffic_guard::{ClientIp, RateLimitScope};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use dokkomplekt_license_core::models::{LicenseDocument, PlanId};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct IssueRequest {
    pub owner_name: Option<String>,
    pub organization_name: Option<String>,
    pub machine_hash: String,
    pub issue_token: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/orders/:order_id/license", post(issue_for_order))
}

async fn issue_for_order(
    Extension(client_ip): Extension<ClientIp>,
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    Json(request): Json<IssueRequest>,
) -> Result<Json<LicenseDocument>, StatusCode> {
    if !state.traffic_guard.check(
        client_ip.0,
        RateLimitScope::LicenseIssue,
        state.config.order_access_limit_per_minute,
        Duration::from_secs(60),
    ) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let requested_machine = request.machine_hash.trim();
    if requested_machine.is_empty()
        || requested_machine.len() > 256
        || requested_machine.chars().any(char::is_control)
        || request
            .owner_name
            .as_deref()
            .is_some_and(|value| value.len() > 256 || value.chars().any(char::is_control))
        || request
            .organization_name
            .as_deref()
            .is_some_and(|value| value.len() > 256 || value.chars().any(char::is_control))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !issue_token_matches(
        state.config.license_issue_secret.as_deref(),
        request.issue_token.as_deref(),
    ) {
        return Err(StatusCode::UNAUTHORIZED);
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
    let activations = state
        .store
        .activations_for_order_async(order_id)
        .await
        .map_err(store_error_status)?;
    if !activations
        .iter()
        .any(|activation| activation.machine_hash.trim() == requested_machine)
    {
        return Err(StatusCode::CONFLICT);
    }
    let mut allowed_machines = activations
        .into_iter()
        .map(|activation| activation.machine_hash.trim().to_string())
        .filter(|machine| !machine.is_empty())
        .collect::<Vec<_>>();
    allowed_machines.sort();
    allowed_machines.dedup();
    let plan = parse_plan(&order.plan).ok_or(StatusCode::BAD_REQUEST)?;
    let issuer_key = state
        .config
        .issuer_key_b64
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let document = issue_license(
        IssueLicenseInput {
            order_id,
            plan,
            owner_name: request.owner_name,
            organization_name: request.organization_name,
            allowed_machines,
            valid_days: state.config.default_license_days,
        },
        &state.config.issuer_id,
        &issuer_key,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let record = LicenseRecord {
        id: Uuid::new_v4(),
        order_id,
        license_id: document.license.payload.license_id.clone(),
        document_json: serde_json::to_string(&document)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        issued_at: document.license.payload.issued_at,
        revoked_at: None,
    };
    let outcome = state
        .store
        .issue_license_for_paid_order_async(record)
        .await
        .map_err(store_error_status)?;
    let response = serde_json::from_str(&outcome.record.document_json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(response))
}

pub fn issue_token_matches(configured_secret: Option<&str>, supplied_token: Option<&str>) -> bool {
    let Some(expected) = configured_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    supplied_token
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

fn parse_plan(value: &str) -> Option<PlanId> {
    match value.trim() {
        "doctor_start" => Some(PlanId::DoctorStart),
        "doctor_pro" => Some(PlanId::DoctorPro),
        "department" => Some(PlanId::Department),
        "clinic" => Some(PlanId::Clinic),
        "enterprise" => Some(PlanId::Enterprise),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::issue_token_matches;

    #[test]
    fn issue_token_is_fail_closed_without_server_secret() {
        assert!(!issue_token_matches(None, None));
        assert!(!issue_token_matches(None, Some("attacker-token")));
        assert!(!issue_token_matches(Some(""), Some("attacker-token")));
    }

    #[test]
    fn issue_token_requires_exact_non_empty_match() {
        assert!(issue_token_matches(
            Some("server-secret"),
            Some("server-secret")
        ));
        assert!(!issue_token_matches(Some("server-secret"), None));
        assert!(!issue_token_matches(
            Some("server-secret"),
            Some("server-secreu")
        ));
    }
}
