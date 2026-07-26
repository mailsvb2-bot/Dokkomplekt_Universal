use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetTemplate {
    pub id: String,
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateStructure {
    pub title: String,
    pub document_type: String,
    pub fields: Vec<String>,
    pub repeated_fields: Vec<String>,
    pub tables: Vec<String>,
    pub signatures: Vec<String>,
    pub input_zones: Vec<String>,
    pub suggested_button_label: String,
}

pub fn detect_template_structure(template: &TargetTemplate) -> TemplateStructure {
    let title = template
        .text
        .lines()
        .map(str::trim)
        .find(|x| !x.is_empty())
        .map(strip_leading_date)
        .unwrap_or("Документ")
        .to_string();
    let fields = extract_fields(&template.text);
    let repeated_fields = repeated(&fields);
    let lower = template.text.to_lowercase();
    TemplateStructure {
        document_type: normalize_type(&title),
        suggested_button_label: title.clone(),
        title,
        fields,
        repeated_fields,
        tables: lower
            .matches("<w:tbl")
            .map(|_| "table".to_string())
            .collect(),
        signatures: template
            .text
            .lines()
            .filter(|l| l.to_lowercase().contains("подпис"))
            .map(str::to_string)
            .collect(),
        input_zones: extract_input_zones(&template.text),
    }
}

fn extract_fields(text: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let tail = &rest[start + 2..];
        if let Some(end) = tail.find("}}") {
            out.insert(tail[..end].trim().to_string());
            rest = &tail[end + 2..];
        } else {
            break;
        }
    }
    out.into_iter().collect()
}

fn repeated(fields: &[String]) -> Vec<String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for field in fields {
        *counts.entry(field.clone()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|(k, v)| (v > 1).then_some(k))
        .collect()
}

fn extract_input_zones(text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains("{{") || line.contains("____"))
        .map(|(idx, _)| format!("line:{idx}"))
        .collect()
}

fn strip_leading_date(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() >= 10
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && matches!(bytes[2], b'.' | b'/' | b'-')
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && matches!(bytes[5], b'.' | b'/' | b'-')
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
    {
        return line[10..].trim_start_matches([' ', '-', '—']).trim();
    }
    line
}

fn normalize_type(title: &str) -> String {
    title
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
