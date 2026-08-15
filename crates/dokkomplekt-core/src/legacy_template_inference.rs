use crate::{canonical_field_id_for_domain, title_for_field, DomainKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MIN_UNDERSCORE_BLANK: usize = 3;
const MIN_DOT_BLANK: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyTemplateFieldCandidate {
    pub field_id: String,
    pub title: String,
    pub line_index: usize,
    pub blank_line: String,
    pub common_prefix: String,
    pub common_suffix: String,
    pub confidence: f32,
    pub reason: String,
}

/// Infer only deterministic label/blank pairs from an old plain-text Word form.
///
/// This is deliberately narrower than general semantic extraction. It never
/// learns from an already filled value and it never guesses across prose. A
/// candidate is emitted only when a registry label resolves to one canonical
/// field in the selected domain and the visible blank target occurs exactly
/// once in the template. Ambiguous labels and duplicate targets fail closed.
pub fn infer_legacy_template_fields(
    text: &str,
    preferred_domain: Option<&DomainKind>,
    role_id: Option<&str>,
) -> Vec<LegacyTemplateFieldCandidate> {
    let lines = text.lines().map(str::trim_end).collect::<Vec<_>>();
    let target_counts = lines
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut counts, line| {
            let normalized = line.trim().to_string();
            if !normalized.is_empty() {
                *counts.entry(normalized).or_default() += 1;
            }
            counts
        });

    let mut candidates = Vec::new();
    for (line_index, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.contains("{{") || line.contains("}}") {
            continue;
        }

        // A standalone blank must be handled before generic inline-blank
        // detection: otherwise the empty prefix is seen first and a valid
        // donor-style `Label\n________` pair is discarded.
        if is_blank_only(line) {
            if line_index == 0 {
                continue;
            }
            let Some(previous_index) = previous_nonempty_line(&lines, line_index) else {
                continue;
            };
            if previous_index + 1 != line_index {
                continue;
            }
            let previous = lines[previous_index].trim();
            if previous.contains("{{")
                || previous.contains("}}")
                || find_explicit_blank(previous).is_some()
            {
                continue;
            }
            let label = clean_label(previous);
            if label.is_empty() || is_too_generic_label(&label) {
                continue;
            }
            let Some(field_id) = resolve_label(&label, preferred_domain, role_id) else {
                continue;
            };
            push_if_unique_target(
                &mut candidates,
                &target_counts,
                LegacyTemplateFieldCandidate {
                    title: title_for_field(&field_id),
                    field_id,
                    line_index,
                    blank_line: line.to_string(),
                    common_prefix: String::new(),
                    common_suffix: String::new(),
                    confidence: 0.97,
                    reason: "однозначная подпись непосредственно перед уникальной пустой строкой"
                        .into(),
                },
            );
            continue;
        }

        if let Some((blank_start, blank_end)) = find_explicit_blank(line) {
            let prefix = &line[..blank_start];
            let suffix = &line[blank_end..];
            let label = clean_label(prefix);
            if !label.is_empty() && !is_too_generic_label(&label) {
                if let Some(field_id) = resolve_label(&label, preferred_domain, role_id) {
                    push_if_unique_target(
                        &mut candidates,
                        &target_counts,
                        LegacyTemplateFieldCandidate {
                            title: title_for_field(&field_id),
                            field_id,
                            line_index,
                            blank_line: line.to_string(),
                            common_prefix: prefix.to_string(),
                            common_suffix: suffix.to_string(),
                            confidence: 1.0,
                            reason: "однозначная подпись и явное пустое место в одной строке"
                                .into(),
                        },
                    );
                }
            }
        }
    }

    // A single visible target may never represent two meanings. This also makes
    // the output directly safe for `apply_template_learning_map_file`.
    let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
    for candidate in &candidates {
        owners
            .entry(candidate.blank_line.clone())
            .or_default()
            .insert(candidate.field_id.clone());
    }
    candidates.retain(|candidate| {
        owners
            .get(&candidate.blank_line)
            .is_some_and(|fields| fields.len() == 1)
    });
    candidates.sort_by_key(|candidate| candidate.line_index);
    candidates.dedup_by(|left, right| {
        left.field_id == right.field_id && left.blank_line == right.blank_line
    });
    candidates
}

fn push_if_unique_target(
    candidates: &mut Vec<LegacyTemplateFieldCandidate>,
    target_counts: &BTreeMap<String, usize>,
    candidate: LegacyTemplateFieldCandidate,
) {
    if target_counts.get(candidate.blank_line.trim()).copied() == Some(1) {
        candidates.push(candidate);
    }
}

fn resolve_label(
    label: &str,
    preferred_domain: Option<&DomainKind>,
    role_id: Option<&str>,
) -> Option<String> {
    let field_id = canonical_field_id_for_domain(label, preferred_domain)?;
    if !field_is_allowed_in_domain(&field_id, preferred_domain) {
        return None;
    }
    if matches!(preferred_domain, Some(DomainKind::Medical)) {
        Some(
            crate::domains::medical_semantics::scope_legacy_field_for_role(
                role_id.unwrap_or_default(),
                &field_id,
            ),
        )
    } else {
        Some(field_id)
    }
}

