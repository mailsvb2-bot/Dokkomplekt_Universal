use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldExtractionRule {
    pub field_id: String,
    pub aliases: Vec<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredField {
    pub field_id: String,
    pub title: String,
}

pub fn extract_required_fields(rules: &[FieldExtractionRule]) -> Vec<RequiredField> {
    rules
        .iter()
        .filter(|rule| rule.required)
        .map(|rule| RequiredField {
            field_id: rule.field_id.clone(),
            title: rule
                .aliases
                .first()
                .cloned()
                .unwrap_or_else(|| rule.field_id.clone()),
        })
        .collect()
}
