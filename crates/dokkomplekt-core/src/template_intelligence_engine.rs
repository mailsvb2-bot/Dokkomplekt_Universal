use crate::data_schema_engine::{is_safe_field_id, normalize_field_id};
use crate::domain_plugin_layer::{builtin_domain_plugins_v2, DomainPluginId, DomainPluginV2};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateTableInfo {
    pub row_index: usize,
    pub columns: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSignatureInfo {
    pub line_index: usize,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateInputZone {
    pub line_index: usize,
    pub kind: String,
    pub field_id: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateStructureAnalysisV2 {
    pub title: String,
    pub document_type: String,
    pub domain: DomainPluginId,
    pub suggested_button_name: String,
    pub placeholders: Vec<String>,
    pub repeated_fields: Vec<String>,
    pub unsafe_fields: Vec<String>,
    pub tables: Vec<TemplateTableInfo>,
    pub signatures: Vec<TemplateSignatureInfo>,
    pub input_zones: Vec<TemplateInputZone>,
    pub warnings: Vec<String>,
}

pub fn analyze_template_structure_v2(text: &str) -> TemplateStructureAnalysisV2 {
    let plugins = builtin_domain_plugins_v2();
    let title = find_visible_top_title(text).unwrap_or_else(|| "Документ".into());
    let raw_placeholders = extract_placeholders(text);
    let placeholders = unique(
        raw_placeholders
            .iter()
            .map(|x| normalize_field_id(x))
            .collect(),
    );
    let repeated_fields = repeated(
        raw_placeholders
            .iter()
            .map(|x| normalize_field_id(x))
            .collect(),
    );
    let unsafe_fields = placeholders
        .iter()
        .filter(|x| !is_safe_field_id(x))
        .cloned()
        .collect::<Vec<_>>();
    let domain = detect_domain(text, &placeholders, &plugins);
    let document_type = detect_document_type(text, &domain, &plugins);
    let mut warnings = Vec::new();
    if placeholders.is_empty() {
        warnings.push("Шаблон без placeholder: будет создана статическая кнопка-копия.".into());
    }
    if !unsafe_fields.is_empty() {
        warnings.push(format!("Небезопасные поля: {}", unsafe_fields.join(", ")));
    }
    TemplateStructureAnalysisV2 {
        suggested_button_name: normalize_button_name(&title),
        title,
        document_type,
        domain,
        placeholders,
        repeated_fields,
        unsafe_fields,
        tables: find_tables(text),
        signatures: find_signatures(text),
        input_zones: find_input_zones(text),
        warnings,
    }
}

fn find_visible_top_title(text: &str) -> Option<String> {
    for raw in text.lines().take(20) {
        let line = raw.trim();
        if line.is_empty() || (line.starts_with("{{") && line.ends_with("}}")) {
            continue;
        }
        let stripped = strip_leading_date(line);
        if stripped.chars().filter(|c| c.is_alphabetic()).count() >= 4 {
            return Some(
                stripped
                    .chars()
                    .take(120)
                    .collect::<String>()
                    .trim()
                    .to_string(),
            );
        }
    }
    None
}

fn strip_leading_date(line: &str) -> String {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|x| x.chars().filter(|c| c.is_ascii_digit()).count() >= 6)
    {
        parts.into_iter().skip(1).collect::<Vec<_>>().join(" ")
    } else {
        line.to_string()
    }
}

fn extract_placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        if let Some(end) = rest[start + 2..].find("}}") {
            out.push(rest[start + 2..start + 2 + end].trim().to_string());
            rest = &rest[start + 2 + end + 2..];
        } else {
            break;
        }
    }
    out
}

fn detect_domain(
    text: &str,
    placeholders: &[String],
    plugins: &[DomainPluginV2],
) -> DomainPluginId {
    let lower = text.to_lowercase();
    plugins
        .iter()
        .filter(|p| p.id != DomainPluginId::Core)
        .map(|plugin| {
            let field_score = placeholders
                .iter()
                .filter(|field| plugin.field_definitions.iter().any(|def| &def.id == *field))
                .count()
                * 5;
            let signal_score = plugin
                .role_signals
                .values()
                .flatten()
                .filter(|signal| lower.contains(&signal.to_lowercase()))
                .count()
                * 3;
            (plugin.id.clone(), field_score + signal_score)
        })
        .max_by_key(|(_, score)| *score)
        .filter(|(_, score)| *score > 0)
        .map(|(id, _)| id)
        .unwrap_or(DomainPluginId::Custom)
}

fn detect_document_type(text: &str, domain: &DomainPluginId, plugins: &[DomainPluginV2]) -> String {
    let lower = text.to_lowercase();
    let Some(plugin) = plugins.iter().find(|p| &p.id == domain) else {
        return "custom_document".into();
    };
    plugin
        .role_signals
        .iter()
        .map(|(role, signals)| {
            (
                role.clone(),
                signals
                    .iter()
                    .filter(|signal| lower.contains(&signal.to_lowercase()))
                    .count(),
            )
        })
        .max_by_key(|(_, score)| *score)
        .filter(|(_, score)| *score > 0)
        .map(|(role, _)| role)
        .unwrap_or_else(|| format!("{:?}_document", domain).to_lowercase())
}

fn find_tables(text: &str) -> Vec<TemplateTableInfo> {
    text.lines()
        .enumerate()
        .filter_map(|(row_index, line)| {
            let pipe = line
                .split('|')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>();
            let tabs = line
                .split('\t')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>();
            if pipe.len() >= 2 {
                Some(TemplateTableInfo {
                    row_index,
                    columns: pipe,
                    reason: "pipe-separated row".into(),
                })
            } else if tabs.len() >= 2 {
                Some(TemplateTableInfo {
                    row_index,
                    columns: tabs,
                    reason: "tab-separated row".into(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn find_signatures(text: &str) -> Vec<TemplateSignatureInfo> {
    text.lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let lower = line.to_lowercase();
            if (lower.contains("подпись")
                || lower.contains("врач")
                || lower.contains("директор")
                || lower.contains("зав."))
                && (line.contains("___") || lower.contains("подпись"))
            {
                Some(TemplateSignatureInfo {
                    line_index,
                    label: line.trim().to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn find_input_zones(text: &str) -> Vec<TemplateInputZone> {
    let mut zones = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        for field in extract_placeholders(line) {
            zones.push(TemplateInputZone {
                line_index,
                kind: "placeholder".into(),
                field_id: Some(normalize_field_id(&field)),
                raw: format!("{{{{{field}}}}}"),
            });
        }
        if line.contains("____") || line.contains(".....") {
            zones.push(TemplateInputZone {
                line_index,
                kind: "underline".into(),
                field_id: None,
                raw: line.trim().to_string(),
            });
        }
    }
    zones
}

fn normalize_button_name(title: &str) -> String {
    let clean = title.split_whitespace().collect::<Vec<_>>().join(" ");
    clean.chars().take(42).collect::<String>()
}

fn unique(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|x| seen.insert(x.clone()))
        .collect()
}

fn repeated(items: Vec<String>) -> Vec<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(item, _)| item)
        .collect()
}
