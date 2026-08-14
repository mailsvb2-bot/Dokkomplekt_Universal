//! Role-scoped semantic fields for medical documents whose labels are ambiguous.
//!
//! A single case can create both an MSE commission document and a sick-leave VK
//! document.  Their protocol number/date and workplace requisites are independent,
//! even though old templates often used the same human labels.  Persist the values
//! separately and adapt them to legacy generic placeholders only for the document
//! currently being rendered.

use crate::SemanticCase;

pub const VK_MSE_COMMISSION_DATE: &str = "medical.vk_mse.commission_date";
pub const VK_MSE_PROTOCOL_NUMBER: &str = "medical.vk_mse.protocol_number";
pub const VK_MSE_PROTOCOL_DATE: &str = "medical.vk_mse.protocol_date";
pub const VK_MSE_WORKPLACE: &str = "medical.vk_mse.workplace";
pub const VK_MSE_POSITION: &str = "medical.vk_mse.position";

pub const SICK_LEAVE_VK_COMMISSION_DATE: &str = "medical.sick_leave_vk.commission_date";
pub const SICK_LEAVE_VK_PROTOCOL_NUMBER: &str = "medical.sick_leave_vk.protocol_number";
pub const SICK_LEAVE_VK_PROTOCOL_DATE: &str = "medical.sick_leave_vk.protocol_date";
pub const SICK_LEAVE_VK_WORKPLACE: &str = "medical.sick_leave_vk.workplace";
pub const SICK_LEAVE_VK_POSITION: &str = "medical.sick_leave_vk.position";

const VK_MSE_BINDINGS: &[(&str, &str)] = &[
    (VK_MSE_COMMISSION_DATE, "medical.commission_date"),
    (VK_MSE_PROTOCOL_NUMBER, "medical.protocol_number"),
    (VK_MSE_PROTOCOL_DATE, "medical.protocol_date"),
    (VK_MSE_WORKPLACE, "medical.workplace"),
    (VK_MSE_POSITION, "medical.position"),
];

const SICK_LEAVE_VK_BINDINGS: &[(&str, &str)] = &[
    (SICK_LEAVE_VK_COMMISSION_DATE, "medical.commission_date"),
    (SICK_LEAVE_VK_PROTOCOL_NUMBER, "medical.protocol_number"),
    (SICK_LEAVE_VK_PROTOCOL_DATE, "medical.protocol_date"),
    (SICK_LEAVE_VK_WORKPLACE, "medical.workplace"),
    (SICK_LEAVE_VK_POSITION, "medical.position"),
];

pub fn role_scoped_bindings(role_id: &str) -> &'static [(&'static str, &'static str)] {
    match crate::domains::medical::canonical_medical_role(role_id).as_str() {
        "vk_mse" => VK_MSE_BINDINGS,
        "sick_leave_vk" => SICK_LEAVE_VK_BINDINGS,
        _ => &[],
    }
}

/// Convert a legacy generic medical field into the independent storage id for a
/// document role. Fields without role-dependent meaning are returned unchanged.
pub fn scope_legacy_field_for_role(role_id: &str, field_id: &str) -> String {
    role_scoped_bindings(role_id)
        .iter()
        .find_map(|(scoped, legacy)| (*legacy == field_id).then(|| (*scoped).to_string()))
        .unwrap_or_else(|| field_id.to_string())
}

/// Clone a case for one render and project role-specific values onto the legacy
/// generic ids used by older user templates. The persistent case remains unchanged.
/// Exact scoped values win; when only a legacy value exists no projection is made,
/// so backward-compatible reading continues to work.
pub fn case_for_medical_document_render(case: &SemanticCase, role_id: &str) -> SemanticCase {
    let mut scoped_case = case.clone();
    for (scoped_id, legacy_id) in role_scoped_bindings(role_id) {
        if let Some(mut value) = case.values.get(*scoped_id).cloned() {
            value.field_id = (*legacy_id).to_string();
            scoped_case.values.insert((*legacy_id).to_string(), value);
        }
        if case.skipped_fields.contains(*scoped_id) {
            scoped_case.skipped_fields.insert((*legacy_id).to_string());
        }
    }
    scoped_case
}

pub fn title_for_role_scoped_field(field_id: &str) -> Option<&'static str> {
    match field_id {
        VK_MSE_COMMISSION_DATE => Some("Дата ВК на МСЭ"),
        VK_MSE_PROTOCOL_NUMBER => Some("Номер протокола ВК на МСЭ"),
        VK_MSE_PROTOCOL_DATE => Some("Дата протокола ВК на МСЭ"),
        VK_MSE_WORKPLACE => Some("Место работы для ВК на МСЭ"),
        VK_MSE_POSITION => Some("Должность для ВК на МСЭ"),
        SICK_LEAVE_VK_COMMISSION_DATE => Some("Дата ВК по больничному"),
        SICK_LEAVE_VK_PROTOCOL_NUMBER => Some("Номер протокола ВК по больничному"),
        SICK_LEAVE_VK_PROTOCOL_DATE => Some("Дата протокола ВК по больничному"),
        SICK_LEAVE_VK_WORKPLACE => Some("Место работы для ВК по больничному"),
        SICK_LEAVE_VK_POSITION => Some("Должность для ВК по больничному"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    fn put(case: &mut SemanticCase, field_id: &str, value: &str) {
        case.values.insert(
            field_id.into(),
            SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
        );
    }

    #[test]
    fn mse_and_sick_leave_protocols_are_independent() {
        let mut case = SemanticCase::default();
        put(&mut case, VK_MSE_PROTOCOL_NUMBER, "MSE-10");
        put(&mut case, SICK_LEAVE_VK_PROTOCOL_NUMBER, "SL-20");

        let mse = case_for_medical_document_render(&case, "vk_mse");
        let sick = case_for_medical_document_render(&case, "sick_leave_vk");
        assert_eq!(mse.get("medical.protocol_number"), Some("MSE-10"));
        assert_eq!(sick.get("medical.protocol_number"), Some("SL-20"));
        assert_eq!(case.get("medical.protocol_number"), None);
    }

    #[test]
    fn legacy_generic_value_is_not_destroyed_when_scoped_value_is_missing() {
        let mut case = SemanticCase::default();
        put(&mut case, "medical.protocol_number", "OLD-77");
        let mse = case_for_medical_document_render(&case, "vk_mse");
        assert_eq!(mse.get("medical.protocol_number"), Some("OLD-77"));
    }

    #[test]
    fn only_ambiguous_fields_are_scoped() {
        assert_eq!(
            scope_legacy_field_for_role("vk_mse", "medical.protocol_number"),
            VK_MSE_PROTOCOL_NUMBER
        );
        assert_eq!(
            scope_legacy_field_for_role("reception", "medical.protocol_number"),
            "medical.protocol_number"
        );
        assert_eq!(
            scope_legacy_field_for_role("vk_mse", "medical.diagnosis"),
            "medical.diagnosis"
        );
    }
}