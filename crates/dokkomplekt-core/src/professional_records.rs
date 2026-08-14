//! Profession-aware preparation of repeated records before template rendering.
//!
//! The collection mechanism itself is universal. Domain adapters may derive a
//! collection from already-confirmed semantic data, but they must never invent
//! professional content. This keeps medicine out of the universal renderer
//! while still allowing a medical profile to provide the proven diary rules.

use crate::{
    build_medical_diary_series, template_collection_references, DomainKind,
    MedicalDiarySeriesRequest, SemanticAtom, SemanticCase, SemanticRecord,
};
use chrono::{Datelike, Local, NaiveDate};

const MEDICAL_DIARY_COLLECTIONS: [&str; 2] = ["diaries", "medical_diaries"];
const MEDICAL_DIARY_TEXT_COLLECTIONS: [&str; 2] = ["medical_diary_texts", "diary_texts"];

/// Clone `case` and derive only the professional collections that the template
/// actually references. Explicitly supplied collections always win.
///
/// This is deliberately called by the common text/DOCX rendering seam, so the
/// same behaviour is used by manual generation, batch generation and zero-touch
/// automation instead of being reimplemented by each caller.
pub fn prepare_professional_collections(template: &str, case: &SemanticCase) -> SemanticCase {
    let referenced = template_collection_references(template);
    if referenced.is_empty() {
        return case.clone();
    }

    let mut prepared = case.clone();
    if is_medical_case(case) {
        for collection_id in MEDICAL_DIARY_COLLECTIONS {
            if referenced.iter().any(|id| id == collection_id)
                && prepared.collection(collection_id).is_none()
            {
                if let Some(rows) = build_medical_diary_rows(case) {
                    prepared.set_collection(collection_id, rows);
                }
            }
        }
    }
    prepared
}

fn is_medical_case(case: &SemanticCase) -> bool {
    case.active_domains.contains(&DomainKind::Medical)
        || case.has("medical.admission_date")
        || case.has("medical.discharge_date")
        || case.has("medical.diagnosis")
}

fn build_medical_diary_rows(case: &SemanticCase) -> Option<Vec<SemanticRecord>> {
    let admission = case.get("medical.admission_date")?.trim();
    let discharge = case.get("medical.discharge_date")?.trim();
    if admission.is_empty() || discharge.is_empty() {
        return None;
    }

    let default_year = explicit_year(admission)
        .or_else(|| explicit_year(discharge))
        .unwrap_or_else(|| Local::now().year());
    let entries = build_medical_diary_series(&MedicalDiarySeriesRequest {
        admission_date: admission.to_string(),
        discharge_date: discharge.to_string(),
        default_year,
        confirmed_cadence: None,
        profile_cadence: None,
        day_start_time: None,
        day_end_time: None,
        skip_weekdays: Vec::new(),
        excluded_dates: Vec::new(),
        force_final_discharge_entry: true,
    })
    .ok()?;

    let diagnosis = case.get("medical.diagnosis").unwrap_or_default();
    let sources = diary_text_sources(case, diagnosis);
    let final_from_block = case
        .blocks
        .get("medical.diary.final_text")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let final_from_condition = case
        .get("medical.discharge_condition")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("Состояние при выписке: {value}"));
    let final_text = sources
        .final_text
        .clone()
        .or(final_from_block)
        .or(final_from_condition);

    let mut regular_index = 0usize;
    let rows = entries
        .into_iter()
        .map(|entry| {
            let mut row = SemanticRecord::new();
            row.insert(
                "sequence".into(),
                SemanticAtom::Integer(i64::from(entry.sequence)),
            );
            row.insert("date".into(), SemanticAtom::Date(entry.date.clone()));
            row.insert(
                "offset_days".into(),
                SemanticAtom::Integer(i64::from(entry.offset_days)),
            );
            if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%d.%m.%Y") {
                row.insert("day".into(), SemanticAtom::Integer(i64::from(date.day())));
                row.insert(
                    "day_number".into(),
                    SemanticAtom::Text(format!("{:02}", date.day())),
                );
                row.insert(
                    "month".into(),
                    SemanticAtom::Integer(i64::from(date.month())),
                );
                row.insert("year".into(), SemanticAtom::Integer(i64::from(date.year())));
            }
            if let Some(time) = entry
                .time
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                row.insert("time".into(), SemanticAtom::Text(time.to_string()));
            }
            row.insert(
                "datetime".into(),
                SemanticAtom::Text(entry.datetime.clone()),
            );
            row.insert(
                "is_final".into(),
                SemanticAtom::Boolean(entry.is_final_discharge_entry),
            );
            if let Some(signature) = entry.signatures.first() {
                row.insert(
                    "treating_physician_signature".into(),
                    SemanticAtom::Text(signature.clone()),
                );
            }
            if let Some(signature) = entry.signatures.get(1) {
                row.insert(
                    "department_head_signature".into(),
                    SemanticAtom::Text(signature.clone()),
                );
            }

            let body = if entry.is_final_discharge_entry {
                final_text.clone()
            } else if sources.regular.is_empty() {
                None
            } else {
                let value = sources.regular[regular_index % sources.regular.len()].clone();
                regular_index += 1;
                Some(value)
            };
            // Deliberately omit `text` when there is no specialist-owned source.
            // A strict template using {{diary.text}} then fails closed instead of
            // silently publishing an empty medical diary.
            if let Some(body) = body.filter(|value| !value.trim().is_empty()) {
                row.insert("text".into(), SemanticAtom::Text(body));
            }
            row
        })
        .collect::<Vec<_>>();
    (!rows.is_empty()).then_some(rows)
}

