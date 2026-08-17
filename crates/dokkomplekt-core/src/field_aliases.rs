//! Canonical semantic field identifiers and compatibility aliases.
//!
//! The project has accumulated several generations of field ids. Keeping those
//! ids as independent values is dangerous: a parser can fill one id while a
//! template asks for another one with the same meaning. This module defines a
//! single storage id for true synonyms and a smaller set of read-only fallbacks
//! for document-specific convenience ids.

/// Return the canonical storage id for aliases that are semantically identical.
/// Role-dependent fields are intentionally never collapsed here.
pub fn canonical_storage_field_id(raw: &str) -> String {
    let field = raw.trim();
    match field {
        "diagnosis.main" => "medical.diagnosis".into(),
        "diagnosis.icd10" | "icd10" | "medical.diagnosis_code" => "medical.icd10".into(),
        "organization.name" => "org.name".into(),
        "organization.inn" | "accounting.inn" | "company.inn" => "org.inn".into(),
        "organization.kpp" | "accounting.kpp" | "company.kpp" => "org.kpp".into(),
        "subject.full_name" | "person.full_name" | "patient.fio" | "patient.full_name" => {
            "subject.name".into()
        }
        "person.birth_date" | "patient.birth_date" => "subject.birth_date".into(),
        "person.address" | "patient.address" => "subject.address".into(),
        "person.age" | "patient.age" => "subject.age".into(),
        "complaints" | "medical.complaints_text" => "medical.complaints".into(),
        "anamnesis.disease" | "disease_anamnesis" => "medical.anamnesis_disease".into(),
        "anamnesis.life" | "life_anamnesis" => "medical.anamnesis_life".into(),
        "profile_observation" | "psych_account" | "medical.psych_account" => {
            "medical.profile_observation".into()
        }
        "rvk_referral" => "medical.rvk_referral".into(),
        "epidemiology" => "medical.epidemiology".into(),
        "status.objective" | "status.somatic" | "somatic_status" => "medical.somatic_status".into(),
        "status.profile" | "status.mental" | "mental_status" => "medical.profile_status".into(),
        "examination.plan" | "examination_plan" => "medical.examination_plan".into(),
        "treatment.result" => "medical.treatment_result".into(),
        "condition.discharge" => "medical.discharge_condition".into(),
        "labs.results" | "labs.block" | "analysis.results" | "analyses.results" => {
            "medical.labs".into()
        }
        "labs.date" => "medical.labs_date".into(),
        "labs.source" => "medical.labs_source".into(),
        "labs.date_policy" => "medical.labs_date_policy".into(),
        "hr.employee_name" => "employee.name".into(),
        "hr.position" => "employee.position".into(),
        "hr.department" => "employee.department".into(),
        "hr.salary" => "employee.salary".into(),
        "legal.contract_number" => "contract.number".into(),
        "legal.contract_date" => "contract.date".into(),
        "legal.subject" => "contract.subject".into(),
        "legal.amount" => "contract.amount".into(),
        "legal.party_a" => "contract.party_a".into(),
        "legal.party_b" => "contract.party_b".into(),
        "accounting.client" => "counterparty.name".into(),
        "accounting.amount_total" => "amount.total".into(),
        "accounting.currency" => "amount.currency".into(),
        _ => field.to_string(),
    }
}

