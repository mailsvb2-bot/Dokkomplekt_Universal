use crate::core::{
    detect_template_structure, parse_source_document, workflow_contract::build_workflow, Button,
    OutputDocument, ParsedDocument, SourceDocument, TargetTemplate, TemplateStructure,
    ValidationRule, Workflow,
};
use crate::domains;
use crate::domains::medical_document_plan::{build_medical_render_plan, MedicalDocumentRole};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UniversalDomain {
    Medical,
    Legal,
    Hr,
    Education,
    Accounting,
    Custom,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniversalPipelineFlags {
    pub sick_leave_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniversalPipelineInput {
    pub source_document: SourceDocument,
    pub target_template: TargetTemplate,
    pub domain_hint: Option<UniversalDomain>,
    pub flags: UniversalPipelineFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniversalPipelineResult {
    pub parsed_source: ParsedDocument,
    pub template_structure: TemplateStructure,
    pub domain: UniversalDomain,
    pub button: Button,
    pub workflow: Workflow,
    pub validation_rules: Vec<ValidationRule>,
    pub output_document: OutputDocument,
}

pub fn run_universal_constructor_pipeline(
    input: UniversalPipelineInput,
) -> UniversalPipelineResult {
    let parsed_source = parse_source_document(&input.source_document);
    let template_structure = detect_template_structure(&input.target_template);
    let domain = input
        .domain_hint
        .unwrap_or_else(|| detect_domain_from_template(&template_structure));
    let role = canonical_role_for_domain(&domain, &template_structure.document_type);
    let required_fields =
        required_fields_for_domain(&domain, &role, &template_structure, &input.flags);
    let button = Button {
        id: format!("button:{}", input.target_template.id),
        label: template_structure.suggested_button_label.clone(),
        target_template_id: input.target_template.id.clone(),
        workflow_id: format!("workflow:{}", input.target_template.id),
    };
    let produces = if template_structure.fields.is_empty() && required_fields.is_empty() {
        "copy"
    } else {
        "docx"
    };
    let workflow = build_workflow(&button, required_fields.clone(), Vec::new(), produces);
    let validation_rules = required_fields
        .iter()
        .map(|field_id| ValidationRule {
            field_id: field_id.clone(),
            required: true,
            message: format!("Не заполнено обязательное поле: {field_id}"),
        })
        .collect::<Vec<_>>();
    let output_document = OutputDocument {
        id: format!("output:{}", input.target_template.id),
        document_type: workflow.id.clone(),
        filename: format!("{}.docx", sanitize_filename(&button.label)),
        content: input.target_template.text.clone(),
    };
    UniversalPipelineResult {
        parsed_source,
        template_structure,
        domain,
        button,
        workflow,
        validation_rules,
        output_document,
    }
}

pub fn required_fields_for_domain(
    domain: &UniversalDomain,
    role: &str,
    template: &TemplateStructure,
    flags: &UniversalPipelineFlags,
) -> Vec<String> {
    let role = canonical_role_for_domain(domain, role);
    let mut fields = BTreeSet::<String>::new();
    for field in &template.fields {
        if is_safe_generic_field_id(field) {
            fields.insert(field.clone());
        }
    }
    if matches!(domain, UniversalDomain::Medical) {
        fields.extend(medical_role_fields(&role, flags));
    } else {
        fields.extend(nonmedical_role_fields(domain, &role));
    }
    fields.into_iter().collect()
}

pub fn canonical_role_for_domain(domain: &UniversalDomain, raw_role: &str) -> String {
    match domain {
        UniversalDomain::Medical => domains::medical::canonical_medical_role(raw_role),
        UniversalDomain::Legal => domains::legal::canonical_legal_role(raw_role),
        UniversalDomain::Hr => domains::hr::canonical_hr_role(raw_role),
        UniversalDomain::Education => domains::education::canonical_education_role(raw_role),
        UniversalDomain::Accounting => domains::accounting::canonical_accounting_role(raw_role),
        UniversalDomain::Custom => raw_role.trim().to_lowercase(),
    }
}

pub fn canonical_role_for_category(category: &crate::DomainKind, raw_role: &str) -> Option<String> {
    let domain = match category {
        crate::DomainKind::Medical => UniversalDomain::Medical,
        crate::DomainKind::Legal => UniversalDomain::Legal,
        crate::DomainKind::Hr => UniversalDomain::Hr,
        crate::DomainKind::Education => UniversalDomain::Education,
        crate::DomainKind::Accounting => UniversalDomain::Accounting,
        crate::DomainKind::Generic | crate::DomainKind::Custom(_) => return None,
    };
    Some(canonical_role_for_domain(&domain, raw_role))
}

#[cfg(test)]
mod canonical_category_role_tests {
    use super::*;

    #[test]
    fn persisted_categories_reuse_the_universal_role_router() {
        assert_eq!(
            canonical_role_for_category(&crate::DomainKind::Medical, "dischargeEpicrisis")
                .as_deref(),
            Some("discharge")
        );
        assert_eq!(
            canonical_role_for_category(&crate::DomainKind::Accounting, "Счёт на оплату")
                .as_deref(),
            Some("invoice")
        );
        assert_eq!(
            canonical_role_for_category(&crate::DomainKind::Generic, "MyRole"),
            None
        );
        assert_eq!(
            canonical_role_for_category(&crate::DomainKind::Custom("clinic-x".into()), "MyRole"),
            None
        );
    }
}

fn nonmedical_role_fields(domain: &UniversalDomain, role: &str) -> Vec<String> {
    let fields: &[&str] = match (domain, role) {
        (UniversalDomain::Legal, "contract") => &[
            "contract.number",
            "contract.date",
            "contract.party_a",
            "contract.party_b",
        ],
        (UniversalDomain::Legal, "acceptance_act") => &[
            "document.number",
            "document.date",
            "contract.number",
            "contract.date",
            "contract.party_a",
            "contract.party_b",
        ],
        (UniversalDomain::Legal, "claim") => &[
            "document.number",
            "document.date",
            "contract.party_a",
            "contract.party_b",
            "legal.claim_subject",
        ],
        (UniversalDomain::Legal, "cover_letter") => &[
            "document.number",
            "document.date",
            "org.name",
            "counterparty.name",
        ],
        (UniversalDomain::Hr, "employment_contract") => &[
            "document.date",
            "org.name",
            "employee.name",
            "employee.position",
            "employee.hire_date",
            "employee.contract_number",
        ],
        (UniversalDomain::Hr, "employment_order") => &[
            "hr.order_number",
            "hr.order_date",
            "employee.name",
            "employee.position",
            "employee.hire_date",
        ],
        (UniversalDomain::Hr, "personal_data_consent") => {
            &["document.date", "org.name", "employee.name"]
        }
        (UniversalDomain::Hr, "familiarization_sheet") => &[
            "document.date",
            "org.name",
            "employee.name",
            "employee.position",
        ],
        (UniversalDomain::Education, "certificate") => &[
            "document.number",
            "document.date",
            "education.student_name",
            "education.institution",
        ],
        (UniversalDomain::Education, "grade_report") => &[
            "document.date",
            "education.student_name",
            "education.group",
            "education.course",
            "education.grade",
        ],
        (UniversalDomain::Accounting, "invoice") => &[
            "accounting.invoice_number",
            "accounting.invoice_date",
            "org.name",
            "counterparty.name",
            "amount.total",
        ],
        (UniversalDomain::Accounting, "service_act") => &[
            "document.number",
            "document.date",
            "org.name",
            "counterparty.name",
            "amount.total",
        ],
        (UniversalDomain::Accounting, "reconciliation") => {
            &["document.date", "org.name", "counterparty.name"]
        }
        _ => &[],
    };
    fields.iter().map(|field| (*field).to_string()).collect()
}

fn medical_role_fields(role: &str, flags: &UniversalPipelineFlags) -> Vec<String> {
    build_medical_render_plan(
        MedicalDocumentRole::from_role_id(role),
        flags.sick_leave_enabled,
        false,
    )
    .required_fields
}

fn detect_domain_from_template(template: &TemplateStructure) -> UniversalDomain {
    let haystack = format!(
        "{} {} {}",
        template.title,
        template.document_type,
        template.fields.join(" ")
    )
    .to_lowercase()
    .replace('ё', "е");
    if haystack.contains("medical.")
        || haystack.contains("выпис")
        || haystack.contains("дневник")
        || haystack.contains("диагноз")
        || haystack.contains("больнич")
        || haystack.contains("приемного покоя")
        || haystack.contains("мсэ")
        || haystack.contains("рвк")
        || haystack.contains("комисс")
    {
        UniversalDomain::Medical
    } else if haystack.contains("contract.")
        || haystack.contains("договор")
        || haystack.contains("заказчик")
    {
        UniversalDomain::Legal
    } else if haystack.contains("employee.")
        || haystack.contains("приказ")
        || haystack.contains("сотрудник")
    {
        UniversalDomain::Hr
    } else if haystack.contains("education.")
        || haystack.contains("студент")
        || haystack.contains("учащ")
    {
        UniversalDomain::Education
    } else if haystack.contains("accounting.")
        || haystack.contains("amount.")
        || haystack.contains("счет")
        || haystack.contains("акт сверки")
    {
        UniversalDomain::Accounting
    } else {
        UniversalDomain::Custom
    }
}

fn is_safe_generic_field_id(field_id: &str) -> bool {
    let trimmed = field_id.trim();
    !trimmed.is_empty()
        && !trimmed.contains("..")
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
        })
}

fn sanitize_filename(value: &str) -> String {
    let out = value
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if out.is_empty() {
        "document".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn generic_pipeline_uses_core_and_domain_plugins() {
        let input = UniversalPipelineInput {
            source_document: SourceDocument {
                id: "s".into(),
                text: "Первичный документ".into(),
                metadata: BTreeMap::new(),
            },
            target_template: TargetTemplate {
                id: "t".into(),
                path: "tpl.docx".into(),
                text: "Выписной эпикриз\n{{medical.case_number}}\n{{medical.diagnosis}}".into(),
            },
            domain_hint: None,
            flags: UniversalPipelineFlags {
                sick_leave_enabled: true,
            },
        };
        let result = run_universal_constructor_pipeline(input);
        assert_eq!(result.domain, UniversalDomain::Medical);
        assert!(result
            .workflow
            .requires
            .contains(&"medical.discharge_date".to_string()));
        assert!(result
            .workflow
            .requires
            .contains(&"medical.sick_leave_number".to_string()));
        assert_eq!(result.workflow.produces, vec!["docx".to_string()]);
    }

    #[test]
    fn medical_role_slug_maps_to_canonical_discharge() {
        let structure = TemplateStructure {
            title: "Выписной эпикриз".into(),
            document_type: "выписной_эпикриз".into(),
            fields: vec!["medical.case_number".into(), "medical.diagnosis".into()],
            repeated_fields: Vec::new(),
            tables: Vec::new(),
            signatures: Vec::new(),
            input_zones: Vec::new(),
            suggested_button_label: "Выписной эпикриз".into(),
        };
        let fields = required_fields_for_domain(
            &UniversalDomain::Medical,
            &structure.document_type,
            &structure,
            &UniversalPipelineFlags {
                sick_leave_enabled: true,
            },
        );
        assert!(fields.contains(&"medical.discharge_date".to_string()));
        assert!(fields.contains(&"medical.sick_leave_number".to_string()));
    }

    #[test]
    fn medical_diary_slug_excludes_case_treatment_and_sick_leave_unless_template_uses_them() {
        let structure = TemplateStructure {
            title: "Дневники".into(),
            document_type: "дневники".into(),
            fields: vec!["medical.diagnosis".into()],
            repeated_fields: Vec::new(),
            tables: Vec::new(),
            signatures: Vec::new(),
            input_zones: Vec::new(),
            suggested_button_label: "Дневники".into(),
        };
        let fields = required_fields_for_domain(
            &UniversalDomain::Medical,
            &structure.document_type,
            &structure,
            &UniversalPipelineFlags {
                sick_leave_enabled: true,
            },
        );
        assert!(fields.contains(&"medical.admission_date".to_string()));
        assert!(fields.contains(&"medical.discharge_date".to_string()));
        assert!(!fields.contains(&"medical.case_number".to_string()));
        assert!(!fields.contains(&"medical.treatment".to_string()));
        assert!(!fields.contains(&"medical.sick_leave_number".to_string()));
    }

    #[test]
    fn every_medical_role_uses_the_same_canonical_plan_as_the_domain_contract() {
        let structure = |role: &str| TemplateStructure {
            title: role.into(),
            document_type: role.into(),
            fields: Vec::new(),
            repeated_fields: Vec::new(),
            tables: Vec::new(),
            signatures: Vec::new(),
            input_zones: Vec::new(),
            suggested_button_label: role.into(),
        };
        let flags = UniversalPipelineFlags {
            sick_leave_enabled: true,
        };
        for role in [
            "primary",
            "discharge",
            "diaries",
            "rvk_act",
            "commission",
            "sick_leave_vk",
            "vk_mse",
            "reception",
        ] {
            let fields = required_fields_for_domain(
                &UniversalDomain::Medical,
                role,
                &structure(role),
                &flags,
            );
            let expected = build_medical_render_plan(
                MedicalDocumentRole::from_role_id(role),
                flags.sick_leave_enabled,
                false,
            )
            .required_fields;
            assert_eq!(fields, expected, "role contract drifted for {role}");
        }
    }

    #[test]
    fn reception_is_medical_but_does_not_require_treatment() {
        let input = UniversalPipelineInput {
            source_document: SourceDocument {
                id: "s".into(),
                text: "Первичный документ".into(),
                metadata: BTreeMap::new(),
            },
            target_template: TargetTemplate {
                id: "t".into(),
                path: "reception.docx".into(),
                text: "Осмотр врача приёмного покоя\n{{medical.diagnosis}}".into(),
            },
            domain_hint: None,
            flags: UniversalPipelineFlags::default(),
        };
        let result = run_universal_constructor_pipeline(input);
        assert_eq!(result.domain, UniversalDomain::Medical);
        assert!(result
            .workflow
            .requires
            .contains(&"medical.admission_date".to_string()));
        assert!(!result
            .workflow
            .requires
            .contains(&"medical.treatment".to_string()));
    }

    #[test]
    fn accounting_invoice_requires_parties_and_total() {
        let structure = TemplateStructure {
            title: "Счёт на оплату".into(),
            document_type: "счёт".into(),
            fields: vec!["accounting.invoice_number".into()],
            repeated_fields: Vec::new(),
            tables: Vec::new(),
            signatures: Vec::new(),
            input_zones: Vec::new(),
            suggested_button_label: "Счёт на оплату".into(),
        };
        let fields = required_fields_for_domain(
            &UniversalDomain::Accounting,
            &structure.document_type,
            &structure,
            &UniversalPipelineFlags::default(),
        );
        for required in [
            "accounting.invoice_number",
            "accounting.invoice_date",
            "org.name",
            "counterparty.name",
            "amount.total",
        ] {
            assert!(fields.contains(&required.to_string()), "missing {required}");
        }
    }

    #[test]
    fn hr_employment_order_uses_hr_fields_without_medical_leakage() {
        let structure = TemplateStructure {
            title: "Приказ о приёме".into(),
            document_type: "приказ о приёме".into(),
            fields: Vec::new(),
            repeated_fields: Vec::new(),
            tables: Vec::new(),
            signatures: Vec::new(),
            input_zones: Vec::new(),
            suggested_button_label: "Приказ о приёме".into(),
        };
        let fields = required_fields_for_domain(
            &UniversalDomain::Hr,
            &structure.document_type,
            &structure,
            &UniversalPipelineFlags::default(),
        );
        assert!(fields.contains(&"hr.order_number".to_string()));
        assert!(fields.contains(&"employee.name".to_string()));
        assert!(fields.iter().all(|field| !field.starts_with("medical.")));
    }

    #[test]
    fn non_medical_pipeline_has_no_medical_fields() {
        let input = UniversalPipelineInput {
            source_document: SourceDocument {
                id: "s".into(),
                text: "Договор".into(),
                metadata: BTreeMap::new(),
            },
            target_template: TargetTemplate {
                id: "t".into(),
                path: "contract.docx".into(),
                text: "Договор\n{{contract.number}}\n{{contract.date}}\n{{contract.party_a}}"
                    .into(),
            },
            domain_hint: None,
            flags: UniversalPipelineFlags::default(),
        };
        let result = run_universal_constructor_pipeline(input);
        assert_eq!(result.domain, UniversalDomain::Legal);
        assert!(result
            .workflow
            .requires
            .iter()
            .all(|field| !field.starts_with("medical.")));
    }
}
