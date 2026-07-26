use crate::{
    accounting_fields, education_fields, generic_fields, hr_fields, legal_fields, medical_fields,
    DomainKind, DomainProfile, WorkflowRule,
};

pub fn generic_profile() -> DomainProfile {
    DomainProfile {
        id: "generic".into(),
        title: "Универсальные документы".into(),
        kind: DomainKind::Generic,
        fields: generic_fields(),
        workflow_rules: vec![],
    }
}

pub fn medical_profile() -> DomainProfile {
    DomainProfile {
        id: "medical".into(),
        title: "Медицинский профиль".into(),
        kind: DomainKind::Medical,
        fields: medical_fields(),
        workflow_rules: vec![
            WorkflowRule::RequireField {
                document_role: "discharge".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "diaries".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "rvk_act".into(),
                field_id: "medical.discharge_date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "rvk_act".into(),
                field_id: "medical.rvk_commissariat".into(),
            },
            WorkflowRule::RequireFieldWhenFlag {
                document_role: "discharge".into(),
                field_id: "medical.sick_leave_number".into(),
                flag: "sick_leave_enabled".into(),
            },
            WorkflowRule::RequireFieldUnlessPresent {
                document_role: "discharge".into(),
                field_id: "medical.treatment".into(),
                unless_field: "medical.treatment".into(),
            },
            WorkflowRule::SkipForRole {
                document_role: "diaries".into(),
                field_id: "medical.treatment".into(),
            },
        ],
    }
}

pub fn legal_profile() -> DomainProfile {
    DomainProfile {
        id: "legal".into(),
        title: "Юридические документы".into(),
        kind: DomainKind::Legal,
        fields: legal_fields(),
        workflow_rules: require_many(&[
            (
                "contract",
                &[
                    "contract.number",
                    "contract.date",
                    "org.name",
                    "counterparty.name",
                    "contract.subject",
                ],
            ),
            (
                "acceptance_act",
                &[
                    "document.number",
                    "document.date",
                    "contract.number",
                    "org.name",
                    "counterparty.name",
                ],
            ),
            (
                "claim",
                &[
                    "document.number",
                    "document.date",
                    "org.name",
                    "counterparty.name",
                    "legal.claim_subject",
                ],
            ),
            (
                "cover_letter",
                &[
                    "document.number",
                    "document.date",
                    "org.name",
                    "counterparty.name",
                ],
            ),
        ]),
    }
}

pub fn hr_profile() -> DomainProfile {
    DomainProfile {
        id: "hr".into(),
        title: "Кадровые документы".into(),
        kind: DomainKind::Hr,
        fields: hr_fields(),
        workflow_rules: require_many(&[
            (
                "employment_contract",
                &[
                    "document.date",
                    "employee.name",
                    "employee.position",
                    "employee.hire_date",
                    "employee.contract_number",
                    "org.name",
                ],
            ),
            (
                "employment_order",
                &[
                    "hr.order_number",
                    "hr.order_date",
                    "employee.name",
                    "employee.position",
                    "employee.hire_date",
                ],
            ),
            (
                "personal_data_consent",
                &["document.date", "employee.name", "org.name"],
            ),
            (
                "familiarization_sheet",
                &[
                    "document.date",
                    "employee.name",
                    "employee.position",
                    "org.name",
                ],
            ),
        ]),
    }
}

pub fn education_profile() -> DomainProfile {
    DomainProfile {
        id: "education".into(),
        title: "Образовательные документы".into(),
        kind: DomainKind::Education,
        fields: education_fields(),
        workflow_rules: vec![
            WorkflowRule::RequireField {
                document_role: "certificate".into(),
                field_id: "education.student_name".into(),
            },
            WorkflowRule::RequireField {
                document_role: "grade_report".into(),
                field_id: "education.student_name".into(),
            },
            WorkflowRule::RequireField {
                document_role: "certificate".into(),
                field_id: "document.date".into(),
            },
            WorkflowRule::RequireField {
                document_role: "grade_report".into(),
                field_id: "education.course".into(),
            },
        ],
    }
}

pub fn accounting_profile() -> DomainProfile {
    DomainProfile {
        id: "accounting".into(),
        title: "Бухгалтерские документы".into(),
        kind: DomainKind::Accounting,
        fields: accounting_fields(),
        workflow_rules: require_many(&[
            (
                "invoice",
                &[
                    "accounting.invoice_number",
                    "accounting.invoice_date",
                    "accounting.client",
                    "accounting.amount_total",
                ],
            ),
            (
                "service_act",
                &[
                    "document.number",
                    "document.date",
                    "accounting.client",
                    "accounting.amount_total",
                ],
            ),
            ("reconciliation", &["document.date", "accounting.client"]),
        ]),
    }
}

fn require_many(specs: &[(&str, &[&str])]) -> Vec<WorkflowRule> {
    specs
        .iter()
        .flat_map(|(role, fields)| {
            fields.iter().map(move |field| WorkflowRule::RequireField {
                document_role: (*role).to_string(),
                field_id: (*field).to_string(),
            })
        })
        .collect()
}

pub fn builtin_profiles() -> Vec<DomainProfile> {
    vec![
        generic_profile(),
        medical_profile(),
        legal_profile(),
        hr_profile(),
        education_profile(),
        accounting_profile(),
    ]
}
