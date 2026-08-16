//! Profession-neutral supplementary source registry and deterministic name matcher.
//!
//! The universal core records additional user-owned materials without assigning
//! professional meaning to them. Domain adapters may consume role-prefixed
//! sources (for example the medical diary adapter), while other professions use
//! the same storage contract for attachments, registries and reference packs.

use crate::{SemanticAtom, SemanticCase, SemanticRecord};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const SUPPLEMENTARY_SOURCES_COLLECTION: &str = "supplementary_sources";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplementarySourceSpec {
    pub source_id: String,
    pub role: String,
    pub name: String,
    pub source_kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub path: String,
}

pub fn upsert_supplementary_source(case: &mut SemanticCase, source: &SupplementarySourceSpec) {
    let mut rows = case
        .collection(SUPPLEMENTARY_SOURCES_COLLECTION)
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    rows.retain(|row| atom(row, "source_id").as_deref() != Some(source.source_id.as_str()));
    rows.push(source_record(source));
    case.set_collection(SUPPLEMENTARY_SOURCES_COLLECTION, rows);
}

pub fn remove_supplementary_source(case: &mut SemanticCase, source_id: &str) -> bool {
    let Some(existing) = case.collection(SUPPLEMENTARY_SOURCES_COLLECTION) else {
        return false;
    };
    let mut rows = existing.to_vec();
    let before = rows.len();
    rows.retain(|row| atom(row, "source_id").as_deref() != Some(source_id));
    if rows.len() == before {
        return false;
    }
    case.set_collection(SUPPLEMENTARY_SOURCES_COLLECTION, rows);
    true
}

pub fn supplementary_sources(
    case: &SemanticCase,
    role: Option<&str>,
) -> Vec<SupplementarySourceSpec> {
    case.collection(SUPPLEMENTARY_SOURCES_COLLECTION)
        .unwrap_or_default()
        .iter()
        .filter_map(source_from_record)
        .filter(|source| role.is_none_or(|wanted| source.role == wanted))
        .collect()
}

fn source_record(source: &SupplementarySourceSpec) -> SemanticRecord {
    let mut row = SemanticRecord::new();
    for (key, value) in [
        ("source_id", source.source_id.as_str()),
        ("role", source.role.as_str()),
        ("name", source.name.as_str()),
        ("source_kind", source.source_kind.as_str()),
        ("text", source.text.as_str()),
        ("path", source.path.as_str()),
    ] {
        row.insert(key.into(), SemanticAtom::Text(value.into()));
    }
    row
}

fn source_from_record(row: &SemanticRecord) -> Option<SupplementarySourceSpec> {
    Some(SupplementarySourceSpec {
        source_id: atom(row, "source_id")?,
        role: atom(row, "role")?,
        name: atom(row, "name")?,
        source_kind: atom(row, "source_kind").unwrap_or_else(|| "unknown".into()),
        text: atom(row, "text").unwrap_or_default(),
        path: atom(row, "path").unwrap_or_default(),
    })
}

fn atom(row: &SemanticRecord, key: &str) -> Option<String> {
    row.get(key)
        .map(SemanticAtom::as_text)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Profession-neutral deterministic score for matching a requested semantic
/// label to a user-owned file/reference name. No medical/legal/HR synonyms are
/// encoded here; domain-specific bridges belong to the respective profile.
pub fn reference_name_match_score(query: &str, candidate: &str) -> u32 {
    let query = normalize_reference_name(query);
    let candidate = normalize_reference_name(candidate);
    if query.is_empty() || candidate.is_empty() {
        return 0;
    }
    if query == candidate {
        return 120;
    }
    if query.contains(&candidate) || candidate.contains(&query) {
        let shorter = query.chars().count().min(candidate.chars().count()) as u32;
        return 96 + shorter.min(12);
    }

    let query_tokens = tokens(&query);
    let candidate_tokens = tokens(&candidate);
    if query_tokens.is_empty() || candidate_tokens.is_empty() {
        return 0;
    }
    let overlap = query_tokens.intersection(&candidate_tokens).count() as u32;
    if overlap == 0 {
        return 0;
    }
    let denominator = query_tokens.len().max(candidate_tokens.len()) as u32;
    let coverage = overlap.saturating_mul(100) / denominator.max(1);
    if overlap == 1 && coverage < 50 {
        return 0;
    }
    50 + coverage.min(45)
}

pub fn normalize_reference_name(value: &str) -> String {
    let mut value = value.trim().to_lowercase().replace('ё', "е");
    for extension in [".docx", ".docm", ".doc", ".pdf", ".txt", ".rtf"] {
        if value.ends_with(extension) {
            value.truncate(value.len() - extension.len());
            break;
        }
    }
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(value: &str) -> BTreeSet<String> {
    value
        .split_whitespace()
        .filter(|token| token.chars().count() >= 2)
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_role_scoped_and_replaceable() {
        let mut case = SemanticCase::default();
        upsert_supplementary_source(
            &mut case,
            &SupplementarySourceSpec {
                source_id: "a".into(),
                role: "reference".into(),
                name: "Приложение.pdf".into(),
                source_kind: "pdf".into(),
                text: "v1".into(),
                path: "C:/a.pdf".into(),
            },
        );
        upsert_supplementary_source(
            &mut case,
            &SupplementarySourceSpec {
                source_id: "a".into(),
                role: "reference".into(),
                name: "Приложение.pdf".into(),
                source_kind: "pdf".into(),
                text: "v2".into(),
                path: "C:/a.pdf".into(),
            },
        );
        let rows = supplementary_sources(&case, Some("reference"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "v2");
        assert!(remove_supplementary_source(&mut case, "a"));
        assert!(supplementary_sources(&case, None).is_empty());
    }

    #[test]
    fn generic_matcher_has_no_profession_dictionary() {
        assert_eq!(
            reference_name_match_score("Договор поставки", "договор поставки.docx"),
            120
        );
        assert!(
            reference_name_match_score("Договор поставки", "поставка договор приложение") >= 50
        );
        assert_eq!(reference_name_match_score("депрессия", "астения.docx"), 0);
    }
}
