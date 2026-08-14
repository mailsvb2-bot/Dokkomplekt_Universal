use crate::data_schema_engine::{is_safe_field_id, UnifiedFieldDefinition, UnifiedFieldKind};
use crate::domain_plugin_layer::{builtin_domain_plugins_v2, plugin_by_id, DomainPluginId};
use crate::label_search::find_label_end;
use crate::{canonical_storage_field_id, parse_flexible_date};
use chrono::{Datelike, Local};
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

/// Parse the profession-neutral facts that belong to the universal Core.
///
/// Domain-specific extraction deliberately does not live here. The higher-level
/// universal source parser enriches this result through the selected plugin.
/// Keeping this layer neutral prevents labels that have different meanings in
/// different professions from contaminating one another.
///
/// Learned or custom profiles can still pass explicit semantic facts through
/// `field.<id>` metadata. Those ids are validated structurally and therefore do
/// not require a hardcoded profession in the Core.
pub fn parse_source_document(source: &SourceDocument) -> ParsedDocument {
    let default_year = default_year_for_source(source);
    parse_source_document_with_resolved_year(source, default_year)
}

/// Parse a source using temporal context supplied by the caller.
///
/// A higher-level workflow already knows the intended default year and must not
/// allow the Core to replace it with the machine clock or an unrelated year
/// found elsewhere in the document. Invalid caller years deliberately fall back
/// to the legacy source resolver so existing direct callers remain compatible.
pub fn parse_source_document_with_default_year(
    source: &SourceDocument,
    default_year: i32,
) -> ParsedDocument {
    let resolved_year = if (1900..=2200).contains(&default_year) {
        default_year
    } else {
        default_year_for_source(source)
    };
    parse_source_document_with_resolved_year(source, resolved_year)
}

fn parse_source_document_with_resolved_year(
    source: &SourceDocument,
    default_year: i32,
) -> ParsedDocument {
    let title = source
        .text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("document")
        .to_string();
    let mut warnings = Vec::new();
    let mut fields = BTreeMap::<String, Field>::new();

    let core = plugin_by_id(&DomainPluginId::Core);
    for definition in core.field_definitions {
        if let Some(field) = extract_declared_field(&source.text, &definition, default_year) {
            insert_field(&mut fields, field);
        }
    }

    for (key, value) in &source.metadata {
        let Some(raw_id) = key.strip_prefix("field.") else {
            continue;
        };
        let field_id = canonical_storage_field_id(raw_id);
        if !is_safe_field_id(&field_id) {
            warnings.push(format!(
                "Отклонено небезопасное learned/custom поле из metadata: {raw_id}"
            ));
            continue;
        }
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if value.is_empty() {
            continue;
        }
        insert_field(
            &mut fields,
            Field {
                id: field_id,
                value,
                confidence: 100,
            },
        );
    }

    ParsedDocument {
        document_type: DocumentType {
            id: normalize_document_type(&title),
            title,
            confidence: 50,
        },
        fields: fields.into_values().collect(),
        warnings,
    }
}

pub(crate) fn insert_field(fields: &mut BTreeMap<String, Field>, field: Field) {
    if field.id.trim().is_empty() || field.value.trim().is_empty() || !is_safe_field_id(&field.id) {
        return;
    }
    match fields.get(&field.id) {
        Some(existing) if existing.confidence >= field.confidence => {}
        _ => {
            fields.insert(field.id.clone(), field);
        }
    }
}

