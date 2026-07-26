use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDocument {
    pub id: String,
    pub text: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub document_type: DocumentType,
    pub fields: Vec<Field>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentType {
    pub id: String,
    pub title: String,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub value: String,
    pub confidence: u8,
}

pub fn parse_source_document(source: &SourceDocument) -> ParsedDocument {
    let title = source
        .text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("document")
        .to_string();
    ParsedDocument {
        document_type: DocumentType {
            id: normalize_document_type(&title),
            title,
            confidence: 50,
        },
        fields: Vec::new(),
        warnings: Vec::new(),
    }
}

fn normalize_document_type(title: &str) -> String {
    let out = title
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    out.split('_')
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}
