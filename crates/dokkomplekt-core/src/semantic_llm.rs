//! Semantic understanding layer — the model-backed strategy of the parser.
//!
//! The deterministic engine ([`crate::semantic_engine`]) covers structured and
//! labelled documents. To actually *understand* free-form text — any wording, any
//! layout — you need a model. This module makes that a first-class, pluggable
//! strategy with one non-negotiable rule: **the model may propose, but every value
//! is re-validated through the same typed validators before it is accepted.** A
//! hallucinated ИНН fails its checksum, a garbage date fails to parse, an unknown
//! field id is ignored — so the model can raise recall without ever fabricating a
//! value the pipeline would trust. That is what keeps zero-touch honest.
//!
//! Transport is injected via [`SemanticModel`]. It can be a local on-device model
//! (recommended for patient data — nothing leaves the machine), a self-hosted
//! endpoint, or a cloud API. The core never assumes a network and never sends data
//! anywhere by itself.

use serde::Deserialize;

use crate::semantic_engine::{field_type_for, normalize_typed, schema_entries_for};
use crate::{
    canonical_storage_field_id, is_valid_field_id, merge_value, validate_case_relations,
    validate_field_value, ExtractedField, ExtractionReport, SemanticCase, SemanticValue,
    ValueEvidence, ValueSource,
};

/// A pluggable semantic model. The app provides the transport; the core provides
/// the prompt, the parsing, the validation and the merge.
pub trait SemanticModel {
    /// Complete a prompt and return the raw model text (expected to contain JSON).
    fn complete(&self, prompt: &str) -> Result<String, String>;
}

/// Build the extraction prompt: the model is asked to return STRICT JSON mapping
/// canonical field ids to `{value, confidence}`, only for fields it can support
/// from the text, and to never invent values.
pub fn build_extraction_prompt(text: &str) -> String {
    build_extraction_prompt_for_domain_and_language(text, &crate::DomainKind::Generic, "auto")
}

/// Build a schema-constrained extraction prompt for the source's active domain.
pub fn build_extraction_prompt_for_domain(text: &str, domain: &crate::DomainKind) -> String {
    build_extraction_prompt_for_domain_and_language(text, domain, "auto")
}

/// Build a multilingual schema-constrained prompt. Canonical identifiers stay
/// stable, while values and evidence remain verbatim in the source language.
pub fn build_extraction_prompt_for_domain_and_language(
    text: &str,
    domain: &crate::DomainKind,
    language: &str,
) -> String {
    let mut lines = String::new();
    for (id, _ftype, hint) in schema_entries_for(domain) {
        lines.push_str(&format!("- {id} ({hint})\n"));
    }
    let requested_language = normalize_prompt_language(language, text);
    format!(
        "You are a precise multilingual document data extractor. Extract values ONLY from the document text below.\n\
         Document language or writing system: {requested_language}. Preserve every value and evidence quotation in the source language; do not translate names, addresses, diagnoses, clauses, organizations or free text.\n\
         Return STRICT JSON only, without explanations or markdown fences, shaped as:\n\
         {{\"field_id\": {{\"value\": \"...\", \"confidence\": 0.0-1.0, \"evidence\": \"short verbatim quotation\"}}}}\n\
         Rules: use only the canonical fields listed below; omit fields not present in the text; \
         evidence is mandatory and must be a short verbatim quotation directly supporting value; \
         НИКОГДА не выдумывай (NEVER invent), infer or translate a value; normalize dates to DD.MM.YYYY when the source supplies a complete date.\n\n\
         Canonical fields:\n{lines}\n\
         === DOCUMENT TEXT ===\n{text}\n=== END DOCUMENT ==="
    )
}