pub(crate) fn extract_declared_field(
    text: &str,
    definition: &UnifiedFieldDefinition,
    default_year: i32,
) -> Option<Field> {
    let lines = text.lines().collect::<Vec<_>>();
    for (line_index, line) in lines.iter().enumerate() {
        for alias in &definition.aliases {
            let Some(value_start) = find_label_end(line, alias) else {
                continue;
            };
            let inline = line[value_start..]
                .trim_start_matches([' ', ':', '-', '—', '№', '\t', '\u{00a0}'])
                .trim();
            if !inline.is_empty() {
                if let Some(value) =
                    normalize_declared_value(&definition.kind, inline, default_year)
                {
                    return Some(Field {
                        id: canonical_storage_field_id(&definition.id),
                        value,
                        confidence: 84,
                    });
                }
            }
            if let Some(next) = next_declared_value(&lines, line_index + 1) {
                if let Some(value) = normalize_declared_value(&definition.kind, next, default_year)
                {
                    return Some(Field {
                        id: canonical_storage_field_id(&definition.id),
                        value,
                        confidence: 76,
                    });
                }
            }
        }
    }
    None
}

/// Return the first following value line without crossing another declared
/// field from any built-in domain or an explicit section heading. A field label
/// is document structure, never data for the preceding field.
fn next_declared_value<'a>(lines: &'a [&'a str], start: usize) -> Option<&'a str> {
    for line in lines.iter().skip(start) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if starts_with_declared_field_label(line) || is_explicit_section_heading(line) {
            return None;
        }
        return Some(line);
    }
    None
}

fn starts_with_declared_field_label(line: &str) -> bool {
    let folded = line.trim_start().to_lowercase();
    builtin_domain_plugins_v2()
        .into_iter()
        .flat_map(|plugin| plugin.field_definitions.into_iter())
        .flat_map(|definition| definition.aliases.into_iter())
        .any(|alias| {
            let alias = alias.trim().to_lowercase();
            folded.strip_prefix(&alias).is_some_and(|remainder| {
                remainder
                    .chars()
                    .next()
                    .is_none_or(|character| !character.is_alphanumeric())
            })
        })
}

fn is_explicit_section_heading(line: &str) -> bool {
    if !line.trim_end().ends_with(':') {
        return false;
    }
    let letters = line
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect::<Vec<_>>();
    letters.len() >= 3 && letters.iter().all(|character| character.is_uppercase())
}

fn normalize_declared_value(
    kind: &UnifiedFieldKind,
    raw: &str,
    default_year: i32,
) -> Option<String> {
    let clean = raw
        .trim()
        .trim_matches(|character: char| matches!(character, ':' | ';' | ' '))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.is_empty() {
        return None;
    }
    match kind {
        UnifiedFieldKind::Date => parse_flexible_date(&clean, default_year),
        UnifiedFieldKind::Number | UnifiedFieldKind::Money => normalize_number(&clean),
        UnifiedFieldKind::Text | UnifiedFieldKind::LongText | UnifiedFieldKind::Choice => {
            Some(clean)
        }
    }
}

fn normalize_number(value: &str) -> Option<String> {
    let mut candidate = String::new();
    let mut started = false;
    for character in value.chars() {
        let numeric = character.is_ascii_digit()
            || matches!(character, '+' | '-' | ',' | '.' | ' ' | '\u{00a0}');
        if numeric {
            candidate.push(character);
            started = true;
        } else if started {
            break;
        }
    }
    let normalized = candidate
        .trim()
        .replace([' ', '\u{00a0}'], "")
        .replace(',', ".");
    if normalized.is_empty()
        || normalized.matches('.').count() > 1
        || normalized.matches('-').count() > 1
        || normalized.matches('+').count() > 1
        || (normalized.contains('-') && !normalized.starts_with('-'))
        || (normalized.contains('+') && !normalized.starts_with('+'))
    {
        return None;
    }
    let number = normalized.parse::<f64>().ok()?;
    if !number.is_finite() || number.abs() > 1.0e15 {
        return None;
    }
    Some(normalized)
}

