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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StructuralAnchorMode {
    #[default]
    Prefix,
    Contains,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabeledTemplateValueCandidate {
    pub field_id: String,
    pub title: String,
    pub line_index: usize,
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub anchor_mode: StructuralAnchorMode,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone)]
struct LabeledTemplateAnchor {
    field_id: String,
    label: String,
    remainder: String,
    replaceable: bool,
}

/// Infer variable values from the *structure* of an already-filled template.
///
/// This is the donor-style counterpart to `infer_legacy_template_fields`: instead
/// of first trying to understand the old patient's value, it recognizes a stable
/// registry label/section (`Ф.И.О.`, `Диагноз`, `Лечение`, ...), then treats the
/// visible content owned by that section as replaceable template data.  This is
/// intentionally domain-aware and role-aware, and signer labels are boundaries
/// only: a doctor's fixed name is never silently converted into patient data.
pub fn infer_labeled_template_values(
    text: &str,
    preferred_domain: Option<&DomainKind>,
    role_id: Option<&str>,
) -> Vec<LabeledTemplateValueCandidate> {
    let lines = text.lines().map(str::trim_end).collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }
    let catalog = template_label_catalog(preferred_domain);
    let anchors = lines
        .iter()
        .map(|line| match_labeled_template_anchor(line.trim(), &catalog, preferred_domain, role_id))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();

    for (line_index, anchor) in anchors.iter().enumerate() {
        let Some(anchor) = anchor else {
            continue;
        };
        if !anchor.replaceable {
            continue;
        }
        let inline = clean_structural_value(&anchor.remainder);
        let multiline = is_multiline_structural_field(&anchor.field_id);
        let mut owned = Vec::new();
        if !inline.is_empty() && !is_blank_only(&inline) && !inline.contains("{{") {
            owned.push(inline);
        }
        if multiline || owned.is_empty() {
            for next_index in line_index + 1..lines.len() {
                if anchors[next_index].is_some() {
                    break;
                }
                let next = lines[next_index].trim();
                if next.is_empty() {
                    continue;
                }
                if next.contains("{{") || next.contains("}}") {
                    break;
                }
                owned.push(next.to_string());
                if !multiline {
                    break;
                }
            }
        }
        let value = owned.join("\n").trim().to_string();
        if value.is_empty() || is_blank_only(&value) || value.contains("{{") || value.contains("}}")
        {
            continue;
        }
        candidates.push(LabeledTemplateValueCandidate {
            field_id: anchor.field_id.clone(),
            title: title_for_field(&anchor.field_id),
            line_index,
            label: anchor.label.clone(),
            value,
            anchor_mode: StructuralAnchorMode::Prefix,
            confidence: 0.995,
            reason: "устойчивая подпись/секция шаблона определяет место значения независимо от данных старого документа".into(),
        });
    }

    // Repeated visual fields are kept as separate anchors. The DOCX compiler
    // must rewrite every owned location, not just the first identical value.
    // A paragraph may already contain one semantic placeholder while another
    // legacy field in the same paragraph is still literal, for example
    // `Диагноз: {{medical.diagnosis}}; Лечение: старая схема`. The whole-line
    // anchor intentionally fails closed on placeholders, so inspect only
    // explicitly separated placeholder-free segments and bind them by containment.
    for (line_index, raw_line) in lines.iter().enumerate() {
        if !(raw_line.contains("{{") || raw_line.contains("}}")) {
            continue;
        }
        for segment in raw_line.split(';') {
            let segment = segment.trim();
            if segment.is_empty() || segment.contains("{{") || segment.contains("}}") {
                continue;
            }
            let Some(anchor) =
                match_labeled_template_anchor(segment, &catalog, preferred_domain, role_id)
            else {
                continue;
            };
            if !anchor.replaceable {
                continue;
            }
            let value = clean_structural_value(&anchor.remainder);
            if value.is_empty() || is_blank_only(&value) {
                continue;
            }
            candidates.push(LabeledTemplateValueCandidate {
                field_id: anchor.field_id.clone(),
                title: title_for_field(&anchor.field_id),
                line_index,
                label: anchor.label,
                value,
                anchor_mode: StructuralAnchorMode::Contains,
                confidence: 0.995,
                reason: "явно разделённый сегмент частично динамического абзаца сохраняет собственную подпись и старое значение".into(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        left.line_index
            .cmp(&right.line_index)
            .then_with(|| left.field_id.cmp(&right.field_id))
    });
    candidates
}

/// Complete structural inference used by the DOCX compiler. The generic label
/// binder remains profession-neutral; the donor compatibility layer adds only
/// stable legacy *layout* patterns for known medical roles.
pub fn infer_structural_template_values(
    text: &str,
    preferred_domain: Option<&DomainKind>,
    role_id: Option<&str>,
) -> Vec<LabeledTemplateValueCandidate> {
    let mut candidates = infer_labeled_template_values(text, preferred_domain, role_id);
    if matches!(preferred_domain, Some(DomainKind::Medical)) {
        candidates.extend(infer_donor_medical_role_values(
            text,
            role_id.unwrap_or_default(),
        ));
    }
    candidates.sort_by(|left, right| {
        left.line_index
            .cmp(&right.line_index)
            .then_with(|| left.field_id.cmp(&right.field_id))
            .then_with(|| left.value.cmp(&right.value))
    });
    candidates.dedup_by(|left, right| {
        left.line_index == right.line_index
            && left.field_id == right.field_id
            && left.value == right.value
    });
    candidates
}

fn infer_donor_medical_role_values(
    text: &str,
    role_id: &str,
) -> Vec<LabeledTemplateValueCandidate> {
    let canonical_role = crate::domains::medical::canonical_medical_role(role_id);
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    let mut out = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let folded = fold_label(line);
        if canonical_role == "discharge" && folded.contains("выписной эпикриз") {
            if let Some(date) = leading_full_date(line) {
                push_donor_candidate(
                    &mut out,
                    "medical.discharge_date",
                    line_index,
                    "Выписной эпикриз",
                    date,
                    "donor discharge header date",
                );
            }
            if let Some(number) = token_after_phrase_marker(line, "Выписной эпикриз", '№')
            {
                push_donor_candidate(
                    &mut out,
                    "medical.case_number",
                    line_index,
                    "Выписной эпикриз",
                    number,
                    "donor discharge header case number",
                );
            }
        }
        if canonical_role == "primary" && folded.contains("первичный осмотр") {
            if let Some(date) = leading_full_date(line) {
                push_donor_candidate(
                    &mut out,
                    "medical.admission_date",
                    line_index,
                    "Первичный осмотр",
                    date,
                    "donor primary header date",
                );
            }
        }
        if folded.contains("зарегистрирован по адресу") && line.contains(',')
        {
            let mut parts = line.splitn(2, ',');
            if let Some(name) = parts.next().map(str::trim).filter(|value| value.len() >= 3) {
                push_donor_candidate(
                    &mut out,
                    "subject.name",
                    line_index,
                    "зарегистрирован по адресу",
                    name.to_string(),
                    "donor combined patient identity line",
                );
            }
            if let Some(before_address) = folded.find("зарегистрирован по адресу")
            {
                let original_prefix =
                    &line[..char_boundary_for_folded_prefix(line, before_address)];
                if let Some(birth) = extract_birth_from_person_prefix(original_prefix) {
                    push_donor_candidate(
                        &mut out,
                        "subject.birth_date",
                        line_index,
                        "зарегистрирован по адресу",
                        birth,
                        "donor combined patient birth line",
                    );
                }
            }
            if let Some(address) = text_after_phrase(line, "зарегистрирован по адресу")
            {
                push_donor_candidate(
                    &mut out,
                    "subject.address",
                    line_index,
                    "зарегистрирован по адресу",
                    address,
                    "donor combined patient address line",
                );
            }
        }
        if folded.contains("находился на лечении") {
            if let Some((from, to)) = extract_period_dates(line) {
                push_donor_candidate(
                    &mut out,
                    "medical.admission_date",
                    line_index,
                    "Находился на лечении",
                    from,
                    "donor treatment period start",
                );
                push_donor_candidate(
                    &mut out,
                    "medical.discharge_date",
                    line_index,
                    "Находился на лечении",
                    to,
                    "donor treatment period end",
                );
            }
        }
    }
    out
}

fn push_donor_candidate(
    out: &mut Vec<LabeledTemplateValueCandidate>,
    field_id: &str,
    line_index: usize,
    label: &str,
    value: String,
    reason: &str,
) {
    let value = value
        .trim()
        .trim_matches(|ch: char| ch == ',' || ch == ';')
        .trim();
    if value.is_empty() || value.contains("{{") || value.contains("}}") {
        return;
    }
    out.push(LabeledTemplateValueCandidate {
        field_id: field_id.to_string(),
        title: title_for_field(field_id),
        line_index,
        label: label.to_string(),
        value: value.to_string(),
        anchor_mode: StructuralAnchorMode::Contains,
        confidence: 0.999,
        reason: reason.to_string(),
    });
}

fn leading_full_date(line: &str) -> Option<String> {
    let token = line.split_whitespace().next()?;
    let bytes = token.as_bytes();
    if bytes.len() == 10
        && bytes[0..2].iter().all(u8::is_ascii_digit)
        && bytes[2] == b'.'
        && bytes[3..5].iter().all(u8::is_ascii_digit)
        && bytes[5] == b'.'
        && bytes[6..10].iter().all(u8::is_ascii_digit)
    {
        Some(token.to_string())
    } else {
        None
    }
}

fn token_after_phrase_marker(line: &str, phrase: &str, marker: char) -> Option<String> {
    let folded = fold_label(line);
    let phrase_folded = fold_label(phrase);
    let start = folded.find(&phrase_folded)?;
    let original_start = char_boundary_for_folded_prefix(line, start);
    let after_phrase = original_start
        + line[original_start..]
            .chars()
            .take(phrase.chars().count())
            .map(char::len_utf8)
            .sum::<usize>();
    let tail = line.get(after_phrase..)?;
    let marker_index = tail.find(marker)?;
    let after_marker = tail.get(marker_index + marker.len_utf8()..)?.trim_start();
    let token = after_marker
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| matches!(ch, ',' | ';' | ':' | '(' | ')' | '[' | ']'));
    (!token.is_empty()).then(|| token.to_string())
}