fn normalize_prompt_language(language: &str, text: &str) -> String {
    let language = language.trim();
    if !language.is_empty() && !language.eq_ignore_ascii_case("auto") {
        return language.chars().take(32).collect();
    }
    let mut cyrillic = 0usize;
    let mut latin = 0usize;
    let mut arabic = 0usize;
    let mut cjk = 0usize;
    for ch in text.chars().take(200_000) {
        match ch as u32 {
            0x0400..=0x052f => cyrillic += 1,
            0x0041..=0x024f => latin += 1,
            0x0600..=0x06ff => arabic += 1,
            0x3400..=0x9fff | 0x3040..=0x30ff | 0xac00..=0xd7af => cjk += 1,
            _ => {}
        }
    }
    let (name, count) = [
        ("Cyrillic (auto-detected)", cyrillic),
        ("Latin (auto-detected)", latin),
        ("Arabic (auto-detected)", arabic),
        ("CJK (auto-detected)", cjk),
    ]
    .into_iter()
    .max_by_key(|(_, count)| *count)
    .unwrap_or(("unknown/mixed (auto-detected)", 0));
    if count == 0 {
        "unknown/mixed (auto-detected)".into()
    } else {
        name.into()
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawModelValue {
    Rich {
        value: String,
        #[serde(default)]
        confidence: f32,
        #[serde(default)]
        evidence: Option<String>,
    },
    Plain(String),
}

/// Parse and *validate* a model's JSON extraction. Every value is re-checked
/// against its field type; anything invalid or unknown is dropped.
pub fn parse_model_extraction(
    model_output: &str,
    default_year: i32,
) -> (SemanticCase, ExtractionReport) {
    parse_model_extraction_with_source(model_output, None, default_year)
}

/// Parse model output while binding every claimed quotation to the actual source.
/// A supplied quotation that is not literally present causes the field to be
/// rejected. Model values without a quotation remain low-confidence proposals;
/// risk-gates will not silently accept them for high-risk fields.
pub fn parse_model_extraction_with_source(
    model_output: &str,
    source_text: Option<&str>,
    default_year: i32,
) -> (SemanticCase, ExtractionReport) {
    let mut case = SemanticCase::default();
    let mut report = ExtractionReport::default();

    let cleaned = strip_code_fences(model_output);
    let Ok(map) =
        serde_json::from_str::<std::collections::BTreeMap<String, RawModelValue>>(&cleaned)
    else {
        report
            .warnings
            .push("Ответ модели не является корректным JSON — семантический слой пропущен".into());
        return (case, report);
    };

    for (raw_field_id, raw) in map {
        let field_id = canonical_storage_field_id(&raw_field_id);
        let (raw_value, model_conf, evidence) = match raw {
            RawModelValue::Rich {
                value,
                confidence,
                evidence,
            } => (
                value,
                confidence.clamp(0.0, 1.0),
                evidence
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty()),
            ),
            RawModelValue::Plain(value) => (value, 0.7, None),
        };
        let raw_value = raw_value.trim().to_string();
        if raw_value.is_empty() {
            continue;
        }

        let verified_evidence = match (source_text, evidence) {
            (Some(source), Some(excerpt)) => {
                if !excerpt_is_present(source, &excerpt) {
                    report.warnings.push(format!(
                        "Цитата модели для «{field_id}» отсутствует в исходном тексте после нормализации пробелов; значение отклонено"
                    ));
                    continue;
                }
                Some(excerpt)
            }
            (_, evidence) => evidence,
        };

        // Known canonical field -> validate by its type. Otherwise accept only a
        // syntactically valid field id, as light free text.
        let (value, mut confidence) = match field_type_for(&field_id) {
            Some(ftype) => {
                let Some((normalized, boost)) = normalize_typed(ftype, &raw_value, default_year)
                else {
                    report.warnings.push(format!(
                        "Значение модели для «{field_id}» не прошло проверку типа и отклонено"
                    ));
                    continue;
                };
                let base = (model_conf * 0.9).min(0.95);
                (normalized, (base + boost).min(0.99))
            }
            None => {
                if !is_valid_field_id(&field_id) {
                    continue;
                }
                (raw_value.clone(), (model_conf * 0.7).min(0.8))
            }
        };

        if let Some(excerpt) = verified_evidence.as_deref() {
            if !value_is_supported_by_excerpt(&field_id, &raw_value, &value, excerpt) {
                report.warnings.push(format!(
                    "Цитата модели для «{field_id}» найдена, но не подтверждает предложенное значение; значение отклонено"
                ));
                continue;
            }
        } else if source_text.is_some() {
            confidence = confidence.min(0.49);
            report.warnings.push(format!(
                "Модель предложила «{field_id}» без проверяемой цитаты; поле оставлено только как ручное предложение и не может пройти zero-touch"
            ));
        }

        if let Err(reason) = validate_field_value(&field_id, &value) {
            report.warnings.push(format!(
                "Значение модели для «{field_id}» отклонено: {reason}"
            ));
            continue;
        }

        let mut semantic_value =
            SemanticValue::new(&field_id, &value, ValueSource::Model, confidence);
        if let Some(excerpt) = verified_evidence {
            semantic_value = semantic_value.with_evidence(ValueEvidence::new(
                "semantic_model",
                excerpt,
                "local_semantic_model",
                confidence,
            ));
        }
        merge_value(&mut case, semantic_value);
        report.fields.push(ExtractedField {
            field_id,
            value,
            confidence,
            method: "model".into(),
        });
    }
    for (field_id, error) in validate_case_relations(&case) {
        case.values.remove(&field_id);
        report.fields.retain(|field| field.field_id != field_id);
        report.warnings.push(format!(
            "Значение модели для «{field_id}» отклонено: {error}"
        ));
    }
    report.fields.sort_by(|a, b| a.field_id.cmp(&b.field_id));
    (case, report)
}

fn normalize_grounding_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_was_space = false;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let mapped = match ch {
            '\u{00a0}' | '\u{2007}' | '\u{202f}' | '\r' | '\n' | '\t' => ' ',
            '«' | '»' | '„' | '“' | '”' | '‟' => '"',
            '—' | '–' | '−' => '-',
            other => other,
        };
        if mapped.is_whitespace() {
            if !previous_was_space && !out.is_empty() {
                out.push(' ');
            }
            previous_was_space = true;
        } else {
            out.push(mapped);
            previous_was_space = false;
        }
    }
    out.trim().to_string()
}

