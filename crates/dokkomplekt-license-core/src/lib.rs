pub mod activation;
pub mod canonical;
pub mod clock;
pub mod core_error;
pub mod crypto;
pub mod machine;
pub mod models;
pub mod policy;
pub mod usage;

pub use activation::{evaluate_machine_activation, max_machines_for_plan, ActivationDecision};
pub use clock::{ClockGuard, ClockState};
pub use core_error::{CoreError, CoreResult};
pub use crypto::{
    verify_license_document_at, verify_license_document_now, verify_license_signature,
    PublicKeyBytes,
};
pub use machine::{MachineFacts, MachineFingerprint};
pub use models::{Feature, LicenseDocument, LicensePayload, PlanId, SignedLicense, WatermarkMode};
pub use policy::{
    evaluate_access, max_documents_per_run, AccessDecision, AccessRequest, AccessStatus,
};
pub use usage::{UsageCounter, UsageLedger};
