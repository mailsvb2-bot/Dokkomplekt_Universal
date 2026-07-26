use base64::{engine::general_purpose::STANDARD, Engine as _};
use dokkomplekt_license_core::canonical::canonical_json;
use dokkomplekt_license_core::models::{
    Feature, LicenseDocument, LicensePayload, PlanId, SignedLicense, WatermarkMode,
};
use ed25519_dalek::{Signer, SigningKey};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct IssueLicenseInput {
    pub order_id: Uuid,
    pub plan: PlanId,
    pub owner_name: Option<String>,
    pub organization_name: Option<String>,
    pub allowed_machines: Vec<String>,
    pub valid_days: i64,
}

pub fn issue_license(
    input: IssueLicenseInput,
    issuer_id: &str,
    issuer_key_b64: &str,
) -> anyhow::Result<LicenseDocument> {
    let key_bytes = STANDARD.decode(issuer_key_b64)?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("issuer key must be 32 bytes"))?;
    let signing_key = SigningKey::from_bytes(&key_array);
    let now = OffsetDateTime::now_utc();
    let limits = limits_for_plan(&input.plan);
    let payload = LicensePayload {
        license_id: format!("DKK-{}", Uuid::new_v4()),
        order_id: Some(input.order_id.to_string()),
        plan: input.plan,
        owner_name: input.owner_name,
        organization_name: input.organization_name,
        seats: limits.seats,
        allowed_machines: input.allowed_machines,
        valid_from: now,
        valid_until: now + Duration::days(input.valid_days.max(1)),
        document_limit_month: limits.document_limit_month,
        template_limit: limits.template_limit,
        profile_limit: limits.profile_limit,
        features: limits.features,
        grace_days: limits.grace_days,
        watermark_mode: limits.watermark_mode,
        issued_by: issuer_id.to_string(),
        issued_at: now,
        metadata: Default::default(),
    };
    let message = canonical_json(&payload)?;
    let signature = signing_key.sign(&message);
    Ok(LicenseDocument {
        schema: "dokkomplekt.license.v1".to_string(),
        license: SignedLicense {
            payload,
            signature_alg: "ed25519".to_string(),
            signature: STANDARD.encode(signature.to_bytes()),
        },
    })
}

struct PlanLimits {
    seats: u32,
    document_limit_month: u32,
    template_limit: u32,
    profile_limit: u32,
    grace_days: u32,
    watermark_mode: WatermarkMode,
    features: Vec<Feature>,
}

fn limits_for_plan(plan: &PlanId) -> PlanLimits {
    match plan {
        PlanId::Trial => PlanLimits {
            seats: 1,
            document_limit_month: 30,
            template_limit: 5,
            profile_limit: 1,
            grace_days: 0,
            watermark_mode: WatermarkMode::Trial,
            features: vec![],
        },
        PlanId::DoctorStart => PlanLimits {
            seats: 1,
            document_limit_month: 600,
            template_limit: 30,
            profile_limit: 1,
            grace_days: 7,
            watermark_mode: WatermarkMode::None,
            features: vec![],
        },
        PlanId::DoctorPro => PlanLimits {
            seats: 2,
            document_limit_month: 3000,
            template_limit: 150,
            profile_limit: 3,
            grace_days: 7,
            watermark_mode: WatermarkMode::None,
            features: vec![
                Feature::BatchGeneration,
                Feature::BatchPrint,
                Feature::ProfileExport,
            ],
        },
        PlanId::Department => PlanLimits {
            seats: 5,
            document_limit_month: 20000,
            template_limit: 500,
            profile_limit: 10,
            grace_days: 14,
            watermark_mode: WatermarkMode::None,
            features: vec![
                Feature::BatchGeneration,
                Feature::BatchPrint,
                Feature::DepartmentProfile,
                Feature::RoleManagement,
            ],
        },
        PlanId::Clinic => PlanLimits {
            seats: 20,
            document_limit_month: 100000,
            template_limit: 2000,
            profile_limit: 50,
            grace_days: 30,
            watermark_mode: WatermarkMode::None,
            features: vec![
                Feature::BatchGeneration,
                Feature::BatchPrint,
                Feature::DepartmentProfile,
                Feature::RoleManagement,
                Feature::LocalLicenseServer,
            ],
        },
        PlanId::Enterprise => PlanLimits {
            seats: 9999,
            document_limit_month: 9_999_999,
            template_limit: 999_999,
            profile_limit: 9999,
            grace_days: 45,
            watermark_mode: WatermarkMode::None,
            features: vec![
                Feature::BatchGeneration,
                Feature::BatchPrint,
                Feature::DepartmentProfile,
                Feature::RoleManagement,
                Feature::LocalLicenseServer,
            ],
        },
        PlanId::Vip => PlanLimits {
            seats: 1,
            document_limit_month: 9_999_999,
            template_limit: 999_999,
            profile_limit: 9999,
            grace_days: 3650,
            watermark_mode: WatermarkMode::None,
            features: vec![
                Feature::BatchGeneration,
                Feature::BatchPrint,
                Feature::ProfileExport,
                Feature::ProfileImport,
                Feature::DepartmentProfile,
                Feature::RoleManagement,
                Feature::LocalLicenseServer,
            ],
        },
    }
}
