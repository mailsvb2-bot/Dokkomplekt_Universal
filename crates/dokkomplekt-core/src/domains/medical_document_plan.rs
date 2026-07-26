use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

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

pub fn build_medical_render_plan(
    role: MedicalDocumentRole,
    sick_leave_enabled: bool,
    treatment_found: bool,
) -> MedicalRenderPlan {
    let mut required = vec!["medical.case_number".into()];
    let mut optional = Vec::new();
    let mut sections = Vec::new();
    match role {
        MedicalDocumentRole::DischargeEpicrisis => {
            required.extend(["medical.discharge_date".into(), "medical.diagnosis".into()]);
            if !treatment_found {
                required.push("medical.treatment".into());
            }
            if sick_leave_enabled {
                optional.push("medical.sick_leave_number".into());
            }
            sections.extend([
                "demographics".into(),
                "diagnosis".into(),
                "treatment".into(),
                "expert_anamnesis".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::Diary => {
            required.extend([
                "medical.admission_date".into(),
                "medical.discharge_date".into(),
                "medical.diagnosis".into(),
            ]);
            sections.extend([
                "calendar_entries".into(),
                "diary_text".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::RvkAct => {
            required.extend([
                "medical.discharge_date".into(),
                "rvk.district".into(),
                "medical.rvk_act_number".into(),
                "medical.diagnosis".into(),
            ]);
            sections.extend([
                "diagnosis".into(),
                "rvk_conclusion".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::CommissionInspection => {
            required.extend([
                "commission.date".into(),
                "medical.commission_number".into(),
                "medical.protocol_number".into(),
                "medical.diagnosis".into(),
            ]);
            sections.extend([
                "commission_members".into(),
                "conclusion".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::SickLeaveCommission => {
            required.extend([
                "medical.commission_date".into(),
                "medical.protocol_number".into(),
                "medical.sick_leave_number".into(),
                "medical.sick_leave_from".into(),
                "medical.workplace".into(),
                "medical.position".into(),
            ]);
            sections.extend([
                "work".into(),
                "sick_leave_period".into(),
                "protocol".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::VkMse => {
            required.extend([
                "vk_mse.date".into(),
                "medical.protocol_number".into(),
                "workplace.organization".into(),
                "workplace.position".into(),
            ]);
            sections.extend([
                "work".into(),
                "diagnosis".into(),
                "protocol".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::PrimaryInspection => {
            required.extend(["medical.admission_date".into(), "medical.diagnosis".into()]);
            sections.extend([
                "complaints".into(),
                "anamnesis".into(),
                "status".into(),
                "expert_anamnesis".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::ReceptionInspection => {
            required.extend(["medical.admission_date".into(), "medical.diagnosis".into()]);
            sections.extend([
                "reception_status".into(),
                "referral_phrase".into(),
                "signatures".into(),
            ]);
        }
        MedicalDocumentRole::GenericMedical => {
            required.push("medical.diagnosis".into());
            sections.push("generic_medical_template".into());
        }
    }
    MedicalRenderPlan {
        role,
        required_fields: required,
        optional_fields: optional,
        output_sections: sections,
        watermark_allowed: true,
    }
}

pub fn normalize_institution_text(text: &str) -> String {
    text.replace("ГБУЗ НО ПБ №2", "ГБУЗ НО «НКЦПЗ» диспансер №2")
        .replace("отделение №3", "диспансер №2")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

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
    fn discharge_plan_asks_treatment_only_if_missing() {
        let plan = build_medical_render_plan(MedicalDocumentRole::DischargeEpicrisis, true, false);
        assert!(plan.required_fields.contains(&"medical.treatment".into()));
        assert!(plan
            .optional_fields
            .contains(&"medical.sick_leave_number".into()));
    }

    #[test]
    fn donor_commission_identifiers_are_not_lost() {
        let rvk = build_medical_render_plan(MedicalDocumentRole::RvkAct, false, true);
        assert!(rvk
            .required_fields
            .contains(&"medical.rvk_act_number".into()));
        let commission =
            build_medical_render_plan(MedicalDocumentRole::CommissionInspection, false, true);
        assert!(commission
            .required_fields
            .contains(&"medical.protocol_number".into()));
        let sick_leave =
            build_medical_render_plan(MedicalDocumentRole::SickLeaveCommission, true, true);
        assert!(sick_leave
            .required_fields
            .contains(&"medical.sick_leave_from".into()));
    }
}
