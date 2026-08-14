//! Completeness contract: the composite blocks a created document MUST contain.
//!
//! The zero-touch promise is stronger than "no unfilled `{{placeholder}}` remains":
//! a specialist drops one source document and expects the *whole* set to come out
//! complete and correction-free. A template can be fully substituted yet still be an
//! incomplete document of its kind — an epicrisis with no diagnosis, a diary with no
//! signature section. This module encodes, per document role, the mandatory composite
//! blocks such a document must contain, so an incomplete result is safely routed to
//! `*_ТРЕБУЕТ_ВНИМАНИЯ.txt` instead of being emitted.
//!
//! Design notes:
//! * The registry is data, keyed by the detected `role_id`/domain — easy to extend
//!   with new document kinds without touching logic.
//! * Generic/unknown roles declare **no** mandatory blocks, so the layer stays
//!   domain-neutral and never over-blocks a plain user template.
//! * A block is satisfied either by case data (fields) or by the presence of a
//!   section header in the rendered text (markers). Checks lean toward a safe stop:
//!   when a required block cannot be confirmed, nothing is created.

use crate::{DocumentTemplateSpec, DomainKind, SemanticCase};

/// How a required block is satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockRequirement {
    /// Every listed field must carry a value.
    AllFields(Vec<String>),
    /// At least one of the listed fields must carry a value.
    AnyField(Vec<String>),
    /// The rendered document must contain this section header (case-insensitive).
    SectionMarker(String),
    /// The rendered document must contain a real signature line for one of the labels.
    /// A narrative mention of a doctor is not sufficient.
    SignatureLine(Vec<String>),
}

/// A mandatory composite block of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredBlock {
    pub id: String,
    pub title: String,
    pub requirement: BlockRequirement,
}

impl RequiredBlock {
    fn any(id: &str, title: &str, fields: &[&str]) -> Self {
        RequiredBlock {
            id: id.to_string(),
            title: title.to_string(),
            requirement: BlockRequirement::AnyField(fields.iter().map(|s| s.to_string()).collect()),
        }
    }

    /// Public constructor for [`BlockRequirement::AllFields`]: domain profiles that
    /// require every field of a block (e.g. полный набор реквизитов организации)
    /// build their blocks with it. Exercised by the unit test below.
    pub fn all(id: &str, title: &str, fields: &[&str]) -> Self {
        RequiredBlock {
            id: id.to_string(),
            title: title.to_string(),
            requirement: BlockRequirement::AllFields(
                fields.iter().map(|s| s.to_string()).collect(),
            ),
        }
    }

    fn signature(id: &str, title: &str, labels: &[&str]) -> Self {
        RequiredBlock {
            id: id.to_string(),
            title: title.to_string(),
            requirement: BlockRequirement::SignatureLine(
                labels.iter().map(|label| (*label).to_string()).collect(),
            ),
        }
    }
}

const PATIENT_NAME_FIELDS: &[&str] = &[
    "subject.name",
    "subject.full_name",
    "patient.name",
    "patient.full_name",
];
const DIAGNOSIS_FIELDS: &[&str] = &[
    "medical.diagnosis",
    "medical.diagnosis_main",
    "diagnosis.main",
    "diagnosis",
];
const TREATMENT_FIELDS: &[&str] = &[
    "medical.treatment",
    "medical.treatment_plan",
    "treatment.plan",
    "treatment",
];
const ADMISSION_DATE_FIELDS: &[&str] = &["medical.admission_date", "period.start_date"];
const DISCHARGE_DATE_FIELDS: &[&str] = &["medical.discharge_date", "period.end_date"];

/// The mandatory composite blocks for a configured document.
///
/// `template_text` is accepted for future template-driven inference; today the
/// contract is driven by the detected role and domain.
pub fn required_blocks_for(
    spec: &DocumentTemplateSpec,
    _template_text: &str,
) -> Vec<RequiredBlock> {
    let mut blocks = if matches!(spec.category, DomainKind::Medical) {
        medical_role_blocks(&spec.role_id)
    } else {
        Vec::new()
    };

    // Domain safety net: any medical document must at least identify its patient.
    if matches!(spec.category, DomainKind::Medical)
        && !blocks.iter().any(|b| b.id == "patient_identity")
    {
        blocks.push(RequiredBlock::any(
            "patient_identity",
            "Данные пациента (ФИО)",
            PATIENT_NAME_FIELDS,
        ));
    }

    blocks
}

