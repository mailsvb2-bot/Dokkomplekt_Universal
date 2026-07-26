use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PRODUCT_ACCESS_CONTRACT_VERSION: &str = "v13-rust-tauri-ed25519-product-access";
pub const TRIAL_WATERMARK_TEXT: &str = "ПРОБНАЯ ВЕРСИЯ. ДОКУМЕНТ ТРЕБУЕТ ПРОВЕРКИ.";
pub const EXPIRED_DEMO_WATERMARK_TEXT: &str = "ДЕМО-ДОКУМЕНТ. ЛИЦЕНЗИЯ НЕ АКТИВНА.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductPlanId {
    Trial,
    DoctorStart,
    DoctorPro,
    Department,
    Clinic,
    Enterprise,
    Vip,
}

impl ProductPlanId {
    pub const fn as_wire_id(&self) -> &'static str {
        match self {
            Self::Trial => "trial",
            Self::DoctorStart => "doctor_start",
            Self::DoctorPro => "doctor_pro",
            Self::Department => "department",
            Self::Clinic => "clinic",
            Self::Enterprise => "enterprise",
            Self::Vip => "vip",
        }
    }

    pub fn from_wire_id(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "trial" => Some(Self::Trial),
            "doctor_start" | "professional_start" => Some(Self::DoctorStart),
            "doctor_pro" | "professional_pro" => Some(Self::DoctorPro),
            "department" | "team" => Some(Self::Department),
            "clinic" | "organization" => Some(Self::Clinic),
            "enterprise" => Some(Self::Enterprise),
            "vip" => Some(Self::Vip),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanLimits {
    pub plan_id: ProductPlanId,
    pub title: String,
    pub included_machines: u32,
    pub included_users: u32,
    pub profile_limit: u32,
    pub template_limit: u32,
    pub document_limit_month: u32,
    pub max_documents_per_run: u32,
    pub watermark_mode: String,
    pub batch_generation: bool,
    pub local_license_server: bool,
    pub offline_activation: bool,
    pub grace_days: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseEntitlement {
    pub license_id: String,
    pub plan: ProductPlanId,
    pub owner_name: String,
    pub organization_name: String,
    pub seats: u32,
    pub allowed_machines: Vec<String>,
    pub valid_until: Option<DateTime<Utc>>,
    pub issued_at: Option<DateTime<Utc>>,
    pub features: BTreeSet<String>,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    Vip,
    Paid,
    Trial,
    Grace,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessDecision {
    pub accepted: bool,
    pub mode: AccessMode,
    pub plan: ProductPlanId,
    pub reason: String,
    pub watermark: Option<String>,
    pub document_limit_month: u32,
    pub max_documents_per_run: u32,
}

pub fn plan_limits(plan: ProductPlanId) -> PlanLimits {
    match plan {
        ProductPlanId::Trial => PlanLimits {
            plan_id: ProductPlanId::Trial,
            title: "Trial".into(),
            included_machines: 1,
            included_users: 1,
            profile_limit: 1,
            template_limit: 5,
            document_limit_month: 30,
            max_documents_per_run: 30,
            watermark_mode: "trial".into(),
            batch_generation: false,
            local_license_server: false,
            offline_activation: false,
            grace_days: 0,
        },
        ProductPlanId::DoctorStart => PlanLimits {
            plan_id: ProductPlanId::DoctorStart,
            title: "Professional Start".into(),
            included_machines: 1,
            included_users: 1,
            profile_limit: 1,
            template_limit: 30,
            document_limit_month: 600,
            max_documents_per_run: 10,
            watermark_mode: "none".into(),
            batch_generation: false,
            local_license_server: false,
            offline_activation: true,
            grace_days: 7,
        },
        ProductPlanId::DoctorPro => PlanLimits {
            plan_id: ProductPlanId::DoctorPro,
            title: "Professional Pro".into(),
            included_machines: 2,
            included_users: 2,
            profile_limit: 5,
            template_limit: 200,
            document_limit_month: 3_000,
            max_documents_per_run: 50,
            watermark_mode: "none".into(),
            batch_generation: true,
            local_license_server: false,
            offline_activation: true,
            grace_days: 14,
        },
        ProductPlanId::Department => PlanLimits {
            plan_id: ProductPlanId::Department,
            title: "Team".into(),
            included_machines: 10,
            included_users: 20,
            profile_limit: 50,
            template_limit: 1_000,
            document_limit_month: 30_000,
            max_documents_per_run: 200,
            watermark_mode: "none".into(),
            batch_generation: true,
            local_license_server: true,
            offline_activation: true,
            grace_days: 30,
        },
        ProductPlanId::Clinic => PlanLimits {
            plan_id: ProductPlanId::Clinic,
            title: "Organization".into(),
            included_machines: 20,
            included_users: 100,
            profile_limit: 100,
            template_limit: 2_000,
            document_limit_month: 100_000,
            max_documents_per_run: 250,
            watermark_mode: "none".into(),
            batch_generation: true,
            local_license_server: true,
            offline_activation: true,
            grace_days: 30,
        },
        ProductPlanId::Enterprise => PlanLimits {
            plan_id: ProductPlanId::Enterprise,
            title: "Enterprise".into(),
            included_machines: 100,
            included_users: 500,
            profile_limit: 500,
            template_limit: 10_000,
            document_limit_month: 1_000_000,
            max_documents_per_run: 5_000,
            watermark_mode: "none".into(),
            batch_generation: true,
            local_license_server: true,
            offline_activation: true,
            grace_days: 60,
        },
        ProductPlanId::Vip => PlanLimits {
            plan_id: ProductPlanId::Vip,
            title: "VIP local".into(),
            included_machines: 1,
            included_users: 1,
            profile_limit: 999,
            template_limit: 9999,
            document_limit_month: 1_000_000,
            max_documents_per_run: 5_000,
            watermark_mode: "none".into(),
            batch_generation: true,
            local_license_server: false,
            offline_activation: true,
            grace_days: 3650,
        },
    }
}

fn configured_vip_access_code() -> Option<String> {
    option_env!("DOKKOMPLEKT_VIP_ACCESS_CODE")
        .map(|value| {
            value
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|value| !value.is_empty())
}

pub fn validate_vip_access_code(code: &str) -> AccessDecision {
    let normalized: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
    if configured_vip_access_code()
        .as_deref()
        .is_some_and(|expected| constant_time_eq(expected.as_bytes(), normalized.as_bytes()))
    {
        let limits = plan_limits(ProductPlanId::Vip);
        return decision(
            true,
            AccessMode::Vip,
            limits,
            "vip_code_accepted_locally",
            None,
        );
    }
    if normalized.is_empty() {
        let limits = plan_limits(ProductPlanId::Trial);
        return decision(
            true,
            AccessMode::Trial,
            limits,
            "no_code_local_trial",
            Some(TRIAL_WATERMARK_TEXT.into()),
        );
    }
    let limits = plan_limits(ProductPlanId::Trial);
    decision(
        false,
        AccessMode::Blocked,
        limits,
        "invalid_access_code",
        Some(EXPIRED_DEMO_WATERMARK_TEXT.into()),
    )
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

pub fn evaluate_entitlement(
    entitlement: Option<LicenseEntitlement>,
    machine: &str,
    now: DateTime<Utc>,
) -> AccessDecision {
    let Some(ent) = entitlement else {
        return validate_vip_access_code("");
    };
    let limits = plan_limits(ent.plan.clone());
    if let Some(valid_until) = ent.valid_until {
        if now > valid_until + Duration::days(limits.grace_days) {
            return decision(
                false,
                AccessMode::Blocked,
                limits,
                "license_expired",
                Some(EXPIRED_DEMO_WATERMARK_TEXT.into()),
            );
        }
        if now > valid_until {
            return decision(
                true,
                AccessMode::Grace,
                limits,
                "license_grace_period",
                None,
            );
        }
    }
    if !ent.allowed_machines.is_empty()
        && !ent
            .allowed_machines
            .iter()
            .any(|m| m.eq_ignore_ascii_case(machine))
    {
        return decision(
            false,
            AccessMode::Blocked,
            limits,
            "machine_not_allowed",
            Some(EXPIRED_DEMO_WATERMARK_TEXT.into()),
        );
    }
    decision(true, AccessMode::Paid, limits, "license_active", None)
}

pub fn no_patient_data_keys_in_license_state(keys: &[&str]) -> bool {
    let forbidden = [
        "patient",
        "diagnosis",
        "treatment",
        "fio",
        "template_text",
        "document_text",
        "full_name",
    ];
    keys.iter().all(|key| {
        let k = key.to_lowercase();
        !forbidden.iter().any(|f| k.contains(f))
    })
}

fn decision(
    accepted: bool,
    mode: AccessMode,
    limits: PlanLimits,
    reason: &str,
    watermark: Option<String>,
) -> AccessDecision {
    AccessDecision {
        accepted,
        mode,
        plan: limits.plan_id,
        reason: reason.into(),
        watermark,
        document_limit_month: limits.document_limit_month,
        max_documents_per_run: limits.max_documents_per_run,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vip_code_is_compile_time_configured_and_fails_closed_when_absent() {
        match configured_vip_access_code() {
            Some(code) => {
                let d = validate_vip_access_code(&code);
                assert!(d.accepted);
                assert_eq!(d.mode, AccessMode::Vip);
            }
            None => {
                let d = validate_vip_access_code("000000");
                assert!(!d.accepted);
                assert_eq!(d.mode, AccessMode::Blocked);
            }
        }
    }

    #[test]
    fn license_state_rejects_patient_keys() {
        assert!(no_patient_data_keys_in_license_state(&[
            "license_id",
            "plan",
            "valid_until"
        ]));
        assert!(!no_patient_data_keys_in_license_state(&[
            "patient.full_name"
        ]));
    }
}

#[cfg(test)]
mod canonical_plan_tests {
    use super::ProductPlanId;

    #[test]
    fn canonical_plan_wire_ids_are_stable_and_legacy_ids_parse() {
        let cases = [
            (ProductPlanId::Trial, "trial"),
            (ProductPlanId::DoctorStart, "doctor_start"),
            (ProductPlanId::DoctorPro, "doctor_pro"),
            (ProductPlanId::Department, "department"),
            (ProductPlanId::Clinic, "clinic"),
            (ProductPlanId::Enterprise, "enterprise"),
            (ProductPlanId::Vip, "vip"),
        ];
        for (plan, id) in cases {
            assert_eq!(plan.as_wire_id(), id);
            assert_eq!(ProductPlanId::from_wire_id(id), Some(plan));
        }
        assert_eq!(
            ProductPlanId::from_wire_id("team"),
            Some(ProductPlanId::Department)
        );
        assert_eq!(
            ProductPlanId::from_wire_id("organization"),
            Some(ProductPlanId::Clinic)
        );
    }
}