fn text_after_phrase(line: &str, phrase: &str) -> Option<String> {
    let folded = fold_label(line);
    let phrase_folded = fold_label(phrase);
    let start = folded.find(&phrase_folded)?;
    let original_start = char_boundary_for_folded_prefix(line, start);
    let after_phrase = original_start
        + line[original_start..]
            .chars()
            .take(phrase.chars().count())
            .map(char::len_utf8)
            .sum::<usize>();
    let value = line
        .get(after_phrase..)?
        .trim()
        .trim_start_matches([':', '-', '–', '—'])
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn char_boundary_for_folded_prefix(original: &str, folded_byte_index: usize) -> usize {
    // Russian lowercasing and ё→е preserve UTF-8 character count for the donor
    // markers used here. Convert the folded byte prefix to a character count,
    // then map that count back to the original UTF-8 boundary.
    let folded = fold_label(original);
    let chars = folded[..folded_byte_index].chars().count();
    original
        .char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(original.len())
}

fn extract_birth_from_person_prefix(prefix: &str) -> Option<String> {
    let before_gr = prefix
        .to_lowercase()
        .find("г.р.")
        .and_then(|index| prefix.get(..index))
        .unwrap_or(prefix);
    before_gr
        .split(',')
        .skip(1)
        .map(str::trim)
        .find(|value| leading_full_date(value).is_some())
        .map(str::to_string)
}

fn extract_period_dates(line: &str) -> Option<(String, String)> {
    let tokens = line
        .split_whitespace()
        .filter_map(|token| {
            token
                .trim_matches(|ch: char| matches!(ch, ',' | ';' | '.' | '(' | ')'))
                .to_string()
                .into()
        })
        .collect::<Vec<String>>();
    let dates = tokens
        .iter()
        .filter(|token| leading_full_date(token).is_some())
        .cloned()
        .collect::<Vec<_>>();
    (dates.len() >= 2).then(|| {
        (
            dates[dates.len() - 2].clone(),
            dates[dates.len() - 1].clone(),
        )
    })
}

fn template_label_catalog(preferred_domain: Option<&DomainKind>) -> Vec<(String, bool)> {
    let mut labels = Vec::<(String, bool)>::new();
    for definition in crate::all_fields() {
        let allowed = match preferred_domain {
            None => true,
            Some(DomainKind::Generic | DomainKind::Custom(_)) => {
                definition.domain == DomainKind::Generic
            }
            Some(domain) => {
                definition.domain == DomainKind::Generic || definition.domain == *domain
            }
        };
        if !allowed {
            continue;
        }
        let replaceable = !matches!(
            definition.id.as_str(),
            "medical.attending_doctor" | "medical.department_head"
        ) && !definition.id.starts_with("doctor.");
        for label in std::iter::once(definition.title_ru).chain(definition.aliases) {
            let label = clean_label(&label);
            if label.is_empty()
                || is_too_generic_label(&label)
                || label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
            {
                continue;
            }
            labels.push((label, replaceable));
        }
    }
    // Combined work/position is a real legacy visual field but has no registry
    // definition of its own.
    if matches!(preferred_domain, None | Some(DomainKind::Medical)) {
        labels.extend([
            ("Место работы / должность".to_string(), true),
            ("Место работы, должность".to_string(), true),
        ]);
    }
    labels.sort_by(|left, right| {
        right
            .0
            .chars()
            .count()
            .cmp(&left.0.chars().count())
            .then_with(|| left.0.cmp(&right.0))
    });
    labels.dedup_by(|left, right| fold_label(&left.0) == fold_label(&right.0));
    labels
}

fn match_labeled_template_anchor(
    line: &str,
    catalog: &[(String, bool)],
    preferred_domain: Option<&DomainKind>,
    role_id: Option<&str>,
) -> Option<LabeledTemplateAnchor> {
    if line.is_empty() || line.contains("{{") || line.contains("}}") {
        return None;
    }
    if let Some(label) = signer_boundary_label(line) {
        return Some(LabeledTemplateAnchor {
            field_id: String::new(),
            label: label.to_string(),
            remainder: String::new(),
            replaceable: false,
        });
    }
    for (label, replaceable) in catalog {
        let Some(remainder) = strip_label_prefix(line, label) else {
            continue;
        };
        let Some(field_id) = resolve_label(label, preferred_domain, role_id) else {
            continue;
        };
        return Some(LabeledTemplateAnchor {
            field_id,
            label: label.clone(),
            remainder: remainder.to_string(),
            replaceable: *replaceable,
        });
    }
    None
}

fn signer_boundary_label(line: &str) -> Option<&'static str> {
    const SIGNER_LABELS: &[&str] = &[
        "Заведующий отделением",
        "Зав. отделением",
        "Зав. отд.",
        "Лечащий врач",
        "Врач-психиатр",
        "Врач психиатр",
        "Заместитель главного врача",
        "Зам. главного врача",
    ];
    SIGNER_LABELS
        .iter()
        .copied()
        .find(|label| strip_label_prefix(line, label).is_some())
}

