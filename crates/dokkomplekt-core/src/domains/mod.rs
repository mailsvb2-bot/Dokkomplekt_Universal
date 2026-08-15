//! Domain plugins. The universal core is domain-neutral; all profession-specific language belongs here.

pub mod accounting;
pub mod custom;
pub use accounting::AccountingProfile;
pub mod education;
pub mod hr;
pub mod legal;
pub mod medical;

// Re-export types and uniquely named constructors only. Keep profile factory names qualified
// (domains::medical::medical_profile, etc.) to avoid glob collisions with legacy-compatible
// root modules kept for migration.
pub use custom::{custom_profile, CustomProfile};
pub use education::EducationProfile;
pub use hr::HrProfile;
pub use legal::LegalProfile;
pub use medical::{canonical_medical_role, medical_discharge_workflow, MedicalProfile};

pub mod medical_document_plan;
pub mod medical_semantics;
/// Build an ephemeral case for one document render. Profession-specific legacy
/// compatibility stays behind the domain boundary and never rewrites stored data.
pub fn case_for_document_render(
    case: &crate::SemanticCase,
    category: &crate::DomainKind,
    role_id: &str,
) -> crate::SemanticCase {
    match category {
        crate::DomainKind::Medical => {
            medical_semantics::case_for_medical_document_render(case, role_id)
        }
        _ => case.clone(),
    }
}

#[cfg(test)]
mod render_case_tests {
    use super::*;
    use crate::{SemanticCase, SemanticValue, ValueSource};

    fn put(case: &mut SemanticCase, field_id: &str, value: &str) {
        case.values.insert(
            field_id.to_string(),
            SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
        );
    }

    #[test]
    fn document_render_scopes_medical_role_without_medicalizing_other_domains() {
        let mut case = SemanticCase::default();
        put(
            &mut case,
            medical_semantics::VK_MSE_PROTOCOL_NUMBER,
            "MSE-10",
        );

        let medical = case_for_document_render(&case, &crate::DomainKind::Medical, "vk_mse");
        let legal = case_for_document_render(&case, &crate::DomainKind::Legal, "vk_mse");

        assert_eq!(medical.get("medical.protocol_number"), Some("MSE-10"));
        assert_eq!(legal.get("medical.protocol_number"), None);
        assert_eq!(case.get("medical.protocol_number"), None);
    }
}