/// Every historical storage id that is equivalent to the requested field.
pub fn storage_equivalent_field_ids(raw: &str) -> &'static [&'static str] {
    match canonical_storage_field_id(raw).as_str() {
        "medical.diagnosis" => &["medical.diagnosis", "diagnosis.main"],
        "medical.icd10" => &[
            "medical.icd10",
            "medical.diagnosis_code",
            "diagnosis.icd10",
            "icd10",
        ],
        "org.name" => &["org.name", "organization.name"],
        "org.inn" => &[
            "org.inn",
            "organization.inn",
            "accounting.inn",
            "company.inn",
        ],
        "org.kpp" => &[
            "org.kpp",
            "organization.kpp",
            "accounting.kpp",
            "company.kpp",
        ],
        "subject.name" => &[
            "subject.name",
            "subject.full_name",
            "person.full_name",
            "patient.fio",
            "patient.full_name",
        ],
        "subject.birth_date" => &[
            "subject.birth_date",
            "person.birth_date",
            "patient.birth_date",
        ],
        "subject.address" => &["subject.address", "person.address", "patient.address"],
        "subject.age" => &["subject.age", "person.age", "patient.age"],
        "medical.complaints" => &[
            "medical.complaints",
            "complaints",
            "medical.complaints_text",
        ],
        "medical.anamnesis_disease" => &[
            "medical.anamnesis_disease",
            "anamnesis.disease",
            "disease_anamnesis",
        ],
        "medical.anamnesis_life" => &["medical.anamnesis_life", "anamnesis.life", "life_anamnesis"],
        "medical.profile_observation" => &[
            "medical.profile_observation",
            "profile_observation",
            "psych_account",
            "medical.psych_account",
        ],
        "medical.rvk_referral" => &["medical.rvk_referral", "rvk_referral"],
        "medical.epidemiology" => &["medical.epidemiology", "epidemiology"],
        "medical.somatic_status" => &[
            "medical.somatic_status",
            "status.objective",
            "status.somatic",
            "somatic_status",
        ],
        "medical.profile_status" => &[
            "medical.profile_status",
            "status.profile",
            "status.mental",
            "mental_status",
        ],
        "medical.examination_plan" => &[
            "medical.examination_plan",
            "examination.plan",
            "examination_plan",
        ],
        "medical.treatment_result" => &["medical.treatment_result", "treatment.result"],
        "medical.discharge_condition" => &["medical.discharge_condition", "condition.discharge"],
        "medical.labs" => &[
            "medical.labs",
            "labs.results",
            "labs.block",
            "analysis.results",
            "analyses.results",
        ],
        "medical.labs_date" => &["medical.labs_date", "labs.date"],
        "medical.labs_source" => &["medical.labs_source", "labs.source"],
        "medical.labs_date_policy" => &["medical.labs_date_policy", "labs.date_policy"],
        "employee.name" => &["employee.name", "hr.employee_name"],
        "employee.position" => &["employee.position", "hr.position"],
        "employee.department" => &["employee.department", "hr.department"],
        "employee.salary" => &["employee.salary", "hr.salary"],
        "contract.number" => &["contract.number", "legal.contract_number"],
        "contract.date" => &["contract.date", "legal.contract_date"],
        "contract.subject" => &["contract.subject", "legal.subject"],
        "contract.amount" => &["contract.amount", "legal.amount"],
        "contract.party_a" => &["contract.party_a", "legal.party_a"],
        "contract.party_b" => &["contract.party_b", "legal.party_b"],
        "counterparty.name" => &["counterparty.name", "accounting.client"],
        "amount.total" => &["amount.total", "accounting.amount_total"],
        "amount.currency" => &["amount.currency", "accounting.currency"],
        _ => &[],
    }
}

/// Read-only convenience fallbacks. These are not storage aliases because both
/// ids may coexist with different values in one multi-document case. Scoped
/// Medical values therefore fall back to old generic storage only for migration;
/// the inverse direction is handled per document at render time.
pub fn contextual_fallback_field_ids(raw: &str) -> &'static [&'static str] {
    match raw.trim() {
        "accounting.invoice_number" => &["document.number"],
        "accounting.invoice_date" => &["document.date"],
        "document.number" => &["accounting.invoice_number"],
        "document.date" => &["accounting.invoice_date"],
        "medical.vk_mse.commission_date" | "medical.sick_leave_vk.commission_date" => {
            &["medical.commission_date"]
        }
        "medical.vk_mse.protocol_number" | "medical.sick_leave_vk.protocol_number" => {
            &["medical.protocol_number"]
        }
        "medical.vk_mse.protocol_date" | "medical.sick_leave_vk.protocol_date" => {
            &["medical.protocol_date"]
        }
        "medical.vk_mse.workplace" | "medical.sick_leave_vk.workplace" => &["medical.workplace"],
        "medical.vk_mse.position" | "medical.sick_leave_vk.position" => &["medical.position"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_synonyms_share_one_storage_id() {
        assert_eq!(
            canonical_storage_field_id("medical.diagnosis_code"),
            "medical.icd10"
        );
        assert_eq!(canonical_storage_field_id("organization.name"), "org.name");
        assert_eq!(
            canonical_storage_field_id("hr.employee_name"),
            "employee.name"
        );
        assert_eq!(
            canonical_storage_field_id("legal.contract_number"),
            "contract.number"
        );
        assert_eq!(
            canonical_storage_field_id("accounting.client"),
            "counterparty.name"
        );
        assert_eq!(
            storage_equivalent_field_ids("counterparty.name"),
            &["counterparty.name", "accounting.client"]
        );
    }

    #[test]
    fn role_dependent_fields_are_not_collapsed() {
        assert_eq!(canonical_storage_field_id("subject.name"), "subject.name");
        assert_eq!(canonical_storage_field_id("employee.name"), "employee.name");
        assert_eq!(
            canonical_storage_field_id("document.number"),
            "document.number"
        );
        assert_eq!(
            canonical_storage_field_id("contract.number"),
            "contract.number"
        );
        assert_eq!(
            canonical_storage_field_id("medical.vk_mse.protocol_number"),
            "medical.vk_mse.protocol_number"
        );
        assert_eq!(
            canonical_storage_field_id("medical.sick_leave_vk.protocol_number"),
            "medical.sick_leave_vk.protocol_number"
        );
    }

    #[test]
    fn role_scoped_medical_fields_can_read_legacy_values_without_collapsing_storage() {
        assert_eq!(
            contextual_fallback_field_ids("medical.vk_mse.protocol_number"),
            &["medical.protocol_number"]
        );
        assert_eq!(
            contextual_fallback_field_ids("medical.sick_leave_vk.protocol_number"),
            &["medical.protocol_number"]
        );
        assert!(contextual_fallback_field_ids("medical.protocol_number").is_empty());
    }
}
