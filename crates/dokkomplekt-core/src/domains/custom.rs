use crate::core::FieldExtractionRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn custom_profile(id: &str, field_rules: Vec<FieldExtractionRule>) -> CustomProfile {
    CustomProfile {
        id: id.into(),
        field_rules,
    }
}
