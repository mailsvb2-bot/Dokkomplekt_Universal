//! Privacy-preserving ground-truth corpus records.
//!
//! A corpus entry is written only after the specialist's final case and the
//! actually generated document set are known. Raw source text and raw field
//! values are never stored here: comparisons use domain-separated SHA-256
//! fingerprints, while confidence/provenance/evidence metadata remain useful
//! for calibration and promotion decisions.

use crate::{DomainKind, SemanticCase, SemanticValue, ValueSource};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldObservation {
    pub field_id: String,
    pub value_sha256: String,
    pub source: ValueSource,
    pub confidence: f32,
    #[serde(default)]
    pub evidence_sha256: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusEntry {
    pub entry_id: String,
    pub case_id: String,
    pub source_sha256: String,
    pub input_text_sha256: String,
    pub domain: DomainKind,
    pub pack_id: Option<String>,
    #[serde(default)]
    pub cluster_id: Option<String>,
    #[serde(default)]
    pub model_proposals: Vec<FieldObservation>,
    #[serde(default)]
    pub deterministic: Vec<FieldObservation>,
    #[serde(default)]
    pub final_accepted: Vec<FieldObservation>,
    #[serde(default)]
    pub proposed_kit_documents: Vec<String>,
    #[serde(default)]
    pub kit_proposal_source: Option<String>,
    #[serde(default)]
    pub kit_documents: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CorpusEntryMetrics {
    pub compared_model_fields: u32,
    pub matching_model_fields: u32,
    pub corrected_model_fields: u32,
    pub missing_model_fields: u32,
    pub field_accuracy: f32,
    pub kit_compared: bool,
    pub kit_exact_match: bool,
    pub kit_precision: f32,
    pub kit_recall: f32,
}

/// Named input for a privacy-preserving corpus record.
///
/// Keeping the fields named prevents silent argument-order regressions when
/// the corpus schema grows (for example, when cluster or routing provenance is
/// added). The borrowed case data is never persisted verbatim.
pub struct CorpusEntryRequest<'a> {
    pub entry_id: String,
    pub case_id: String,
    pub source_sha256: &'a str,
    pub fingerprint_key: &'a [u8; 32],
    pub input_text: &'a str,
    pub domain: DomainKind,
    pub pack_id: Option<String>,
    pub cluster_id: Option<String>,
    pub model_case: &'a SemanticCase,
    pub deterministic_case: &'a SemanticCase,
    pub final_case: &'a SemanticCase,
    pub proposed_kit_documents: Vec<String>,
    pub kit_proposal_source: Option<String>,
    pub kit_documents: Vec<String>,
    pub created_at: String,
}

pub fn build_corpus_entry(request: CorpusEntryRequest<'_>) -> Result<CorpusEntry, String> {
    validate_sha256(request.source_sha256)?;
    let proposed_kit_documents = normalized_document_ids(request.proposed_kit_documents);
    let kit_documents = normalized_document_ids(request.kit_documents);

    Ok(CorpusEntry {
        entry_id: validate_identifier(request.entry_id, "entry_id")?,
        case_id: validate_identifier(request.case_id, "case_id")?,
        source_sha256: request.source_sha256.to_ascii_lowercase(),
        input_text_sha256: fingerprint(
            request.fingerprint_key,
            "input-text",
            request.input_text.trim(),
        )?,
        domain: request.domain,
        pack_id: request
            .pack_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        cluster_id: request
            .cluster_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        model_proposals: observations(
            request.model_case,
            Some(ValueSource::Model),
            request.fingerprint_key,
        )?,
        deterministic: observations_without_model(
            request.deterministic_case,
            request.fingerprint_key,
        )?,
        final_accepted: observations(request.final_case, None, request.fingerprint_key)?,
        proposed_kit_documents,
        kit_proposal_source: request
            .kit_proposal_source
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        kit_documents,
        created_at: request.created_at,
    })
}

pub fn corpus_entry_metrics(entry: &CorpusEntry) -> CorpusEntryMetrics {
    let final_by_field = entry
        .final_accepted
        .iter()
        .map(|item| (item.field_id.as_str(), item.value_sha256.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut metrics = CorpusEntryMetrics::default();
    for proposal in &entry.model_proposals {
        metrics.compared_model_fields = metrics.compared_model_fields.saturating_add(1);
        match final_by_field.get(proposal.field_id.as_str()) {
            Some(value) if *value == proposal.value_sha256 => {
                metrics.matching_model_fields = metrics.matching_model_fields.saturating_add(1)
            }
            Some(_) => {
                metrics.corrected_model_fields = metrics.corrected_model_fields.saturating_add(1)
            }
            None => metrics.missing_model_fields = metrics.missing_model_fields.saturating_add(1),
        }
    }
    if metrics.compared_model_fields > 0 {
        metrics.field_accuracy =
            metrics.matching_model_fields as f32 / metrics.compared_model_fields as f32;
    }
    if !entry.proposed_kit_documents.is_empty() {
        metrics.kit_compared = true;
        let proposed = entry
            .proposed_kit_documents
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let actual = entry
            .kit_documents
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let matching = proposed.intersection(&actual).count() as f32;
        metrics.kit_exact_match = proposed == actual;
        metrics.kit_precision = matching / proposed.len().max(1) as f32;
        metrics.kit_recall = matching / actual.len().max(1) as f32;
    }
    metrics
}

fn normalized_document_ids(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn observations(
    case: &SemanticCase,
    only_source: Option<ValueSource>,
    fingerprint_key: &[u8; 32],
) -> Result<Vec<FieldObservation>, String> {
    let mut result = case
        .values
        .values()
        .filter(|value| only_source.is_none_or(|source| value.source == source))
        .filter(|value| !value.value.trim().is_empty())
        .map(|value| observation(value, fingerprint_key))
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by(|left, right| left.field_id.cmp(&right.field_id));
    Ok(result)
}

fn observations_without_model(
    case: &SemanticCase,
    fingerprint_key: &[u8; 32],
) -> Result<Vec<FieldObservation>, String> {
    let mut result = case
        .values
        .values()
        .filter(|value| value.source != ValueSource::Model)
        .filter(|value| !value.value.trim().is_empty())
        .map(|value| observation(value, fingerprint_key))
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by(|left, right| left.field_id.cmp(&right.field_id));
    Ok(result)
}

fn observation(
    value: &SemanticValue,
    fingerprint_key: &[u8; 32],
) -> Result<FieldObservation, String> {
    let mut evidence_sha256 = value
        .evidence
        .iter()
        .filter_map(|evidence| {
            let excerpt = evidence.excerpt.trim();
            (!excerpt.is_empty()).then_some(excerpt)
        })
        .map(|excerpt| fingerprint(fingerprint_key, "evidence", excerpt))
        .collect::<Result<BTreeSet<_>, _>>()?
        .into_iter()
        .collect::<Vec<_>>();
    evidence_sha256.sort();
    Ok(FieldObservation {
        field_id: value.field_id.clone(),
        value_sha256: fingerprint(fingerprint_key, "field-value", value.value.trim())?,
        source: value.source,
        confidence: value.confidence.clamp(0.0, 1.0),
        evidence_sha256,
    })
}

fn fingerprint(fingerprint_key: &[u8; 32], kind: &str, value: &str) -> Result<String, String> {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(fingerprint_key)
        .map_err(|_| "invalid corpus fingerprint key".to_string())?;
    mac.update(b"dokkomplekt-corpus-v2\0");
    mac.update(kind.as_bytes());
    mac.update(b"\0");
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("source_sha256 must be a 64-character hexadecimal SHA-256".into())
    }
}

fn validate_identifier(value: String, title: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 160
        || trimmed.chars().any(char::is_control)
        || trimmed.contains('/')
        || trimmed.contains('\\')
    {
        Err(format!("{title} is invalid"))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueEvidence};

    fn value(field: &str, raw: &str, source: ValueSource, confidence: f32) -> SemanticValue {
        SemanticValue::new(field, raw, source, confidence)
            .with_evidence(ValueEvidence::new("source", raw, "test", confidence))
    }

    #[test]
    fn corpus_never_contains_raw_values_or_source_text() {
        let mut model = SemanticCase::default();
        model.values.insert(
            "subject.name".into(),
            value(
                "subject.name",
                "Иванов Иван Иванович",
                ValueSource::Model,
                0.9,
            ),
        );
        let source_sha256 = "a".repeat(64);
        let deterministic = SemanticCase::default();
        let entry = build_corpus_entry(CorpusEntryRequest {
            entry_id: "entry-1".into(),
            case_id: "case-1".into(),
            source_sha256: &source_sha256,
            fingerprint_key: &[7u8; 32],
            input_text: "ФИО: Иванов Иван Иванович",
            domain: DomainKind::Hr,
            pack_id: Some("hr-pack".into()),
            cluster_id: None,
            model_case: &model,
            deterministic_case: &deterministic,
            final_case: &model,
            proposed_kit_documents: vec!["employment_contract".into()],
            kit_proposal_source: Some("curated-router".into()),
            kit_documents: vec!["employment_contract".into()],
            created_at: "2026-07-21T12:00:00Z".into(),
        })
        .unwrap();
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("Иванов"));
        assert!(!json.contains("ФИО:"));
        assert_eq!(entry.model_proposals.len(), 1);
        let metrics = corpus_entry_metrics(&entry);
        assert!(metrics.kit_compared);
        assert!(metrics.kit_exact_match);
    }

    #[test]
    fn metrics_compare_model_with_specialist_final_not_deterministic_parser() {
        let mut model = SemanticCase::default();
        model.values.insert(
            "employee.position".into(),
            value("employee.position", "инженер", ValueSource::Model, 0.8),
        );
        let mut final_case = SemanticCase::default();
        final_case.values.insert(
            "employee.position".into(),
            value(
                "employee.position",
                "ведущий инженер",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let source_sha256 = "b".repeat(64);
        let deterministic = SemanticCase::default();
        let entry = build_corpus_entry(CorpusEntryRequest {
            entry_id: "entry-2".into(),
            case_id: "case-2".into(),
            source_sha256: &source_sha256,
            fingerprint_key: &[8u8; 32],
            input_text: "Должность: инженер",
            domain: DomainKind::Hr,
            pack_id: None,
            cluster_id: None,
            model_case: &model,
            deterministic_case: &deterministic,
            final_case: &final_case,
            proposed_kit_documents: Vec::new(),
            kit_proposal_source: None,
            kit_documents: Vec::new(),
            created_at: "2026-07-21T12:00:00Z".into(),
        })
        .unwrap();
        let metrics = corpus_entry_metrics(&entry);
        assert_eq!(metrics.compared_model_fields, 1);
        assert_eq!(metrics.corrected_model_fields, 1);
        assert_eq!(metrics.matching_model_fields, 0);
    }
    #[test]
    fn low_entropy_values_are_installation_keyed() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "document.date".into(),
            value(
                "document.date",
                "01.01.2026",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let source_sha256 = "d".repeat(64);
        let model_case = SemanticCase::default();
        let deterministic_case = SemanticCase::default();
        let first = build_corpus_entry(CorpusEntryRequest {
            entry_id: "entry-key-1".into(),
            case_id: "case-key-1".into(),
            source_sha256: &source_sha256,
            fingerprint_key: &[1u8; 32],
            input_text: "Дата: 01.01.2026",
            domain: DomainKind::Generic,
            pack_id: None,
            cluster_id: None,
            model_case: &model_case,
            deterministic_case: &deterministic_case,
            final_case: &case,
            proposed_kit_documents: Vec::new(),
            kit_proposal_source: None,
            kit_documents: Vec::new(),
            created_at: "2026-07-21T12:00:00Z".into(),
        })
        .unwrap();
        let second = build_corpus_entry(CorpusEntryRequest {
            entry_id: "entry-key-2".into(),
            case_id: "case-key-2".into(),
            source_sha256: &source_sha256,
            fingerprint_key: &[2u8; 32],
            input_text: "Дата: 01.01.2026",
            domain: DomainKind::Generic,
            pack_id: None,
            cluster_id: None,
            model_case: &model_case,
            deterministic_case: &deterministic_case,
            final_case: &case,
            proposed_kit_documents: Vec::new(),
            kit_proposal_source: None,
            kit_documents: Vec::new(),
            created_at: "2026-07-21T12:00:00Z".into(),
        })
        .unwrap();
        assert_ne!(
            first.final_accepted[0].value_sha256,
            second.final_accepted[0].value_sha256
        );
        assert_ne!(first.input_text_sha256, second.input_text_sha256);
    }
}