pub(crate) fn default_year_for_source(source: &SourceDocument) -> i32 {
    if let Some(year) = source
        .metadata
        .get("default_year")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|year| (1900..=2200).contains(year))
    {
        return year;
    }
    for token in source
        .text
        .split(|character: char| !character.is_ascii_digit())
    {
        if token.len() != 4 {
            continue;
        }
        if let Ok(year) = token.parse::<i32>() {
            if (1900..=2200).contains(&year) {
                return year;
            }
        }
    }
    Local::now().year()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(document: &'a ParsedDocument, field_id: &str) -> Option<&'a str> {
        document
            .fields
            .iter()
            .find(|field| field.id == field_id)
            .map(|field| field.value.as_str())
    }

    #[test]
    fn core_parser_extracts_declared_universal_fields() {
        let document = parse_source_document(&SourceDocument {
            id: "generic".into(),
            text: "ОТЧЁТ\nНомер документа: R-17\nДата документа: 14.07.2026\nКомпания: ООО Ромашка\nФИО: Иванов Иван Иванович".into(),
            metadata: BTreeMap::from([("default_year".into(), "2026".into())]),
        });
        assert_eq!(value(&document, "document.number"), Some("R-17"));
        assert_eq!(value(&document, "document.date"), Some("14.07.2026"));
        assert_eq!(value(&document, "org.name"), Some("ООО Ромашка"));
        assert_eq!(
            value(&document, "subject.name"),
            Some("Иванов Иван Иванович")
        );
    }

    #[test]
    fn explicit_default_year_overrides_clock_and_unrelated_document_years() {
        let document = parse_source_document_with_default_year(
            &SourceDocument {
                id: "year-context".into(),
                text: "Редакция 2026\ndocument.date: 14.07".into(),
                metadata: BTreeMap::new(),
            },
            2025,
        );
        assert_eq!(value(&document, "document.date"), Some("14.07.2025"));
    }

    #[test]
    fn next_core_label_is_never_consumed_as_previous_field_value() {
        let document = parse_source_document(&SourceDocument {
            id: "field-boundary".into(),
            text: "Клиент\nДата документа: 14.07.2026".into(),
            metadata: BTreeMap::from([("default_year".into(), "2026".into())]),
        });
        assert_eq!(value(&document, "subject.name"), None);
        assert_eq!(value(&document, "document.date"), Some("14.07.2026"));
    }

    #[test]
    fn next_domain_label_is_never_consumed_as_core_field_value() {
        let document = parse_source_document(&SourceDocument {
            id: "cross-domain-boundary".into(),
            text: "Клиент\nДиагноз: F32.1".into(),
            metadata: BTreeMap::new(),
        });
        assert_eq!(value(&document, "subject.name"), None);
    }

    #[test]
    fn explicit_section_heading_stops_multiline_fallback() {
        let document = parse_source_document(&SourceDocument {
            id: "heading-boundary".into(),
            text: "Клиент\nРЕКВИЗИТЫ:\nКомпания: ООО Ромашка".into(),
            metadata: BTreeMap::new(),
        });
        assert_eq!(value(&document, "subject.name"), None);
        assert_eq!(value(&document, "org.name"), Some("ООО Ромашка"));
    }

    #[test]
    fn explicit_learned_fields_do_not_require_a_hardcoded_profession() {
        let document = parse_source_document(&SourceDocument {
            id: "custom".into(),
            text: "Пользовательская карточка".into(),
            metadata: BTreeMap::from([
                ("field.custom.subject_name".into(), "Барсик".into()),
                ("field.custom.classification".into(), "Категория А".into()),
            ]),
        });
        assert_eq!(value(&document, "custom.subject_name"), Some("Барсик"));
        assert_eq!(
            value(&document, "custom.classification"),
            Some("Категория А")
        );
    }

    #[test]
    fn unsafe_learned_metadata_field_is_rejected_fail_closed() {
        let document = parse_source_document(&SourceDocument {
            id: "unsafe".into(),
            text: "Пользовательский документ".into(),
            metadata: BTreeMap::from([("field../escape".into(), "value".into())]),
        });
        assert!(document.fields.is_empty());
        assert!(document
            .warnings
            .iter()
            .any(|warning| warning.contains("небезопасное learned/custom поле")));
    }
}
