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
