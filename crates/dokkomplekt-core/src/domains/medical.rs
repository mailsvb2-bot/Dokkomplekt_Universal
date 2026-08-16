use crate::core::{FieldExtractionRule, Workflow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn canonical_medical_role(raw_role: &str) -> String {
    let normalized = normalize_role_slug(raw_role);
    match normalized.as_str() {
        "discharge"
        | "discharge_epicrisis"
        | "dischargeepicrisis"
        | "выписной_эпикриз"
        | "выписка"
        | "эпикриз" => "discharge".into(),
        "diaries" | "diary" | "medicaldiary" | "дневник" | "дневники" | "ежедневные_записи" => {
            "diaries".into()
        }
        "rvk_act"
        | "rvkact"
        | "акт_для_рвк"
        | "акт_рвк"
        | "рвк"
        | "военный_комиссариат"
        | "военкомат" => "rvk_act".into(),
        "commission"
        | "commissioninspection"
        | "jointmedicalexam"
        | "совместный_осмотр"
        | "комиссионный_осмотр"
        | "комиссия"
        | "врачебная_комиссия" => "commission".into(),
        "sick_leave_vk"
        | "sickleavevk"
        | "вк_больничный"
        | "вк_по_больничному"
        | "продление_больничного" => "sick_leave_vk".into(),
        "vk_mse" | "vkmse" | "вк_на_мсэ" | "мсэ" | "медико_социальная_экспертиза" => {
            "vk_mse".into()
        }
        "reception"
        | "reception_inspection"
        | "receptioninspection"
        | "admission_doctor_referral"
        | "admissiondoctorreferral"
        | "осмотр_врача_приемного_покоя"
        | "осмотр_врача_приёмного_покоя" => "reception".into(),
        "primary"
        | "primaryinspection"
        | "первичный_осмотр"
        | "направление_на_госпитализацию"
        | "направление" => "primary".into(),
        _ => normalized,
    }
}

fn normalize_role_slug(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Quick options belong to the concrete Medical profile, never to the universal
/// popup engine. They are intentionally editable because another organization
/// may use a completely different commissariat list.
pub fn rvk_commissariat_quick_options() -> Vec<String> {
    vec![
        "Ленинский".into(),
        "Канавинский".into(),
        "Сормовский и Московский".into(),
    ]
}

pub fn medical_profile() -> MedicalProfile {
    MedicalProfile {
        id: "medical".into(),
        field_rules: vec![
            FieldExtractionRule {
                field_id: "medical.case_number".into(),
                aliases: vec!["Номер истории болезни".into()],
                required: true,
            },
            FieldExtractionRule {
                field_id: "medical.diagnosis".into(),
                aliases: vec!["Диагноз".into()],
                required: true,
            },
            FieldExtractionRule {
                field_id: "medical.treatment".into(),
                aliases: vec!["Лечение".into()],
                required: false,
            },
        ],
    }
}

pub fn medical_discharge_workflow(
    button_id: &str,
    require_treatment: bool,
    sick_leave_enabled: bool,
) -> Workflow {
    let mut requires = vec![
        "medical.case_number".into(),
        "medical.discharge_date".into(),
    ];
    if require_treatment {
        requires.push("medical.treatment".into());
    }
    let optional = if sick_leave_enabled {
        vec!["medical.sick_leave_number".into()]
    } else {
        Vec::new()
    };
    Workflow {
        id: format!("medical:discharge:{button_id}"),
        button_id: button_id.into(),
        requires,
        optional,
        produces: vec!["docx".into()],
    }
}

#[cfg(test)]
mod donor_parity_tests {
    use super::*;

    #[test]
    fn canonical_roles_cover_commission_reception_and_sick_leave_workflows() {
        assert_eq!(canonical_medical_role("ВК больничный"), "sick_leave_vk");
        assert_eq!(
            canonical_medical_role("Осмотр врача приёмного покоя"),
            "reception"
        );
        assert_eq!(canonical_medical_role("Акт для РВК"), "rvk_act");
        assert_eq!(
            rvk_commissariat_quick_options(),
            vec!["Ленинский", "Канавинский", "Сормовский и Московский"]
        );
        for (legacy, canonical) in [
            ("primaryInspection", "primary"),
            ("dischargeEpicrisis", "discharge"),
            ("medicalDiary", "diaries"),
            ("rvkAct", "rvk_act"),
            ("jointMedicalExam", "commission"),
            ("commissionInspection", "commission"),
            ("sickLeaveVk", "sick_leave_vk"),
            ("vkMse", "vk_mse"),
            ("receptionInspection", "reception"),
            ("admission_doctor_referral", "reception"),
        ] {
            assert_eq!(
                canonical_medical_role(legacy),
                canonical,
                "legacy role {legacy}"
            );
        }
    }
}
