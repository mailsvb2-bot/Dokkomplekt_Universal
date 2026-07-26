//! Safe segmentation of compound sources (archives and e-mail attachments).
//!
//! A compound source is never flattened into one semantic case until identity
//! compatibility has been checked. Strong identifier conflicts create separate
//! segments and block zero-touch generation.

use crate::{canonical_storage_field_id, SemanticCase, SemanticValue};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const IDENTITY_FIELDS: &[&str] = &[
    "subject.name",
    "subject.birth_date",
    "medical.case_number",
    "employee.name",
    "employee.tab_number",
    "org.inn",
    "counterparty.inn",
    "contract.number",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseFragment {
    pub source_reference: String,
    pub text: String,
    pub semantic_case: SemanticCase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseIdentityConflict {
    pub field_id: String,
    pub left_value: String,
    pub right_value: String,
    pub left_source: String,
    pub right_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseSegment {
    pub segment_id: String,
    pub source_references: Vec<String>,
    pub identity: BTreeMap<String, String>,
    pub semantic_case: SemanticCase,
    pub conflicts: Vec<CaseIdentityConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseSegmentationReport {
    pub segments: Vec<CaseSegment>,
    pub unassigned_sources: Vec<String>,
    pub zero_touch_allowed: bool,
    pub reasons: Vec<String>,
}

pub fn segment_case_fragments(fragments: &[CaseFragment]) -> CaseSegmentationReport {
    let mut segments = Vec::<CaseSegment>::new();
    let mut unassigned = Vec::<CaseFragment>::new();

    for fragment in fragments {
        let identity = extract_identity(&fragment.semantic_case);
        if identity.is_empty() {
            unassigned.push(fragment.clone());
            continue;
        }

        let compatible = segments
            .iter()
            .enumerate()
            .filter_map(|(index, segment)| {
                identity_compatibility(&segment.identity, &identity).then_some(index)
            })
            .collect::<Vec<_>>();
        let target = if compatible.len() == 1 {
            Some(compatible[0])
        } else if compatible.len() > 1 {
            compatible
                .into_iter()
                .max_by_key(|index| identity_match_count(&segments[*index].identity, &identity))
        } else {
            None
        };

        if let Some(index) = target {
            merge_fragment(&mut segments[index], fragment, identity);
        } else {
            segments.push(CaseSegment {
                segment_id: format!("case-{}", segments.len() + 1),
                source_references: vec![fragment.source_reference.clone()],
                identity,
                semantic_case: fragment.semantic_case.clone(),
                conflicts: Vec::new(),
            });
        }
    }

    if segments.len() == 1 {
        for fragment in unassigned.drain(..) {
            merge_fragment(&mut segments[0], &fragment, BTreeMap::new());
        }
    }

    let mut reasons = Vec::new();
    if segments.len() > 1 {
        reasons.push(format!(
            "В источнике обнаружено независимых дел: {}. Автоматическое объединение запрещено.",
            segments.len()
        ));
    }
    if !unassigned.is_empty() {
        reasons.push(format!(
            "Не удалось безопасно отнести вложения к делу: {}.",
            unassigned
                .iter()
                .map(|fragment| fragment.source_reference.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let conflict_count = segments
        .iter()
        .map(|segment| segment.conflicts.len())
        .sum::<usize>();
    if conflict_count > 0 {
        reasons.push(format!(
            "Обнаружено конфликтов значений внутри одного дела: {conflict_count}."
        ));
    }
    let zero_touch_allowed = segments.len() == 1 && unassigned.is_empty() && conflict_count == 0;

    CaseSegmentationReport {
        segments,
        unassigned_sources: unassigned
            .into_iter()
            .map(|fragment| fragment.source_reference)
            .collect(),
        zero_touch_allowed,
        reasons,
    }
}

fn extract_identity(case: &SemanticCase) -> BTreeMap<String, String> {
    IDENTITY_FIELDS
        .iter()
        .filter_map(|field_id| {
            case.get(field_id)
                .map(normalize_identity_value)
                .filter(|value| !value.is_empty())
                .map(|value| (canonical_storage_field_id(field_id), value))
        })
        .collect()
}

fn normalize_identity_value(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn identity_compatibility(
    left: &BTreeMap<String, String>,
    right: &BTreeMap<String, String>,
) -> bool {
    let shared = left
        .keys()
        .filter(|field_id| right.contains_key(*field_id))
        .collect::<Vec<_>>();
    if shared
        .iter()
        .any(|field_id| left.get(*field_id) != right.get(*field_id))
    {
        return false;
    }
    shared
        .iter()
        .any(|field_id| left.get(*field_id) == right.get(*field_id))
}

fn identity_match_count(
    left: &BTreeMap<String, String>,
    right: &BTreeMap<String, String>,
) -> usize {
    left.iter()
        .filter(|(field_id, value)| right.get(*field_id) == Some(*value))
        .count()
}

fn merge_fragment(
    segment: &mut CaseSegment,
    fragment: &CaseFragment,
    fragment_identity: BTreeMap<String, String>,
) {
    if !segment
        .source_references
        .contains(&fragment.source_reference)
    {
        segment
            .source_references
            .push(fragment.source_reference.clone());
    }
    for (field_id, value) in fragment_identity {
        if let Some(existing) = segment.identity.get(&field_id) {
            if existing != &value {
                segment.conflicts.push(CaseIdentityConflict {
                    field_id: field_id.clone(),
                    left_value: existing.clone(),
                    right_value: value,
                    left_source: segment
                        .source_references
                        .first()
                        .cloned()
                        .unwrap_or_default(),
                    right_source: fragment.source_reference.clone(),
                });
            }
        } else {
            segment.identity.insert(field_id, value);
        }
    }
    merge_semantic_case(
        &mut segment.semantic_case,
        &fragment.semantic_case,
        &fragment.source_reference,
        &mut segment.conflicts,
    );
}

fn merge_semantic_case(
    target: &mut SemanticCase,
    incoming: &SemanticCase,
    incoming_source: &str,
    conflicts: &mut Vec<CaseIdentityConflict>,
) {
    for (field_id, value) in &incoming.values {
        let canonical = canonical_storage_field_id(field_id);
        match target.value(&canonical) {
            Some(existing) if !same_value(existing, value) => {
                if IDENTITY_FIELDS
                    .iter()
                    .map(|id| canonical_storage_field_id(id))
                    .collect::<BTreeSet<_>>()
                    .contains(&canonical)
                {
                    conflicts.push(CaseIdentityConflict {
                        field_id: canonical,
                        left_value: existing.value.clone(),
                        right_value: value.value.clone(),
                        left_source: existing
                            .evidence
                            .first()
                            .and_then(|item| item.source_reference.clone())
                            .unwrap_or_else(|| "previous_fragment".into()),
                        right_source: incoming_source.into(),
                    });
                }
                if value.source > existing.source
                    || (value.source == existing.source && value.confidence > existing.confidence)
                {
                    target.values.insert(field_id.clone(), value.clone());
                }
            }
            None => {
                target.values.insert(field_id.clone(), value.clone());
            }
            _ => {}
        }
    }
    for domain in &incoming.active_domains {
        if !target.active_domains.contains(domain) {
            target.active_domains.push(domain.clone());
        }
    }
    for (key, records) in &incoming.collections {
        target
            .collections
            .entry(key.clone())
            .or_default()
            .extend(records.clone());
    }
    for (key, value) in &incoming.blocks {
        target
            .blocks
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

fn same_value(left: &SemanticValue, right: &SemanticValue) -> bool {
    normalize_identity_value(&left.value) == normalize_identity_value(&right.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    fn fragment(source: &str, values: &[(&str, &str)]) -> CaseFragment {
        let mut semantic_case = SemanticCase::default();
        for (field, value) in values {
            semantic_case.values.insert(
                (*field).into(),
                SemanticValue::new(*field, *value, ValueSource::Scanner, 0.95),
            );
        }
        CaseFragment {
            source_reference: source.into(),
            text: source.into(),
            semantic_case,
        }
    }

    #[test]
    fn same_person_fragments_are_merged() {
        let report = segment_case_fragments(&[
            fragment("a.docx", &[("subject.name", "Иванов Иван")]),
            fragment(
                "b.pdf",
                &[
                    ("subject.name", "Иванов Иван"),
                    ("medical.case_number", "42"),
                ],
            ),
        ]);
        assert!(report.zero_touch_allowed);
        assert_eq!(report.segments.len(), 1);
        assert_eq!(report.segments[0].source_references.len(), 2);
    }

    #[test]
    fn different_people_are_never_flattened() {
        let report = segment_case_fragments(&[
            fragment("ivanov.docx", &[("subject.name", "Иванов Иван")]),
            fragment("petrov.docx", &[("subject.name", "Петров Пётр")]),
        ]);
        assert!(!report.zero_touch_allowed);
        assert_eq!(report.segments.len(), 2);
    }

    #[test]
    fn anonymous_attachment_is_safe_only_when_one_case_exists() {
        let report = segment_case_fragments(&[
            fragment("primary.docx", &[("subject.name", "Иванов Иван")]),
            fragment("scan.pdf", &[("document.number", "15")]),
        ]);
        assert!(report.zero_touch_allowed);
        assert_eq!(report.segments[0].source_references.len(), 2);
    }

    #[test]
    fn unassigned_attachment_blocks_when_multiple_cases_exist() {
        let report = segment_case_fragments(&[
            fragment("ivanov.docx", &[("subject.name", "Иванов Иван")]),
            fragment("petrov.docx", &[("subject.name", "Петров Пётр")]),
            fragment("scan.pdf", &[("document.number", "15")]),
        ]);
        assert!(!report.zero_touch_allowed);
        assert_eq!(report.unassigned_sources, vec!["scan.pdf"]);
    }
}
