//! Universal, domain-neutral core.
//!
//! This module deliberately contains only generic constructor concepts:
//! document_type, field, required_field, source_document, target_template,
//! button, workflow, validation_rule and output_document.

pub mod document_generator;
pub mod field_extractor;
pub mod parser;
pub mod storage;
pub mod template_detector;
pub mod validation;
pub mod workflow_contract;

/// Compatibility namespace. The implementation lives only in `workflow_contract`;
/// new code must not add a second workflow engine here.
#[deprecated(
    note = "use core::workflow_contract; production popup planning is crate::workflow_engine"
)]
pub mod workflow_engine {
    pub use super::workflow_contract::*;
}

pub use document_generator::*;
pub use field_extractor::*;
pub use parser::*;
pub use storage::*;
pub use template_detector::*;
pub use validation::*;
pub use workflow_contract::*;