fn is_multiline_structural_field(field_id: &str) -> bool {
    matches!(
        field_id,
        "medical.complaints"
            | "medical.anamnesis_life"
            | "medical.anamnesis_disease"
            | "medical.epidemiology"
            | "medical.profile_observation"
            | "medical.profile_status"
            | "medical.somatic_status"
            | "medical.examination_plan"
            | "medical.treatment"
            | "medical.treatment_result"
            | "medical.recommendations"
            | "medical.labs"
    )
}

fn strip_label_prefix<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let line = line.trim_start();
    let wanted_chars = label.chars().count();
    let prefix = line.chars().take(wanted_chars).collect::<String>();
    if prefix.chars().count() != wanted_chars || fold_label(&prefix) != fold_label(label) {
        return None;
    }
    let remainder = &line[prefix.len()..];
    if let Some(first) = remainder.chars().next() {
        if !(first.is_whitespace()
            || matches!(first, ':' | ';' | ',' | '.' | '-' | '–' | '—' | '№' | '('))
        {
            return None;
        }
    }
    Some(remainder)
}

fn clean_structural_value(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, ':' | ';' | ',' | '.' | '-' | '–' | '—' | '№')
        })
        .trim()
        .to_string()
}

fn fold_label(value: &str) -> String {
    value.trim().to_lowercase().replace('ё', "е")
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
    if field_id == crate::MEDICAL_WORK_POSITION
        && matches!(preferred_domain, None | Some(DomainKind::Medical))
    {
        return Some(field_id);
    }
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
    fn donor_style_filled_discharge_is_bound_by_labels_not_old_patient_parsing() {
        let text = concat!(
            "Выписной эпикриз\n",
            "Ф.И.О.: Иванов Иван Иванович\n",
            "Номер истории болезни: АБ-4213/26\n",
            "Дата поступления: 01.09.2026\n",
            "Диагноз: F20 — локальная формулировка с нестандартным текстом\n",
            "Дата выписки: 09.09.2026\n",
            "Лечение:\n",
            "Необычная авторская схема 1\n",
            "Необычная авторская схема 2\n",
            "Место работы: ООО Ромашка\n",
            "Должность: инженер-конструктор\n",
            "Состояние при выписке: улучшение\n",
            "Зав. отделением Петров П.П.\n",
            "Врач-психиатр Иванов И.И."
        );
        let candidates =
            infer_labeled_template_values(text, Some(&DomainKind::Medical), Some("discharge"));
        let by_id = candidates
            .iter()
            .map(|candidate| (candidate.field_id.as_str(), candidate.value.as_str()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(by_id.get("subject.name"), Some(&"Иванов Иван Иванович"));
        assert_eq!(by_id.get("medical.case_number"), Some(&"АБ-4213/26"));
        assert_eq!(by_id.get("medical.admission_date"), Some(&"01.09.2026"));
        assert_eq!(
            by_id.get("medical.diagnosis"),
            Some(&"F20 — локальная формулировка с нестандартным текстом")
        );
        assert_eq!(by_id.get("medical.discharge_date"), Some(&"09.09.2026"));
        assert_eq!(
            by_id.get("medical.treatment"),
            Some(&"Необычная авторская схема 1\nНеобычная авторская схема 2")
        );
        assert_eq!(by_id.get("medical.workplace"), Some(&"ООО Ромашка"));
        assert_eq!(by_id.get("medical.position"), Some(&"инженер-конструктор"));
        assert_eq!(by_id.get("medical.discharge_condition"), Some(&"улучшение"));
        assert!(!by_id.contains_key("medical.department_head"));
        assert!(!by_id.contains_key("medical.attending_doctor"));
    }

    #[test]
    fn structural_binding_works_even_when_template_is_already_partly_dynamic() {
        let candidates = infer_labeled_template_values(
            "Выписной эпикриз\n{{medical.expert_anamnesis}}\nДиагноз: F20\nЛечение: donor text",
            Some(&DomainKind::Medical),
            Some("discharge"),
        );
        let fields = candidates
            .iter()
            .map(|candidate| candidate.field_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(fields.contains("medical.diagnosis"));
        assert!(fields.contains("medical.treatment"));
    }

    #[test]
    fn donor_composite_line_with_existing_placeholder_still_binds_remaining_values() {
        let text = concat!(
            "09.09.2026 Выписной эпикриз № {{medical.case_number}}\n",
            "{{subject.name}}, 01.01.1980 г.р., зарегистрирован по адресу: Н. Новгород"
        );
        let candidates =
            infer_structural_template_values(text, Some(&DomainKind::Medical), Some("discharge"));
        let fields = candidates
            .iter()
            .map(|candidate| (candidate.field_id.as_str(), candidate.value.as_str()))
            .collect::<BTreeSet<_>>();
        assert!(fields.contains(&("medical.discharge_date", "09.09.2026")));
        assert!(fields.contains(&("subject.birth_date", "01.01.1980")));
        assert!(fields.contains(&("subject.address", "Н. Новгород")));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.field_id == "medical.case_number"));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.field_id == "subject.name"));
    }

    #[test]
    fn donor_composite_discharge_lines_become_structural_bindings() {
        let text = concat!(
            "09.09.2026      Выписной эпикриз № 4213\n",
            "Иванов Иван Иванович, 01.01.1980 г.р., зарегистрирован по адресу: Н. Новгород\n",
            "Находился на лечении в ГБУЗ НО «НКЦПЗ» диспансер №2 с 01.09.2026 по 09.09.2026\n",
            "Диагноз: F20\n",
            "Лечение: терапия\n",
            "Экспертный анамнез: Работает в Завод, в должности инженер."
        );
        let bindings =
            infer_structural_template_values(text, Some(&DomainKind::Medical), Some("discharge"));
        let values = bindings
            .iter()
            .map(|binding| (binding.field_id.as_str(), binding.value.as_str()))
            .collect::<Vec<_>>();
        for expected in [
            ("medical.case_number", "4213"),
            ("medical.discharge_date", "09.09.2026"),
            ("subject.name", "Иванов Иван Иванович"),
            ("subject.birth_date", "01.01.1980"),
            ("subject.address", "Н. Новгород"),
            ("medical.admission_date", "01.09.2026"),
            ("medical.diagnosis", "F20"),
            ("medical.treatment", "терапия"),
        ] {
            assert!(
                values.contains(&expected),
                "missing {expected:?}: {values:?}"
            );
        }
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.field_id == "medical.discharge_date")
                .count(),
            2,
            "both donor discharge-date locations must stay bound"
        );
        assert!(bindings.iter().any(|binding| {
            binding.field_id == "medical.case_number"
                && binding.anchor_mode == StructuralAnchorMode::Contains
        }));
    }

    #[test]
    fn partially_dynamic_semicolon_paragraph_binds_remaining_literal_field() {
        let candidates = infer_labeled_template_values(
            "Диагноз: {{medical.diagnosis}}; Лечение: старая схема",
            Some(&DomainKind::Medical),
            Some("discharge"),
        );
        let treatment = candidates
            .iter()
            .find(|candidate| candidate.field_id == "medical.treatment")
            .expect("literal treatment segment must remain bindable");
        assert_eq!(treatment.value, "старая схема");
        assert_eq!(treatment.anchor_mode, StructuralAnchorMode::Contains);
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.field_id == "medical.diagnosis"));
    }

    #[test]
    fn discharge_case_number_is_bound_to_heading_marker_not_later_department_marker() {
        let bindings = infer_structural_template_values(
            "09.09.2026 Выписной эпикриз № 4213, отделение № 2",
            Some(&DomainKind::Medical),
            Some("discharge"),
        );
        let case_number = bindings
            .iter()
            .find(|binding| binding.field_id == "medical.case_number")
            .expect("case number binding");
        assert_eq!(case_number.value, "4213");
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
