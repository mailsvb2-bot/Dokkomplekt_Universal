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
            row.insert(
                "treating_physician_signature".into(),
                SemanticAtom::Text("Лечащий врач __________________ /____________/".into()),
            );
            row.insert(
                "department_head_signature".into(),
                SemanticAtom::Text("Заведующий отделением __________ /____________/".into()),
            );

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
    if all.is_empty() {
        return DiaryTextSources::default();
    }

    let target = normalize_match(diagnosis);
    let matching = all
        .iter()
        .copied()
        .filter(|row| {
            atom_text(row, "diagnosis")
                .map(|value| normalize_match(&value) == target)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let selected = if !matching.is_empty() {
        matching
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
    result
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
    fn nonmedical_case_does_not_receive_medical_diaries() {
        let case = SemanticCase::default();
        let prepared =
            prepare_professional_collections("{{#each diaries}}{{diary.date}}{{/each}}", &case);
        assert!(prepared.collection("diaries").is_none());
    }
}
