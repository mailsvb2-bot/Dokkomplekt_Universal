use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use super::medical_semantics::{
    SICK_LEAVE_VK_COMMISSION_DATE, SICK_LEAVE_VK_POSITION, SICK_LEAVE_VK_PROTOCOL_DATE,
    SICK_LEAVE_VK_PROTOCOL_NUMBER, SICK_LEAVE_VK_WORKPLACE, VK_MSE_COMMISSION_DATE,
    VK_MSE_POSITION, VK_MSE_PROTOCOL_DATE, VK_MSE_PROTOCOL_NUMBER, VK_MSE_WORKPLACE,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MedicalDocumentRole {
    PrimaryInspection,
    DischargeEpicrisis,
    Diary,
    RvkAct,
    CommissionInspection,
    SickLeaveCommission,
    VkMse,
    ReceptionInspection,
    GenericMedical,
}

impl MedicalDocumentRole {
    pub fn role_id(&self) -> &'static str {
        match self {
            Self::PrimaryInspection => "primary",
            Self::DischargeEpicrisis => "discharge",
            Self::Diary => "diaries",
            Self::RvkAct => "rvk_act",
            Self::CommissionInspection => "commission",
            Self::SickLeaveCommission => "sick_leave_vk",
            Self::VkMse => "vk_mse",
            Self::ReceptionInspection => "reception",
            Self::GenericMedical => "medical_generic",
        }
    }

    pub fn from_role_id(raw: &str) -> Self {
        match crate::domains::medical::canonical_medical_role(raw).as_str() {
            "primary" => Self::PrimaryInspection,
            "discharge" => Self::DischargeEpicrisis,
            "diaries" => Self::Diary,
            "rvk_act" => Self::RvkAct,
            "commission" => Self::CommissionInspection,
            "sick_leave_vk" => Self::SickLeaveCommission,
            "vk_mse" => Self::VkMse,
            "reception" => Self::ReceptionInspection,
            _ => Self::GenericMedical,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaryCalendarOptions {
    pub admission_date: NaiveDate,
    pub discharge_date: NaiveDate,
    pub include_holidays: bool,
    pub hourly_offsets: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepDiaryEntry {
    pub day_number: u32,
    pub date: NaiveDate,
    pub month: u32,
    pub year: i32,
    pub hour_offsets: Vec<u32>,
    pub signatures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalRenderPlan {
    pub role: MedicalDocumentRole,
    pub required_fields: Vec<String>,
    pub optional_fields: Vec<String>,
    pub output_sections: Vec<String>,
    pub watermark_allowed: bool,
}

pub fn build_deep_diary_calendar(opts: DiaryCalendarOptions) -> Vec<DeepDiaryEntry> {
    if opts.discharge_date <= opts.admission_date {
        return Vec::new();
    }
    let mut entries = Vec::new();
    let mut cursor = opts.admission_date + Duration::days(1);
    let mut day_number = 1;
    while cursor <= opts.discharge_date {
        let is_holiday = matches!(
            cursor.weekday(),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        );
        if opts.include_holidays || !is_holiday {
            entries.push(DeepDiaryEntry {
                day_number,
                date: cursor,
                month: cursor.month(),
                year: cursor.year(),
                hour_offsets: opts.hourly_offsets.clone(),
                signatures: vec!["Лечащий врач".into(), "Заведующий отделением".into()],
            });
            day_number += 1;
        }
        cursor += Duration::days(1);
    }
    entries
}

/// Single Medical-domain contract used by workflow, popup and post-render checks.
/// Unknown medical templates remain driven only by their own configured fields.
pub fn build_medical_render_plan(
    role: MedicalDocumentRole,
    sick_leave_enabled: bool,
    treatment_found: bool,
) -> MedicalRenderPlan {
    let mut required = Vec::new();
    if !matches!(role, MedicalDocumentRole::GenericMedical) {
        required.extend(["medical.admission_date".into(), "medical.diagnosis".into()]);
        if !matches!(role, MedicalDocumentRole::Diary) {
            required.push("medical.case_number".into());
        }
    }
    let mut optional = Vec::new();
    let mut sections = Vec::new();

    match role {
        MedicalDocumentRole::DischargeEpicrisis => {
            required.push("medical.discharge_date".into());
            require_treatment_if_missing(&mut required, treatment_found);
            if sick_leave_enabled {
                required.push("medical.sick_leave_number".into());
            }
            required.extend(["medical.workplace".into(), "medical.position".into()]);
            sections.extend([
                "demographics".into(),
                "diagnosis".into(),
                "treatment".into(),
                "expert_anamnesis".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::Diary => {
            required.push("medical.discharge_date".into());
            optional.push("medical.treatment".into());
            sections.extend([
                "calendar_entries".into(),
                "diary_text".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::RvkAct => {
            required.extend([
                "medical.discharge_date".into(),
                "medical.rvk_commissariat".into(),
                "medical.rvk_act_number".into(),
            ]);
            optional.push("medical.treatment".into());
            sections.extend([
                "diagnosis".into(),
                "rvk_conclusion".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::CommissionInspection => {
            required.extend([
                "medical.commission_date".into(),
                "medical.commission_number".into(),
            ]);
            require_treatment_if_missing(&mut required, treatment_found);
            sections.extend([
                "commission_members".into(),
                "diagnosis".into(),
                "treatment".into(),
                "conclusion".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::SickLeaveCommission => {
            required.extend([
                SICK_LEAVE_VK_COMMISSION_DATE.into(),
                SICK_LEAVE_VK_PROTOCOL_NUMBER.into(),
                SICK_LEAVE_VK_PROTOCOL_DATE.into(),
                "medical.sick_leave_commission_date".into(),
                "medical.sick_leave_number".into(),
                SICK_LEAVE_VK_WORKPLACE.into(),
                SICK_LEAVE_VK_POSITION.into(),
            ]);
            require_treatment_if_missing(&mut required, treatment_found);
            sections.extend([
                "work".into(),
                "sick_leave_period".into(),
                "diagnosis".into(),
                "treatment".into(),
                "protocol".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::VkMse => {
            required.extend([
                VK_MSE_COMMISSION_DATE.into(),
                VK_MSE_PROTOCOL_NUMBER.into(),
                VK_MSE_PROTOCOL_DATE.into(),
                VK_MSE_WORKPLACE.into(),
                VK_MSE_POSITION.into(),
            ]);
            require_treatment_if_missing(&mut required, treatment_found);
            sections.extend([
                "work".into(),
                "diagnosis".into(),
                "treatment".into(),
                "protocol".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::PrimaryInspection => {
            require_treatment_if_missing(&mut required, treatment_found);
            required.extend(["medical.workplace".into(), "medical.position".into()]);
            sections.extend([
                "complaints".into(),
                "anamnesis".into(),
                "status".into(),
                "treatment".into(),
                "expert_anamnesis".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::ReceptionInspection => {
            sections.extend([
                "reception_status".into(),
                "referral_phrase".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::GenericMedical => sections.push("generic_medical_template".into()),
    }

    required.sort();
    required.dedup();
    optional.retain(|field| !required.contains(field));
    optional.sort();
    optional.dedup();
    MedicalRenderPlan {
        role,
        required_fields: required,
        optional_fields: optional,
        output_sections: sections,
        watermark_allowed: true,
    }
}

fn require_treatment_if_missing(required: &mut Vec<String>, treatment_found: bool) {
    if !treatment_found {
        required.push("medical.treatment".into());
    }
}

pub fn normalize_institution_text(text: &str) -> String {
    text.replace("ГБУЗ НО ПБ №2", "ГБУЗ НО «НКЦПЗ» диспансер №2")
        .replace("отделение №3", "диспансер №2")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diary_starts_after_admission_and_stops_on_discharge() {
        let entries = build_deep_diary_calendar(DiaryCalendarOptions {
            admission_date: NaiveDate::from_ymd_opt(2026, 5, 10).unwrap(),
            discharge_date: NaiveDate::from_ymd_opt(2026, 5, 12).unwrap(),
            include_holidays: true,
            hourly_offsets: vec![30],
        });
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].date.day(), 11);
        assert_eq!(entries[1].date.day(), 12);
        assert!(entries[0].signatures.iter().any(|s| s.contains("Лечащий")));
    }

    #[test]
    fn discharge_preserves_legacy_preflight_boundaries() {
        let missing =
            build_medical_render_plan(MedicalDocumentRole::DischargeEpicrisis, true, false);
        for field in [
            "medical.case_number",
            "medical.admission_date",
            "medical.discharge_date",
            "medical.diagnosis",
            "medical.treatment",
            "medical.sick_leave_number",
        ] {
            assert!(
                missing.required_fields.contains(&field.to_string()),
                "missing {field}"
            );
        }
        let parsed =
            build_medical_render_plan(MedicalDocumentRole::DischargeEpicrisis, false, true);
        assert!(!parsed.required_fields.contains(&"medical.treatment".into()));
        assert!(!parsed
            .required_fields
            .contains(&"medical.sick_leave_number".into()));
    }

    #[test]
    fn diary_is_the_only_known_role_without_case_number() {
        let diary = build_medical_render_plan(MedicalDocumentRole::Diary, false, false);
        assert!(!diary
            .required_fields
            .contains(&"medical.case_number".into()));
        assert!(diary
            .required_fields
            .contains(&"medical.admission_date".into()));
        assert!(diary
            .required_fields
            .contains(&"medical.discharge_date".into()));
        assert!(!diary.required_fields.contains(&"medical.treatment".into()));
    }

    #[test]
    fn reception_does_not_require_treatment() {
        let plan =
            build_medical_render_plan(MedicalDocumentRole::ReceptionInspection, false, false);
        assert!(plan.required_fields.contains(&"medical.case_number".into()));
        assert!(plan
            .required_fields
            .contains(&"medical.admission_date".into()));
        assert!(plan.required_fields.contains(&"medical.diagnosis".into()));
        assert!(!plan.required_fields.contains(&"medical.treatment".into()));
    }

    #[test]
    fn unknown_medical_role_does_not_inherit_legacy_requirements() {
        let plan = build_medical_render_plan(MedicalDocumentRole::GenericMedical, false, false);
        assert!(plan.required_fields.is_empty());
        assert!(plan.optional_fields.is_empty());
    }

    #[test]
    fn mse_and_sick_leave_vk_have_distinct_storage_fields() {
        let mse = build_medical_render_plan(MedicalDocumentRole::VkMse, false, false);
        let sick =
            build_medical_render_plan(MedicalDocumentRole::SickLeaveCommission, false, false);
        assert!(mse.required_fields.contains(&VK_MSE_PROTOCOL_NUMBER.into()));
        assert!(sick
            .required_fields
            .contains(&SICK_LEAVE_VK_PROTOCOL_NUMBER.into()));
        assert!(!mse
            .required_fields
            .contains(&SICK_LEAVE_VK_PROTOCOL_NUMBER.into()));
        assert!(!sick
            .required_fields
            .contains(&VK_MSE_PROTOCOL_NUMBER.into()));
        assert!(!mse
            .required_fields
            .contains(&"medical.protocol_number".into()));
        assert!(!sick
            .required_fields
            .contains(&"medical.protocol_number".into()));
    }

    #[test]
    fn no_obsolete_legacy_contract_ids_are_required() {
        for role in [
            MedicalDocumentRole::PrimaryInspection,
            MedicalDocumentRole::DischargeEpicrisis,
            MedicalDocumentRole::Diary,
            MedicalDocumentRole::RvkAct,
            MedicalDocumentRole::CommissionInspection,
            MedicalDocumentRole::SickLeaveCommission,
            MedicalDocumentRole::VkMse,
            MedicalDocumentRole::ReceptionInspection,
        ] {
            let plan = build_medical_render_plan(role, false, false);
            assert!(!plan.required_fields.iter().any(|field| matches!(
                field.as_str(),
                "rvk.district"
                    | "commission.date"
                    | "vk_mse.date"
                    | "workplace.organization"
                    | "medical.sick_leave_from"
            )));
        }
    }

    #[test]
    fn stable_role_ids_cover_all_legacy_generated_documents() {
        let roles = [
            (MedicalDocumentRole::PrimaryInspection, "primary"),
            (MedicalDocumentRole::DischargeEpicrisis, "discharge"),
            (MedicalDocumentRole::Diary, "diaries"),
            (MedicalDocumentRole::RvkAct, "rvk_act"),
            (MedicalDocumentRole::CommissionInspection, "commission"),
            (MedicalDocumentRole::SickLeaveCommission, "sick_leave_vk"),
            (MedicalDocumentRole::VkMse, "vk_mse"),
            (MedicalDocumentRole::ReceptionInspection, "reception"),
        ];
        for (role, id) in roles {
            assert_eq!(role.role_id(), id);
            assert_eq!(MedicalDocumentRole::from_role_id(id), role);
        }
    }
}