/// Template auto-markup is intentionally stricter than ordinary alias lookup.
/// A globally unique alias from another profession must not be imported into a
/// selected document domain merely because no competing alias exists there.
/// Generic fields are shared infrastructure and remain available everywhere.
fn field_is_allowed_in_domain(field_id: &str, preferred_domain: Option<&DomainKind>) -> bool {
    let Some(preferred_domain) = preferred_domain else {
        return true;
    };
    let Some(definition) = crate::all_fields()
        .into_iter()
        .find(|definition| definition.id == field_id)
    else {
        return false;
    };
    match preferred_domain {
        DomainKind::Generic | DomainKind::Custom(_) => definition.domain == DomainKind::Generic,
        domain => definition.domain == DomainKind::Generic || definition.domain == *domain,
    }
}

fn previous_nonempty_line(lines: &[&str], before: usize) -> Option<usize> {
    (0..before)
        .rev()
        .find(|index| !lines[*index].trim().is_empty())
}

fn clean_label(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, ':' | ';' | ',' | '.' | '-' | '—' | '–')
        })
        .trim()
        .to_string()
}

fn is_too_generic_label(label: &str) -> bool {
    let key = label
        .to_lowercase()
        .replace('ё', "е")
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<String>();
    matches!(
        key.as_str(),
        "дата" | "номер" | "подпись" | "значение" | "наименование" | "итого"
    )
}

fn is_blank_only(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let chars = trimmed.chars().collect::<Vec<_>>();
    let underscore_count = chars.iter().filter(|character| **character == '_').count();
    let dot_count = chars.iter().filter(|character| **character == '.').count();
    let allowed = chars
        .iter()
        .all(|character| matches!(*character, '_' | '.' | '…' | ' ' | '\t'));
    allowed
        && (underscore_count >= MIN_UNDERSCORE_BLANK
            || dot_count >= MIN_DOT_BLANK
            || chars.iter().filter(|character| **character == '…').count() >= 2)
}

fn find_explicit_blank(value: &str) -> Option<(usize, usize)> {
    let chars = value.char_indices().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        let marker = chars[index].1;
        let minimum = match marker {
            '_' => MIN_UNDERSCORE_BLANK,
            '.' => MIN_DOT_BLANK,
            '…' => 2,
            _ => {
                index += 1;
                continue;
            }
        };
        let start_index = index;
        while index < chars.len() && chars[index].1 == marker {
            index += 1;
        }
        if index - start_index >= minimum {
            let start = chars[start_index].0;
            let end = if index < chars.len() {
                chars[index].0
            } else {
                value.len()
            };
            return Some((start, end));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_safe_same_line_generic_and_medical_fields() {
        let text = "Первичный осмотр\nФ.И.О. ____________________\nДиагноз: ____________";
        let fields =
            infer_legacy_template_fields(text, Some(&DomainKind::Medical), Some("primary"));
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_id, "subject.name");
        assert_eq!(fields[1].field_id, "medical.diagnosis");
        assert_eq!(fields[0].confidence, 1.0);
    }

    #[test]
    fn domain_disambiguates_position_without_medicalizing_other_profiles() {
        let hr = infer_legacy_template_fields(
            "Приказ\nДолжность: __________",
            Some(&DomainKind::Hr),
            Some("unknown"),
        );
        assert_eq!(hr.len(), 1);
        assert_eq!(hr[0].field_id, "employee.position");

        let legal = infer_legacy_template_fields(
            "Договор\nДолжность: __________",
            Some(&DomainKind::Legal),
            Some("unknown"),
        );
        assert_eq!(legal.len(), 1);
        assert_eq!(legal[0].field_id, "subject.position");
        assert_ne!(legal[0].field_id, "employee.position");
        assert_ne!(legal[0].field_id, "medical.position");
    }

    #[test]
    fn infers_unique_blank_on_immediately_following_line() {
        let fields = infer_legacy_template_fields(
            "Ф.И.О.\n_____________________",
            Some(&DomainKind::Generic),
            None,
        );
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_id, "subject.name");
        assert!(fields[0].common_prefix.is_empty());
    }

    #[test]
    fn does_not_infer_filled_values_or_existing_placeholders() {
        let fields = infer_legacy_template_fields(
            "Ф.И.О. Иванов Иван Иванович\nФ.И.О. {{subject.name}}",
            Some(&DomainKind::Generic),
            None,
        );
        assert!(fields.is_empty());
    }

    #[test]
    fn repeated_blank_only_targets_fail_closed() {
        let fields = infer_legacy_template_fields(
            "Ф.И.О.\n__________\nДата рождения\n__________",
            Some(&DomainKind::Generic),
            None,
        );
        assert!(fields.is_empty());
    }

    #[test]
    fn broad_unqualified_date_is_not_guessed() {
        let fields = infer_legacy_template_fields(
            "Дата: __________",
            Some(&DomainKind::Medical),
            Some("commission"),
        );
        assert!(fields.is_empty());
    }
}
