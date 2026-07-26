use crate::core_error::{CoreError, CoreResult};
use crate::machine::MachineFingerprint;
use crate::models::{LicensePayload, PlanId, WatermarkMode};
use crate::usage::UsageLedger;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRequest {
    pub now_utc: OffsetDateTime,
    pub month_key: String,
    pub machine: MachineFingerprint,
    pub requested_documents: u32,
    pub template_count: Option<u32>,
    pub profile_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessStatus {
    Allowed,
    Warning,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessDecision {
    pub status: AccessStatus,
    pub code: String,
    pub message: String,
    pub watermark: bool,
    pub documents_left_month: u32,
    pub plan: PlanId,
}

#[derive(Debug, Clone, Copy)]
struct PlanPolicy {
    max_documents_per_run: u32,
    overage_percent: u32,
}

pub fn evaluate_access(
    payload: &LicensePayload,
    usage: &UsageLedger,
    request: &AccessRequest,
) -> CoreResult<AccessDecision> {
    validate_time(payload, request.now_utc)?;
    validate_machine(payload, &request.machine)?;

    let policy = plan_policy(&payload.plan);
    if request.requested_documents > policy.max_documents_per_run {
        return Ok(deny(
            payload,
            "per_run_limit",
            format!(
                "plan allows up to {} documents per run",
                policy.max_documents_per_run
            ),
            0,
        ));
    }
    if let Some(count) = request.template_count {
        if count > payload.template_limit {
            return Ok(deny(
                payload,
                "template_limit",
                "template limit exceeded",
                0,
            ));
        }
    }
    if let Some(count) = request.profile_count {
        if count > payload.profile_limit {
            return Ok(deny(payload, "profile_limit", "profile limit exceeded", 0));
        }
    }

    let used = usage.documents_for_month(&request.month_key);
    let hard_limit = payload.document_limit_month
        + payload
            .document_limit_month
            .saturating_mul(policy.overage_percent)
            / 100;
    let projected = used.saturating_add(request.requested_documents);
    if projected > hard_limit {
        return Ok(deny(
            payload,
            "monthly_limit",
            "monthly document limit exceeded",
            payload.document_limit_month.saturating_sub(used),
        ));
    }

    let left = payload.document_limit_month.saturating_sub(projected);
    if projected > payload.document_limit_month {
        return Ok(warn(
            payload,
            "monthly_overage",
            "monthly limit exceeded; grace overage is active",
            left,
        ));
    }
    if projected >= payload.document_limit_month.saturating_mul(80) / 100 {
        return Ok(warn(
            payload,
            "monthly_80_percent",
            "more than 80 percent of monthly limit will be used",
            left,
        ));
    }

    Ok(allow(payload, left))
}

fn validate_time(payload: &LicensePayload, now: OffsetDateTime) -> CoreResult<()> {
    if now < payload.valid_from {
        return Err(CoreError::NotYetValid);
    }
    if now > payload.valid_until {
        return Err(CoreError::Expired);
    }
    Ok(())
}

fn validate_machine(payload: &LicensePayload, machine: &MachineFingerprint) -> CoreResult<()> {
    if payload.allowed_machines.is_empty() || machine.matches_any(&payload.allowed_machines) {
        Ok(())
    } else {
        Err(CoreError::MachineMismatch)
    }
}

pub fn max_documents_per_run(plan: &PlanId) -> u32 {
    plan_policy(plan).max_documents_per_run
}

fn plan_policy(plan: &PlanId) -> PlanPolicy {
    match plan {
        PlanId::Trial => PlanPolicy {
            max_documents_per_run: 3,
            overage_percent: 0,
        },
        PlanId::DoctorStart => PlanPolicy {
            max_documents_per_run: 10,
            overage_percent: 20,
        },
        PlanId::DoctorPro => PlanPolicy {
            max_documents_per_run: 50,
            overage_percent: 20,
        },
        PlanId::Department => PlanPolicy {
            max_documents_per_run: 100,
            overage_percent: 20,
        },
        PlanId::Clinic => PlanPolicy {
            max_documents_per_run: 250,
            overage_percent: 20,
        },
        PlanId::Enterprise => PlanPolicy {
            max_documents_per_run: 1000,
            overage_percent: 20,
        },
        PlanId::Vip => PlanPolicy {
            max_documents_per_run: 5000,
            overage_percent: 100,
        },
    }
}

fn allow(payload: &LicensePayload, left: u32) -> AccessDecision {
    AccessDecision {
        status: AccessStatus::Allowed,
        code: "ok".to_string(),
        message: "access allowed".to_string(),
        watermark: should_watermark(payload),
        documents_left_month: left,
        plan: payload.plan.clone(),
    }
}

fn warn(payload: &LicensePayload, code: &str, message: &str, left: u32) -> AccessDecision {
    AccessDecision {
        status: AccessStatus::Warning,
        code: code.to_string(),
        message: message.to_string(),
        watermark: should_watermark(payload),
        documents_left_month: left,
        plan: payload.plan.clone(),
    }
}

fn deny(
    payload: &LicensePayload,
    code: &str,
    message: impl Into<String>,
    left: u32,
) -> AccessDecision {
    AccessDecision {
        status: AccessStatus::Denied,
        code: code.to_string(),
        message: message.into(),
        watermark: should_watermark(payload),
        documents_left_month: left,
        plan: payload.plan.clone(),
    }
}

fn should_watermark(payload: &LicensePayload) -> bool {
    matches!(
        payload.watermark_mode,
        WatermarkMode::Trial | WatermarkMode::Demo
    )
}
