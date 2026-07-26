use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PlanId {
    Trial,
    DoctorStart,
    DoctorPro,
    Department,
    Clinic,
    Enterprise,
    Vip,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    BatchGeneration,
    BatchPrint,
    ProfileExport,
    ProfileImport,
    DepartmentProfile,
    RoleManagement,
    LocalLicenseServer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicensePayload {
    pub license_id: String,
    pub order_id: Option<String>,
    pub plan: PlanId,
    pub owner_name: Option<String>,
    pub organization_name: Option<String>,
    pub seats: u32,
    pub allowed_machines: Vec<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub valid_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub valid_until: OffsetDateTime,
    pub document_limit_month: u32,
    pub template_limit: u32,
    pub profile_limit: u32,
    pub features: Vec<Feature>,
    pub grace_days: u32,
    pub watermark_mode: WatermarkMode,
    pub issued_by: String,
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkMode {
    None,
    Trial,
    Demo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedLicense {
    pub payload: LicensePayload,
    pub signature_alg: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseDocument {
    pub schema: String,
    pub license: SignedLicense,
}

impl LicensePayload {
    pub fn owner_label(&self) -> String {
        self.organization_name
            .clone()
            .or_else(|| self.owner_name.clone())
            .unwrap_or_else(|| "Dokkomplekt user".to_string())
    }

    pub fn has_feature(&self, feature: &Feature) -> bool {
        self.features.iter().any(|item| item == feature)
    }
}
