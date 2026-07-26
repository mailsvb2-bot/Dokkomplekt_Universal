use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRule {
    pub field_id: String,
    pub required: bool,
    pub message: String,
}

pub fn validate_required_fields(
    values: &BTreeMap<String, String>,
    rules: &[ValidationRule],
) -> Result<(), Vec<String>> {
    let missing = rules
        .iter()
        .filter(|rule| rule.required)
        .filter(|rule| {
            values
                .get(&rule.field_id)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|rule| rule.message.clone())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}
