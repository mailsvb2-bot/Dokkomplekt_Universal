//! Completeness contract for generated documents.
//!
//! A document is not complete merely because all placeholders were substituted:
//! required semantic values must actually be visible in the rendered result.  The
//! Medical profile therefore consumes the same canonical role plan as popup and
//! workflow planning instead of maintaining another, drifting list of requirements.
//! Unknown/custom roles stay template-driven and never inherit legacy medical rules.

use crate::domains::medical_document_plan::{build_medical_render_plan, MedicalDocumentRole};
use crate::{title_for_field, DocumentTemplateSpec, DomainKind, SemanticCase};
use std::collections::BTreeSet;

/// How a required block is satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockRequirement {
    /// Every listed field must carry a value.
    AllFields(Vec<String>),
    /// At least one of the listed fields must carry a value.
    AnyField(Vec<String>),
    /// At least one field must carry a value and that value must be visible in the rendered document.
    AnyRenderedField(Vec<String>),
    /// The rendered document must contain this section header (case-insensitive).
    SectionMarker(String),
    /// The rendered document must contain a real signature line for one of the labels.
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
    fn any_rendered(id: &str, title: &str, fields: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            requirement: BlockRequirement::AnyRenderedField(
                fields.iter().map(|field| (*field).to_string()).collect(),
            ),
        }
    }

    fn rendered_field(field_id: &str) -> Self {
        Self {
            id: format!("rendered:{field_id}"),
            title: title_for_field(field_id),
            requirement: BlockRequirement::AnyRenderedField(vec![field_id.to_string()]),
        }
    }

    /// Public constructor for domain profiles that require all fields of a block.
    pub fn all(id: &str, title: &str, fields: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            requirement: BlockRequirement::AllFields(
                fields.iter().map(|field| (*field).to_string()).collect(),
            ),
        }
    }

    fn signature(id: &str, title: &str, labels: &[&str]) -> Self {
        Self {
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

/// Mandatory composite blocks for a configured document.
pub fn required_blocks_for(
    spec: &DocumentTemplateSpec,
    _template_text: &str,
) -> Vec<RequiredBlock> {
    if !matches!(spec.category, DomainKind::Medical) {
        return Vec::new();
    }

    let mut blocks = vec![RequiredBlock::any_rendered(
        "patient_identity",
        "Данные пациента (ФИО)",
        PATIENT_NAME_FIELDS,
    )];

    let role = MedicalDocumentRole::from_role_id(&spec.role_id);
    let plan = build_medical_render_plan(role.clone(), false, false);
    let mut required = plan.required_fields.into_iter().collect::<BTreeSet<_>>();

    // A template or a configured popup may legitimately add stricter fields than
    // the built-in role profile (including a conditional sick-leave number).  Those
    // requirements must also reach the post-render gate.
    required.extend(
        spec.required_fields
            .iter()
            .map(|field| crate::canonical_storage_field_id(field)),
    );

    for field_id in required {
        blocks.push(RequiredBlock::rendered_field(&field_id));
    }

    add_role_signature_blocks(&mut blocks, &role);
    blocks
}

fn add_role_signature_blocks(blocks: &mut Vec<RequiredBlock>, role: &MedicalDocumentRole) {
    match role {
        MedicalDocumentRole::DischargeEpicrisis => blocks.push(RequiredBlock::signature(
            "treating_physician_signature",
            "Подпись лечащего врача",
            &["лечащий врач", "врач-психиатр", "врач психиатр", "врач"],
        )),
        MedicalDocumentRole::Diary => {
            blocks.push(RequiredBlock::signature(
                "treating_physician_signature",
                "Подпись лечащего врача",
                &["лечащий врач", "врач-психиатр", "врач психиатр", "врач"],
            ));
            blocks.push(RequiredBlock::signature(
                "department_head_signature",
                "Подпись заведующего отделением",
                &["заведующий отделением", "зав. отделением", "зав отделением"],
            ));
        }
        _ => {}
    }
}

/// Titles of every block that is not satisfied for this case + rendered text.
pub fn unmet_blocks(
    blocks: &[RequiredBlock],
    case: &SemanticCase,
    rendered_text: &str,
) -> Vec<String> {
    let haystack = rendered_text.to_lowercase();
    let mut unmet = Vec::new();
    for block in blocks {
        let satisfied = match &block.requirement {
            BlockRequirement::AllFields(fields) => fields.iter().all(|field| case.has(field)),
            BlockRequirement::AnyField(fields) => fields.iter().any(|field| case.has(field)),
            BlockRequirement::AnyRenderedField(fields) => fields.iter().any(|field_id| {
                case.get(field_id)
                    .is_some_and(|value| rendered_contains_value(rendered_text, value))
            }),
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

fn rendered_contains_value(rendered_text: &str, value: &str) -> bool {
    let needle = normalize_visible_text(value);
    !needle.is_empty() && normalize_visible_text(rendered_text).contains(&needle)
}

fn normalize_visible_text(value: &str) -> String {
    value
        .replace('\u{00a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn contains_signature_line(rendered_text: &str, labels: &[String]) -> bool {
    rendered_text.lines().any(|line| {
        let normalized = normalize_visible_text(line);
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
        for (field_id, value) in pairs {
            values.insert(
                (*field_id).to_string(),
                SemanticValue {
                    field_id: (*field_id).to_string(),
                    value: (*value).to_string(),
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
    }

    #[test]
    fn medical_role_names_do_not_leak_into_nonmedical_domains() {
        assert!(required_blocks_for(&spec("discharge", DomainKind::Generic), "").is_empty());
        assert!(required_blocks_for(&spec("primary", DomainKind::Legal), "").is_empty());
    }

    #[test]
    fn every_known_medical_role_enforces_the_same_canonical_required_fields() {
        for role_id in [
            "primary",
            "discharge",
            "diaries",
            "rvk_act",
            "commission",
            "sick_leave_vk",
            "vk_mse",
            "reception",
        ] {
            let blocks = required_blocks_for(&spec(role_id, DomainKind::Medical), "");
            let plan =
                build_medical_render_plan(MedicalDocumentRole::from_role_id(role_id), false, false);
            for field_id in plan.required_fields {
                assert!(
                    blocks.iter().any(|block| {
                        matches!(
                            &block.requirement,
                            BlockRequirement::AnyRenderedField(fields)
                                if fields == &vec![field_id.clone()]
                        )
                    }),
                    "{role_id}: completeness gate misses {field_id}"
                );
            }
        }
    }

    #[test]
    fn unknown_medical_role_keeps_only_patient_and_template_requirements() {
        let mut document = spec("unknown", DomainKind::Medical);
        let blocks = required_blocks_for(&document, "");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "patient_identity");

        document.required_fields = vec!["custom.special_value".into()];
        let blocks = required_blocks_for(&document, "");
        assert!(blocks
            .iter()
            .any(|block| block.id == "rendered:custom.special_value"));
    }

    #[test]
    fn special_document_cannot_pass_when_required_value_never_reaches_render() {
        let blocks = required_blocks_for(&spec("vk_mse", DomainKind::Medical), "");
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "12345"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.diagnosis", "F20.0"),
            ("medical.treatment", "Терапия"),
            ("medical.commission_date", "10.06.2026"),
            ("medical.protocol_number", "77"),
            ("medical.protocol_date", "10.06.2026"),
            ("medical.workplace", "ООО Пример"),
            ("medical.position", "Инженер"),
        ]);
        let rendered = concat!(
            "Иванов Иван\nИстория болезни 12345\nПоступил 01.06.2026\n",
            "Диагноз F20.0\nЛечение Терапия\nДата комиссии 10.06.2026\n",
            "Дата протокола 10.06.2026\nООО Пример\nИнженер"
        );
        let unmet = unmet_blocks(&blocks, &case, rendered);
        assert_eq!(
            unmet,
            vec![title_for_field("medical.vk_mse.protocol_number")]
        );
    }

    #[test]
    fn discharge_requires_every_semantic_value_and_signature_to_be_rendered() {
        let blocks = required_blocks_for(&spec("discharge", DomainKind::Medical), "");
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "12345"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let rendered = concat!(
            "Пациент Иванов Иван\nИстория болезни 12345\nДиагноз J06.9\n",
            "Лечение Терапия\nДата поступления 01.06.2026\n",
            "Дата выписки 12.06.2026\nЛечащий врач ______"
        );
        assert!(unmet_blocks(&blocks, &case, rendered).is_empty());

        let missing_treatment = rendered.replace("Лечение Терапия\n", "");
        assert_eq!(
            unmet_blocks(&blocks, &case, &missing_treatment),
            vec![title_for_field("medical.treatment")]
        );
    }

    #[test]
    fn narrative_doctor_mention_is_not_a_signature() {
        let blocks = required_blocks_for(&spec("discharge", DomainKind::Medical), "");
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "12345"),
            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let rendered = concat!(
            "Иванов Иван 12345 J06.9 Терапия 01.06.2026 12.06.2026\n",
            "Лечащий врач осмотрел пациента и продолжил наблюдение."
        );
        let unmet = unmet_blocks(&blocks, &case, rendered);
        assert!(unmet.iter().any(|title| title == "Подпись лечащего врача"));
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
        let values = concat!(
            "Иванов Иван\nF20.0\n01.06.2026\n12.06.2026\n",
            "Лечащий врач __________________"
        );
        let unmet = unmet_blocks(&blocks, &case, values);
        assert_eq!(unmet, vec!["Подпись заведующего отделением".to_string()]);

        let both = format!("{values}\nЗаведующий отделением __________");
        assert!(unmet_blocks(&blocks, &case, &both).is_empty());
    }

    #[test]
    fn all_fields_block_requires_every_listed_field() {
        let block = RequiredBlock::all("req", "Реквизиты организации", &["org.inn", "org.kpp"]);
        let partial = case_with(&[("org.inn", "7736050003")]);
        assert!(!unmet_blocks(std::slice::from_ref(&block), &partial, "").is_empty());
        let complete = case_with(&[("org.inn", "7736050003"), ("org.kpp", "773601001")]);
        assert!(unmet_blocks(&[block], &complete, "").is_empty());
    }
}