fn compact_alphanumeric(text: &str) -> String {
    normalize_grounding_text(text)
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn meaningful_tokens(text: &str) -> Vec<String> {
    normalize_grounding_text(text)
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
        .filter(|token| {
            !matches!(
                *token,
                "ооо" | "ао" | "пао" | "ип" | "от" | "до" | "на" | "по" | "для"
            )
        })
        .map(str::to_owned)
        .collect()
}

fn excerpt_is_present(source: &str, excerpt: &str) -> bool {
    let normalized_excerpt = normalize_grounding_text(excerpt);
    !normalized_excerpt.is_empty() && normalize_grounding_text(source).contains(&normalized_excerpt)
}

fn value_is_supported_by_excerpt(
    field_id: &str,
    raw_value: &str,
    normalized_value: &str,
    excerpt: &str,
) -> bool {
    let excerpt_normalized = normalize_grounding_text(excerpt);
    let compact_excerpt = compact_alphanumeric(excerpt);
    for candidate in [raw_value, normalized_value] {
        let candidate_normalized = normalize_grounding_text(candidate);
        if candidate_normalized.chars().count() >= 2
            && excerpt_normalized.contains(&candidate_normalized)
        {
            return true;
        }
        let compact = compact_alphanumeric(candidate);
        if compact.chars().count() >= 4 && compact_excerpt.contains(&compact) {
            return true;
        }
    }

    let value_tokens = meaningful_tokens(raw_value);
    if value_tokens.is_empty() {
        return false;
    }
    let excerpt_tokens = meaningful_tokens(excerpt);
    let matched = value_tokens
        .iter()
        .filter(|token| excerpt_tokens.iter().any(|candidate| candidate == *token))
        .count();

    // Names must be fully localized. Partial-token grounding previously allowed
    // a correct surname/name with a hallucinated patronymic to pass.
    if field_id == "subject.name" || field_id.ends_with(".person_name") {
        return matched == value_tokens.len();
    }

    // Addresses must preserve every numeric locator (postal code, house, flat,
    // office) and almost all lexical tokens. A matching city/street is not proof
    // of a different house or apartment.
    if field_id.contains("address") {
        let value_numbers = numeric_tokens(raw_value);
        let excerpt_numbers = numeric_tokens(excerpt);
        if value_numbers
            .iter()
            .any(|number| !excerpt_numbers.iter().any(|candidate| candidate == number))
        {
            return false;
        }
        return matched * 10 >= value_tokens.len() * 8;
    }

    // Diagnosis and free-text conclusions are semantically risky: require all
    // significant claimed tokens to be present in the quoted evidence.
    if field_id.contains("diagnosis") || field_id.contains("conclusion") {
        return matched == value_tokens.len();
    }

    matched * 10 >= value_tokens.len() * 8
}

fn numeric_tokens(text: &str) -> Vec<String> {
    normalize_grounding_text(text)
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn strip_code_fences(text: &str) -> String {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    // keep only the outermost JSON object if the model added prose around it
    if let (Some(start), Some(end)) = (t.find('{'), t.rfind('}')) {
        if end > start {
            return t[start..=end].to_string();
        }
    }
    t.trim().to_string()
}

/// Merge model-derived values into a base case: only override when strictly more confident.
fn merge_confident(
    base: &mut SemanticCase,
    base_report: &mut ExtractionReport,
    add: SemanticCase,
    add_report: ExtractionReport,
) {
    for (field_id, sv) in add.values {
        let better = base
            .values
            .get(&field_id)
            .is_none_or(|ex| sv.confidence > ex.confidence + 0.001);
        if better {
            base.values.insert(field_id, sv);
        }
    }
    for f in add_report.fields {
        base_report.fields.retain(|existing| {
            existing.field_id != f.field_id || existing.confidence >= f.confidence
        });
        if !base_report.fields.iter().any(|e| e.field_id == f.field_id) {
            base_report.fields.push(f);
        }
    }
    for w in add_report.warnings {
        if !base_report.warnings.contains(&w) {
            base_report.warnings.push(w);
        }
    }
    for (field_id, error) in validate_case_relations(base) {
        base.values.remove(&field_id);
        base_report
            .fields
            .retain(|field| field.field_id != field_id);
        base_report.warnings.push(format!(
            "Поле «{field_id}» отклонено после объединения: {error}"
        ));
    }
    base_report
        .fields
        .sort_by(|a, b| a.field_id.cmp(&b.field_id));
}

/// Unified "parser-parser" entry: always run the deterministic engine; if a model
/// output is supplied, validate and merge it on top (higher confidence wins).
pub fn extract_understanding(
    text: &str,
    default_year: i32,
    model_output: Option<&str>,
) -> (SemanticCase, ExtractionReport) {
    let (mut case, mut report) = crate::extract_semantic(text, default_year);
    if let Some(model_output) = model_output {
        let (mcase, mreport) =
            parse_model_extraction_with_source(model_output, Some(text), default_year);
        merge_confident(&mut case, &mut report, mcase, mreport);
    }
    (case, report)
}

/// Convenience: run a live model end-to-end (build prompt -> call -> validate -> merge).
pub fn extract_with_model(
    text: &str,
    default_year: i32,
    model: &dyn SemanticModel,
) -> Result<(SemanticCase, ExtractionReport), String> {
    let prompt = build_extraction_prompt(text);
    let output = model.complete(&prompt)?;
    Ok(extract_understanding(text, default_year, Some(&output)))
}

/// Merge several independently generated model answers. High-risk fields are
/// accepted only when at least two passes agree on the same normalized value.
pub fn apply_model_consensus_with_source(
    case: &mut SemanticCase,
    model_outputs: &[String],
    source_text: &str,
    default_year: i32,
) -> Vec<String> {
    if model_outputs.is_empty() {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    let mut votes: std::collections::BTreeMap<(String, String), Vec<SemanticValue>> =
        std::collections::BTreeMap::new();
    for output in model_outputs {
        let (candidate, report) =
            parse_model_extraction_with_source(output, Some(source_text), default_year);
        warnings.extend(report.warnings);
        for (field_id, value) in candidate.values {
            votes
                .entry((field_id, value.value.clone()))
                .or_default()
                .push(value);
        }
    }

    let passes = model_outputs.len();
    let mut best_by_field: std::collections::BTreeMap<String, (usize, SemanticValue)> =
        std::collections::BTreeMap::new();
    for ((field_id, _), values) in votes {
        let count = values.len();
        let Some(mut selected) = values
            .into_iter()
            .max_by(|left, right| left.confidence.total_cmp(&right.confidence))
        else {
            continue;
        };
        selected.confidence = selected.confidence.min(count as f32 / passes as f32);
        let replace = best_by_field
            .get(&field_id)
            .is_none_or(|(best_count, best)| {
                count > *best_count
                    || (count == *best_count && selected.confidence > best.confidence)
            });
        if replace {
            best_by_field.insert(field_id, (count, selected));
        }
    }

    for (field_id, (count, value)) in best_by_field {
        let high_risk = is_high_risk_model_field(&field_id);
        if high_risk && count < 2 {
            warnings.push(format!(
                "SemanticModel не достигла self-consistency для high-risk поля «{field_id}»; значение не применено"
            ));
            continue;
        }
        let better = case
            .values
            .get(&field_id)
            .is_none_or(|existing| value.confidence > existing.confidence + 0.001);
        if better {
            case.values.insert(field_id, value);
        }
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

fn is_high_risk_model_field(field_id: &str) -> bool {
    const HIGH_RISK_EXACT: &[&str] = &[
        "subject.name",
        "subject.birth_date",
        "subject.snils",
        "medical.diagnosis",
        "medical.icd10",
        "org.inn",
        "org.kpp",
        "org.ogrn",
        "counterparty.inn",
        "counterparty.kpp",
    ];
    let field_id = field_id.to_ascii_lowercase();
    // Contact channels may end in `_number`, but they do not carry the same
    // business consequence as contract, case or document identifiers. Treat
    // them through the normal confidence/evidence gate rather than requiring
    // a second model pass solely because of their spelling.
    let contact_channel = field_id.contains("phone")
        || field_id.contains("telephone")
        || field_id.contains("fax")
        || field_id.contains("email");
    HIGH_RISK_EXACT.contains(&field_id.as_str())
        || field_id.ends_with(".date")
        || field_id.ends_with("_date")
        || field_id.ends_with(".amount")
        || field_id.ends_with("_amount")
        || (!contact_channel && (field_id.ends_with(".number") || field_id.ends_with("_number")))
        || field_id.starts_with("bank.")
}

/// Validate a model output and merge it into an existing case (higher confidence
/// wins). Returns any warnings (e.g. values rejected by type validation). Used to
/// layer semantic understanding on top of the deterministic parse without losing
/// its title/date heuristics.
pub fn apply_model_output(
    case: &mut SemanticCase,
    model_output: &str,
    default_year: i32,
) -> Vec<String> {
    let (mcase, mreport) = parse_model_extraction(model_output, default_year);
    let mut candidate = case.clone();
    for (field_id, sv) in mcase.values {
        let better = candidate
            .values
            .get(&field_id)
            .is_none_or(|ex| sv.confidence > ex.confidence + 0.001);
        if better {
            candidate.values.insert(field_id, sv);
        }
    }
    let mut warnings = mreport.warnings;
    for (field_id, error) in validate_case_relations(&candidate) {
        candidate.values.remove(&field_id);
        warnings.push(format!(
            "Поле «{field_id}» отклонено после объединения: {error}"
        ));
    }
    *case = candidate;
    warnings
}

/// Source-aware variant used by zero-touch and live SemanticModel routes.
pub fn apply_model_output_with_source(
    case: &mut SemanticCase,
    model_output: &str,
    source_text: &str,
    default_year: i32,
) -> Vec<String> {
    let (mcase, mreport) =
        parse_model_extraction_with_source(model_output, Some(source_text), default_year);
    let mut candidate = case.clone();
    for (field_id, sv) in mcase.values {
        let better = candidate
            .values
            .get(&field_id)
            .is_none_or(|existing| sv.confidence > existing.confidence + 0.001);
        if better {
            candidate.values.insert(field_id, sv);
        }
    }
    let mut warnings = mreport.warnings;
    for (field_id, error) in validate_case_relations(&candidate) {
        candidate.values.remove(&field_id);
        warnings.push(format!(
            "Поле «{field_id}» отклонено после объединения: {error}"
        ));
    }
    *case = candidate;
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockModel(&'static str);
    impl SemanticModel for MockModel {
        fn complete(&self, _prompt: &str) -> Result<String, String> {
            Ok(self.0.to_string())
        }
    }

    #[test]
    fn prompt_lists_canonical_fields_and_demands_json() {
        let p = build_extraction_prompt("любой текст");
        assert!(p.contains("document.date"));
        assert!(p.contains("org.inn"));
        assert!(p.contains("JSON"));
        assert!(p.contains("НИКОГДА не выдумывай"));
    }

    #[test]
    fn domain_prompt_hides_unrelated_professional_fields() {
        let hr = build_extraction_prompt_for_domain("Трудовой договор", &crate::DomainKind::Hr);
        assert!(hr.contains("employee.position"));
        assert!(!hr.contains("medical.diagnosis"));
        assert!(!hr.contains("amount.vat"));

        let medical =
            build_extraction_prompt_for_domain("История болезни", &crate::DomainKind::Medical);
        assert!(medical.contains("medical.diagnosis"));
        assert!(!medical.contains("employee.salary"));
    }

    #[test]
    fn suffix_based_risk_does_not_mark_phone_number_high_risk() {
        assert!(!is_high_risk_model_field("subject.phone_number"));
        assert!(is_high_risk_model_field("contract.number"));
        assert!(is_high_risk_model_field("document.date"));
    }

    #[test]
    fn valid_model_json_is_typed_and_normalised() {
        let json = r#"{"document.date":{"value":"21 февраля 2026","confidence":0.9},
                       "amount.total":{"value":"146500","confidence":0.8}}"#;
        let (case, _r) = parse_model_extraction(json, 2026);
        assert_eq!(case.get("document.date"), Some("21.02.2026"));
        assert_eq!(case.get("amount.total"), Some("146\u{00A0}500"));
    }

    #[test]
    fn model_hallucination_is_rejected_by_type_validation() {
        // invalid ИНН checksum + garbage date + bad ICD -> all dropped
        let json = r#"{"org.inn":{"value":"1234567890","confidence":0.99},
                       "document.date":{"value":"позавчера","confidence":0.99},
                       "medical.diagnosis_code":{"value":"ZZZ","confidence":0.99}}"#;
        let (case, report) = parse_model_extraction(json, 2026);
        assert_eq!(case.get("org.inn"), None);
        assert_eq!(case.get("document.date"), None);
        assert_eq!(case.get("medical.diagnosis_code"), None);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("не прошло проверку типа")));
    }

    #[test]
    fn unknown_field_id_is_ignored() {
        let json = r#"{"not a field":{"value":"x","confidence":0.9}}"#;
        let (case, _r) = parse_model_extraction(json, 2026);
        assert!(case.values.is_empty());
    }

    #[test]
    fn plain_string_shorthand_supported() {
        let json = r#"{"medical.diagnosis":"Острый бронхит"}"#;
        let (case, _r) = parse_model_extraction(json, 2026);
        assert_eq!(case.get("medical.diagnosis"), Some("Острый бронхит"));
    }

    #[test]
    fn prose_wrapped_json_is_recovered() {
        let out = "Вот результат:\n```json\n{\"org.inn\":{\"value\":\"7736050003\",\"confidence\":0.95}}\n```\nГотово.";
        let (case, _r) = parse_model_extraction(out, 2026);
        assert_eq!(case.get("org.inn"), Some("7736050003"));
    }

    #[test]
    fn model_cannot_override_with_value_not_supported_by_source() {
        let text = "Поставщик: ООО «Ромашка»";
        let hallucinated = r#"{"org.name":{"value":"ООО \"Ромашка-Трейд\"","confidence":0.97,"evidence":"Поставщик: ООО «Ромашка»"}}"#;
        let (case, report) = extract_understanding(text, 2026, Some(hallucinated));
        assert!(!case
            .get("org.name")
            .is_some_and(|value| value.contains("Трейд")));
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("не подтверждает")));
    }

    #[test]
    fn offline_equals_deterministic() {
        let text = "ИНН 7736050003";
        let (a, _) = extract_understanding(text, 2026, None);
        let (b, _) = crate::extract_semantic(text, 2026);
        assert_eq!(a.get("org.inn"), b.get("org.inn"));
    }

    #[test]
    fn bank_account_from_model_is_checked_against_existing_bik() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "org.bank_bik".into(),
            SemanticValue::new("org.bank_bik", "044525225", ValueSource::Scanner, 0.9),
        );
        let warnings = apply_model_output(
            &mut case,
            r#"{"org.bank_account":{"value":"40702810900000002851","confidence":0.99}}"#,
            2026,
        );
        assert!(case.get("org.bank_account").is_none());
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("контрольный ключ")));
    }

    #[test]
    fn model_evidence_must_exist_in_normalized_source() {
        let source = "Пациент:\nИванов   Иван\u{00a0}Иванович";
        let valid = r#"{"subject.name":{"value":"Иванов Иван Иванович","confidence":0.99,"evidence":"Иванов Иван Иванович"}}"#;
        let (case, _) = parse_model_extraction_with_source(valid, Some(source), 2026);
        let value = case
            .values
            .get("subject.name")
            .expect("verified model field");
        assert_eq!(value.source, ValueSource::Model);
        assert_eq!(value.evidence.len(), 1);

        let fabricated = r#"{"subject.name":{"value":"Петров Пётр","confidence":0.99,"evidence":"Петров Пётр"}}"#;
        let (case, report) = parse_model_extraction_with_source(fabricated, Some(source), 2026);
        assert!(case.values.is_empty());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("отсутствует")));
    }

    #[test]
    fn real_excerpt_with_unrelated_value_is_rejected() {
        let source = "Пациент: Иванов Иван Иванович. Дата: 01.02.2026";
        let output = r#"{"subject.name":{"value":"Петров Пётр Петрович","confidence":0.99,"evidence":"Пациент: Иванов Иван Иванович"}}"#;
        let (case, report) = parse_model_extraction_with_source(output, Some(source), 2026);
        assert!(case.get("subject.name").is_none());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("не подтверждает")));
    }

    #[test]
    fn partial_name_tokens_do_not_ground_a_wrong_patronymic() {
        let source = "Пациент: Иванов Иван Иванович";
        let output = r#"{"subject.name":{"value":"Иванов Иван Петрович","confidence":0.99,"evidence":"Пациент: Иванов Иван Иванович"}}"#;
        let (case, report) = parse_model_extraction_with_source(output, Some(source), 2026);
        assert!(case.get("subject.name").is_none());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("не подтверждает")));
    }

    #[test]
    fn address_grounding_rejects_a_different_house_or_flat() {
        let source = "Адрес: г. Москва, ул. Ленина, д. 10, кв. 55";
        let output = r#"{"subject.address":{"value":"г. Москва, ул. Ленина, д. 99, кв. 1","confidence":0.99,"evidence":"Адрес: г. Москва, ул. Ленина, д. 10, кв. 55"}}"#;
        let (case, report) = parse_model_extraction_with_source(output, Some(source), 2026);
        assert!(case.get("subject.address").is_none());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("не подтверждает")));
    }

    #[test]
    fn one_model_pass_cannot_approve_a_high_risk_field() {
        let source = "Дата документа: 01.02.2026";
        let mut case = SemanticCase::default();
        let outputs = vec![r#"{"document.date":{"value":"01.02.2026","confidence":0.99,"evidence":"Дата документа: 01.02.2026"}}"#.to_string()];
        let warnings = apply_model_consensus_with_source(&mut case, &outputs, source, 2026);
        assert!(case.get("document.date").is_none());
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("self-consistency")));
    }

    #[test]
    fn high_risk_consensus_requires_two_equal_grounded_answers() {
        let source = "Дата документа: 01.02.2026";
        let mut case = SemanticCase::default();
        let outputs = vec![
            r#"{"document.date":{"value":"01.02.2026","confidence":0.99,"evidence":"Дата документа: 01.02.2026"}}"#.to_string(),
            r#"{"document.date":{"value":"02.02.2026","confidence":0.99,"evidence":"Дата документа: 01.02.2026"}}"#.to_string(),
        ];
        let warnings = apply_model_consensus_with_source(&mut case, &outputs, source, 2026);
        assert!(case.get("document.date").is_none());
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("self-consistency")));

        let mut agreed = SemanticCase::default();
        let outputs = vec![outputs[0].clone(), outputs[0].clone()];
        apply_model_consensus_with_source(&mut agreed, &outputs, source, 2026);
        assert_eq!(agreed.get("document.date"), Some("01.02.2026"));
    }

    #[test]
    fn live_model_end_to_end_via_trait() {
        let model = MockModel(
            r#"{"subject.name":{"value":"Иванов Иван Иванович","confidence":0.9,"evidence":"Иванов Иван Иванович"}}"#,
        );
        let (case, _r) = extract_with_model(
            "В документе указан Иванов Иван Иванович без стандартной метки",
            2026,
            &model,
        )
        .unwrap();
        assert_eq!(case.get("subject.name"), Some("Иванов Иван Иванович"));
    }

    #[test]
    fn multilingual_prompt_preserves_source_language_and_detects_script() {
        let prompt = build_extraction_prompt_for_domain_and_language(
            "Сотрудник Иванов принят на работу",
            &crate::DomainKind::Hr,
            "auto",
        );
        assert!(prompt.contains("Cyrillic (auto-detected)"));
        assert!(prompt.contains("do not translate"));
        assert!(prompt.contains("Сотрудник Иванов"));

        let german = build_extraction_prompt_for_domain_and_language(
            "Mitarbeiter Max Mustermann",
            &crate::DomainKind::Hr,
            "de-DE",
        );
        assert!(german.contains("de-DE"));
    }
}
