use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputDocument {
    pub id: String,
    pub document_type: String,
    pub filename: String,
    pub content: String,
}

pub fn generate_output_document(
    template: &str,
    values: &BTreeMap<String, String>,
    document_type: &str,
    filename: &str,
) -> OutputDocument {
    let mut content = template.to_string();
    for (field, value) in values {
        content = content.replace(&format!("{{{{{field}}}}}"), value);
    }
    OutputDocument {
        id: filename.to_string(),
        document_type: document_type.to_string(),
        filename: filename.to_string(),
        content,
    }
}