fn medical_role_blocks(role_id: &str) -> Vec<RequiredBlock> {
    // Role identifiers may be namespaced by a profile (`medical.discharge`).
    // Completeness semantics belong to the terminal role, not to spelling style.
    let role = role_id.rsplit('.').next().unwrap_or(role_id);
    match role {
        "discharge" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("treatment", "Лечение", TREATMENT_FIELDS),
            RequiredBlock::any("admission_date", "Дата поступления", ADMISSION_DATE_FIELDS),
            RequiredBlock::any("discharge_date", "Дата выписки", DISCHARGE_DATE_FIELDS),
            RequiredBlock::signature(
                "treating_physician_signature",
                "Подпись лечащего врача",
                &["лечащий врач", "врач-психиатр", "врач психиатр"],
            ),
        ],
        "diaries" | "diary" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("admission_date", "Дата поступления", ADMISSION_DATE_FIELDS),
            RequiredBlock::any("discharge_date", "Дата выписки", DISCHARGE_DATE_FIELDS),
            RequiredBlock::signature(
                "treating_physician_signature",
                "Подпись лечащего врача",
                &["лечащий врач", "врач-психиатр", "врач психиатр"],
            ),
            RequiredBlock::signature(
                "department_head_signature",
                "Подпись заведующего отделением",
                &["заведующий отделением", "зав. отделением", "зав отделением"],
            ),
        ],
        "primary" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("treatment", "Лечение", TREATMENT_FIELDS),
        ],
        "rvk_act" | "vk_mse" | "commission" => vec![RequiredBlock::any(
            "patient_identity",
            "Данные освидетельствуемого (ФИО)",
            PATIENT_NAME_FIELDS,
        )],
        _ => Vec::new(),
    }
}

/// Titles of every block that is **not** satisfied for this case + rendered text.
/// An empty result means the document is structurally complete.
pub fn unmet_blocks(
    blocks: &[RequiredBlock],
    case: &SemanticCase,
    rendered_text: &str,
) -> Vec<String> {
    let haystack = rendered_text.to_lowercase();
    let mut unmet = Vec::new();
    for block in blocks {
        let satisfied = match &block.requirement {
            BlockRequirement::AllFields(fields) => fields.iter().all(|f| case.has(f)),
            BlockRequirement::AnyField(fields) => fields.iter().any(|f| case.has(f)),
            BlockRequirement::SectionMarker(marker) => haystack.contains(&marker.to_lowercase()),
            BlockRequirement::SignatureLine(labels) => {
                contains_signature_line(rendered_text, labels)
            }
        };
        if !satisfied {
            unmet.push(block.title.clone());
        }
    }
    unmet
}