#[derive(Default)]
struct DiaryTextSources {
    regular: Vec<String>,
    final_text: Option<String>,
}

fn diary_text_sources(case: &SemanticCase, diagnosis: &str) -> DiaryTextSources {
    let mut all = Vec::<&SemanticRecord>::new();
    for collection_id in MEDICAL_DIARY_TEXT_COLLECTIONS {
        if let Some(rows) = case.collection(collection_id) {
            all.extend(rows);
        }
    }

    let target = normalize_match(diagnosis);
    let exact = all
        .iter()
        .copied()
        .filter(|row| {
            atom_text(row, "diagnosis")
                .map(|value| normalize_match(&value) == target)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let compatible = if exact.is_empty() {
        unambiguous_compatible_rows(&all, &target)
    } else {
        Vec::new()
    };
    let selected = if !exact.is_empty() {
        exact
    } else if !compatible.is_empty() {
        compatible
    } else {
        // Unscoped rows are reusable within the active medical profile. Rows
        // explicitly assigned to a different diagnosis must never leak across.
        all.into_iter()
            .filter(|row| atom_text(row, "diagnosis").is_none_or(|value| value.trim().is_empty()))
            .collect::<Vec<_>>()
    };

    let mut result = DiaryTextSources::default();
    for row in selected {
        let Some(text) = atom_text(row, "text")
            .or_else(|| atom_text(row, "body"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if record_is_final(row) {
            if result.final_text.is_none() {
                result.final_text = Some(text);
            }
        } else {
            result.regular.push(text);
        }
    }

    // Persistent profile sources reuse the existing local clause-block store.
    // This keeps storage universal: other professions may introduce their own
    // namespaced sources without a medical database or a second semantic brain.
    let key = source_key(diagnosis);
    if result.regular.is_empty() {
        if let Some(content) = persistent_source(case, "professional.medical.diary.regular.", &key)
        {
            result.regular = split_status_source(content);
        }
    }
    if result.final_text.is_none() {
        result.final_text = persistent_source(case, "professional.medical.diary.final.", &key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    result
}

fn unambiguous_compatible_rows<'a>(
    rows: &[&'a SemanticRecord],
    target: &str,
) -> Vec<&'a SemanticRecord> {
    let mut candidates = rows
        .iter()
        .copied()
        .filter_map(|row| {
            let diagnosis = atom_text(row, "diagnosis")?;
            let normalized = normalize_match(&diagnosis);
            diagnosis_compatible(&normalized, target).then_some((normalized, row))
        })
        .collect::<Vec<_>>();
    let mut keys = candidates
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() != 1 {
        return Vec::new();
    }
    candidates.drain(..).map(|(_, row)| row).collect()
}

fn diagnosis_compatible(candidate: &str, target: &str) -> bool {
    let candidate = source_key(candidate);
    let target = source_key(target);
    candidate.len() >= 3
        && target.len() >= 3
        && (candidate.contains(&target) || target.contains(&candidate))
}

fn persistent_source<'a>(case: &'a SemanticCase, prefix: &str, key: &str) -> Option<&'a str> {
    let exact = format!("{prefix}{key}");
    if let Some(value) = case.blocks.get(&exact) {
        return Some(value.as_str());
    }
    let mut candidates = case
        .blocks
        .iter()
        .filter_map(|(id, value)| {
            let suffix = id.strip_prefix(prefix)?;
            diagnosis_compatible(suffix, key).then_some((suffix, value.as_str()))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(right.0));
    candidates.dedup_by(|left, right| left.0 == right.0);
    (candidates.len() == 1).then(|| candidates[0].1)
}

fn source_key(value: &str) -> String {
    normalize_match(value)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn split_status_source(content: &str) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if paragraphs.len() > 1 {
        return paragraphs;
    }
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|value| value.chars().count() >= 25)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 1 {
        lines
    } else {
        normalized
            .trim()
            .is_empty()
            .then(Vec::new)
            .unwrap_or_else(|| vec![normalized.trim().to_string()])
    }
}

fn record_is_final(row: &SemanticRecord) -> bool {
    match row.get("is_final") {
        Some(SemanticAtom::Boolean(value)) => *value,
        Some(value) => matches!(
            value.as_text().trim().to_lowercase().as_str(),
            "1" | "true" | "да" | "final" | "итоговый"
        ),
        None => atom_text(row, "kind").is_some_and(|value| {
            matches!(
                value.trim().to_lowercase().as_str(),
                "final" | "discharge" | "итоговый" | "выписной"
            )
        }),
    }
}

fn atom_text(row: &SemanticRecord, key: &str) -> Option<String> {
    row.get(key).map(SemanticAtom::as_text)
}

fn normalize_match(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ё', "е")
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

fn explicit_year(value: &str) -> Option<i32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .filter_map(|part| part.parse::<i32>().ok())
        .find(|year| (1900..=2200).contains(year))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_text_template, SemanticValue, ValueSource};

    fn medical_case() -> SemanticCase {
        let mut case = SemanticCase::default();
        case.active_domains.push(DomainKind::Medical);
        for (field, value) in [
            ("medical.admission_date", "10.05.2026"),
            ("medical.discharge_date", "13.05.2026"),
            ("medical.diagnosis", "F20.0"),
            ("medical.discharge_condition", "улучшение"),
        ] {
            case.values.insert(
                field.into(),
                SemanticValue::new(field, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        case
    }

    fn text_row(text: &str, diagnosis: Option<&str>, final_row: bool) -> SemanticRecord {
        let mut row = SemanticRecord::new();
        row.insert("text".into(), SemanticAtom::Text(text.into()));
        if let Some(diagnosis) = diagnosis {
            row.insert("diagnosis".into(), SemanticAtom::Text(diagnosis.into()));
        }
        if final_row {
            row.insert("is_final".into(), SemanticAtom::Boolean(true));
        }
        row
    }

    #[test]
    fn common_renderer_derives_complete_medical_diary_collection() {
        let mut case = medical_case();
        case.set_collection(
            "medical_diary_texts",
            vec![
                text_row("Дневник A", Some("F20.0"), false),
                text_row("Дневник B", Some("F20.0"), false),
                text_row("Выписной дневник", Some("F20.0"), true),
                text_row("Чужой диагноз", Some("F32.0"), false),
            ],
        );
        let template = "{{#each diaries}}{{diary.date}}|{{diary.text}}|{{diary.treating_physician_signature}}|{{diary.department_head_signature}}\n{{/each}}";
        let rendered = render_text_template(template, &case, true);
        assert!(
            rendered.missing_fields.is_empty(),
            "{:?}",
            rendered.missing_fields
        );
        assert!(
            rendered.unknown_fields.is_empty(),
            "{:?}",
            rendered.unknown_fields
        );
        assert!(rendered.output_text.contains("11.05.2026|Дневник A"));
        assert!(rendered.output_text.contains("12.05.2026|Дневник B"));
        assert!(rendered.output_text.contains("13.05.2026|Выписной дневник"));
        assert!(!rendered.output_text.contains("Чужой диагноз"));
        assert_eq!(rendered.output_text.matches("Лечащий врач").count(), 3);
        assert_eq!(
            rendered
                .output_text
                .matches("Заведующий отделением")
                .count(),
            3
        );
    }

    #[test]
    fn missing_specialist_diary_text_fails_closed_in_strict_template() {
        let case = medical_case();
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &case,
            true,
        );
        assert!(
            !rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty(),
            "strict diary unexpectedly rendered without specialist text: {rendered:?}"
        );
    }

    #[test]
    fn explicit_user_diary_collection_is_never_replaced() {
        let mut case = medical_case();
        let mut row = SemanticRecord::new();
        row.insert("text".into(), SemanticAtom::Text("Ручной дневник".into()));
        case.set_collection("diaries", vec![row]);
        let prepared =
            prepare_professional_collections("{{#each diaries}}{{diary.text}}{{/each}}", &case);
        assert_eq!(
            prepared.collection("diaries").unwrap()[0]["text"].as_text(),
            "Ручной дневник"
        );
    }

    #[test]
    fn unambiguous_parent_diagnosis_source_matches_more_specific_code() {
        let mut case = medical_case();
        case.blocks.insert(
            "professional.medical.diary.regular.f20".into(),
            "Профессиональный статус для родительского кода диагноза.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.final.f20".into(),
            "Итоговый статус родительского кода.".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}\n{{/each}}", &case, true);
        assert!(rendered.output_text.contains("родительского кода"));
        assert!(rendered.missing_fields.is_empty());
    }

    #[test]
    fn ambiguous_partial_diagnosis_sources_are_not_guessed() {
        let mut case = medical_case();
        case.values.get_mut("medical.diagnosis").unwrap().value = "F20".into();
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            "Статус F20.0".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.regular.f201".into(),
            "Статус F20.1".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}{{/each}}", &case, true);
        assert!(!rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty());
    }

    #[test]
    fn persistent_clause_block_sources_feed_medical_diaries() {
        let mut case = medical_case();
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            "Первый профессиональный статус достаточно длинный для источника.\n\nВторой профессиональный статус также хранится локально.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.final.f200".into(),
            "Подтверждённый специалистом итоговый дневник.".into(),
        );
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}}|{{diary.text}}\n{{/each}}",
            &case,
            true,
        );
        assert!(
            rendered.missing_fields.is_empty(),
            "{:?}",
            rendered.missing_fields
        );
        assert!(rendered
            .output_text
            .contains("Первый профессиональный статус"));
        assert!(rendered
            .output_text
            .contains("Второй профессиональный статус"));
        assert!(rendered
            .output_text
            .contains("Подтверждённый специалистом итоговый дневник"));
    }

    #[test]
    fn nonmedical_case_does_not_receive_medical_diaries() {
        let case = SemanticCase::default();
        let prepared =
            prepare_professional_collections("{{#each diaries}}{{diary.date}}{{/each}}", &case);
        assert!(prepared.collection("diaries").is_none());
    }
}
