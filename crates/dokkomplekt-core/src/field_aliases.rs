//! Canonical semantic field identifiers and compatibility aliases.
//!
//! The project has accumulated several generations of field ids.  Keeping those
//! ids as independent values is dangerous: a parser can fill one id while a
//! template asks for another one with the same meaning.  This module defines a
//! single storage id for true synonyms and a much smaller set of read-only
//! fallbacks for document-specific convenience ids.

/// Return the canonical storage id for aliases that are semantically identical.
///
/// This function intentionally does **not** collapse role-dependent fields such
/// as `subject.name` and `employee.name`, or `document.number` and
/// `contract.number`. Those values can legitimately differ in one case.
pub fn canonical_storage_field_id(raw: &str) -> String {
    let field = raw.trim();
    match field {
        "medical.diagnosis_code" => "medical.icd10".into(),
        "organization.name" => "org.name".into(),
        "organization.inn" | "accounting.inn" | "company.inn" => "org.inn".into(),
        "organization.kpp" | "accounting.kpp" | "company.kpp" => "org.kpp".into(),
        "subject.full_name" | "person.full_name" | "patient.fio" | "patient.full_name" => {
            "subject.name".into()
        }
        "person.birth_date" | "patient.birth_date" => "subject.birth_date".into(),
        "person.address" | "patient.address" => "subject.address".into(),
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
        "accounting.amount_total" => "amount.total".into(),
        "accounting.currency" => "amount.currency".into(),
        _ => field.to_string(),
    }
}

/// Every historical storage id that is equivalent to the requested field.
/// The canonical id is always the first item.
pub fn storage_equivalent_field_ids(raw: &str) -> &'static [&'static str] {
    match canonical_storage_field_id(raw).as_str() {
        "medical.icd10" => &["medical.icd10", "medical.diagnosis_code"],
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
        "amount.total" => &["amount.total", "accounting.amount_total"],
        "amount.currency" => &["amount.currency", "accounting.currency"],
        _ => &[],
    }
}

/// Read-only convenience fallbacks. These are not storage aliases because both
/// ids may coexist with different values in one multi-document case.
pub fn contextual_fallback_field_ids(raw: &str) -> &'static [&'static str] {
    match raw.trim() {
        "accounting.invoice_number" => &["document.number"],
        "accounting.invoice_date" => &["document.date"],
        "document.number" => &["accounting.invoice_number"],
        "document.date" => &["accounting.invoice_date"],
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
    }
}