fn contains_signature_line(rendered_text: &str, labels: &[String]) -> bool {
    rendered_text.lines().any(|line| {
        let normalized = line
            .replace('\u{00a0}', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if normalized.is_empty() {
            return false;
        }
        let has_signature_cue = normalized.contains("___")
            || normalized.contains("подпись")
            || normalized.contains("/____")
            || normalized.contains("м.п.");
        has_signature_cue
            && labels
                .iter()
                .any(|label| normalized.contains(&label.to_lowercase()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};
    use std::collections::BTreeMap;

    fn spec(role: &str, category: DomainKind) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "d".into(),
            button_label: "Док".into(),
            template_path: "t.docx".into(),
            category,
            role_id: role.into(),
            required_fields: vec![],
            placeholders: vec![],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    fn case_with(pairs: &[(&str, &str)]) -> SemanticCase {
        let mut values = BTreeMap::new();
        for (k, v) in pairs {
            values.insert(
                (*k).to_string(),
                SemanticValue {
                    field_id: (*k).to_string(),
                    value: (*v).to_string(),
                    source: ValueSource::UserConfirmed,
                    confidence: 1.0,
                    evidence: Vec::new(),
                },
            );
        }
        SemanticCase {
            values,
            active_domains: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn generic_role_has_no_mandatory_blocks() {
        let blocks = required_blocks_for(&spec("generic", DomainKind::Generic), "любой текст");
        assert!(blocks.is_empty());
        assert!(unmet_blocks(&blocks, &SemanticCase::default(), "").is_empty());
    }

    #[test]
    fn medical_role_names_do_not_leak_into_nonmedical_domains() {
        let blocks = required_blocks_for(&spec("discharge", DomainKind::Generic), "");
        assert!(blocks.is_empty());
        let blocks = required_blocks_for(&spec("primary", DomainKind::Legal), "");
        assert!(blocks.is_empty());
    }

    #[test]
    fn discharge_requires_diagnosis_and_signature() {
        let blocks = required_blocks_for(&spec("discharge", DomainKind::Medical), "");
        // Patient present, diagnosis present, signature section present -> complete.
        let ok_case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let ok_text = "Диагноз: J06.9\nЛечащий врач ______";
        assert!(unmet_blocks(&blocks, &ok_case, ok_text).is_empty());

        // Missing clinical data and signature must all be surfaced, never hidden.
        let bad_case = case_with(&[("subject.name", "Иванов Иван")]);
        let unmet = unmet_blocks(&blocks, &bad_case, "Просто текст без подписи");
        assert!(unmet.iter().any(|t| t.contains("Диагноз")));
        assert!(unmet.iter().any(|t| t.contains("Лечение")));
        assert!(unmet.iter().any(|t| t.contains("Дата поступления")));
        assert!(unmet.iter().any(|t| t.contains("Дата выписки")));
        assert!(unmet.iter().any(|t| t.contains("Подпись лечащего врача")));
    }

    #[test]
    fn namespaced_discharge_requires_full_medical_contract() {
        let blocks = required_blocks_for(&spec("medical.discharge", DomainKind::Medical), "");
        let partial = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "J06.9"),
        ]);
        let unmet = unmet_blocks(&blocks, &partial, "Лечащий врач ______");
        assert!(unmet.iter().any(|title| title == "Лечение"));
        assert!(unmet.iter().any(|title| title == "Дата поступления"));
        assert!(unmet.iter().any(|title| title == "Дата выписки"));
    }

    #[test]
    fn primary_requires_diagnosis_and_treatment() {
        let blocks = required_blocks_for(&spec("medical.primary", DomainKind::Medical), "");
        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let unmet = unmet_blocks(&blocks, &case, "");
        assert!(unmet.iter().any(|title| title == "Диагноз"));
        assert!(unmet.iter().any(|title| title == "Лечение"));
    }

    #[test]
    fn narrative_doctor_mention_is_not_a_signature() {
        let blocks = required_blocks_for(&spec("discharge", DomainKind::Medical), "");
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let unmet = unmet_blocks(
            &blocks,
            &case,
            "Лечащий врач осмотрел пациента и продолжил наблюдение.",
        );
        assert!(unmet.iter().any(|title| title.contains("лечащего врача")));
    }

    #[test]
    fn diaries_require_both_signature_lines() {
        let blocks = required_blocks_for(&spec("diaries", DomainKind::Medical), "");
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "F20.0"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let one_signature = "Лечащий врач __________________";
        let unmet = unmet_blocks(&blocks, &case, one_signature);
        assert_eq!(unmet, vec!["Подпись заведующего отделением".to_string()]);

        let both = concat!(
            "Лечащий врач __________________\n",
            "Заведующий отделением __________"
        );
        assert!(unmet_blocks(&blocks, &case, both).is_empty());
    }

    #[test]
    fn all_fields_block_requires_every_listed_field() {
        let block = RequiredBlock::all("req", "Реквизиты организации", &["org.inn", "org.kpp"]);
        let partial = case_with(&[("org.inn", "7736050003")]);
        assert!(!unmet_blocks(std::slice::from_ref(&block), &partial, "").is_empty());
        let complete = case_with(&[("org.inn", "7736050003"), ("org.kpp", "773601001")]);
        assert!(unmet_blocks(&[block], &complete, "").is_empty());
    }

    #[test]
    fn medical_category_adds_patient_identity_even_for_unknown_role() {
        let blocks = required_blocks_for(&spec("unknown", DomainKind::Medical), "");
        assert!(blocks.iter().any(|b| b.id == "patient_identity"));
        let unmet = unmet_blocks(&blocks, &SemanticCase::default(), "текст");
        assert!(unmet.iter().any(|t| t.contains("ФИО")));
    }
}
