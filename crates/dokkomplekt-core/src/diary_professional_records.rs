//! Donor-compatible dynamic epicrisis enrichment for medical diary collections.
//!
//! The universal repeated-record engine builds the ordinary diary collection first.
//! This façade enriches that collection only for the medical diary workflow and keeps
//! all schedule/content rules in Rust domain code rather than in React or DOCX templates.

use crate::{
    build_dynamic_epicrisis_text, dynamic_epicrisis_base_date, dynamic_epicrisis_dates,
    parse_flexible_date, professional_records, DynamicEpicrisisInput, SemanticAtom, SemanticCase,
    SemanticRecord, DIARY_SICK_LEAVE_EPICRISIS, DIARY_TREATMENT_CORRECTION, MAX_DYNAMIC_EPICRISES,
};
use chrono::{Datelike, NaiveDate};

#[path = "diary_gender.rs"]
mod diary_gender;

const MEDICAL_DIARY_COLLECTIONS: [&str; 2] = ["diaries", "medical_diaries"];
const MEDICAL_DIARY_TEXT_COLLECTIONS: [&str; 2] = ["medical_diary_texts", "diary_texts"];
const NEUTRAL_FINAL_DIARY_TEXT: &str = "Состояние улучшилось. Жалоб активно не предъявляет. Отрицательной динамики не отмечается. Общее самочувствие стабильное, режим соблюдает, назначения выполняет. На текущую дату оформлена выписка из стационара. Даны рекомендации.";

pub fn prepare_professional_collections(template: &str, case: &SemanticCase) -> SemanticCase {
    let mut prepared = professional_records::prepare_professional_collections(template, case);
    if !is_medical_case(case) {
        return prepared;
    }
    for collection_id in MEDICAL_DIARY_COLLECTIONS {
        let Some(rows) = prepared.collections.get_mut(collection_id) else {
            continue;
        };
        mark_regular_rows(rows);
        ensure_donor_final_diary_text(rows, case);
        diary_gender::adapt_diary_rows(rows, case);
        if yes(case.get(DIARY_SICK_LEAVE_EPICRISIS)) {
            merge_dynamic_epicrises(rows, case);
        }
    }
    prepared
}

fn is_medical_case(case: &SemanticCase) -> bool {
    case.active_domains.contains(&crate::DomainKind::Medical)
        || case.has("medical.admission_date")
        || case.has("medical.discharge_date")
        || case.has("medical.diagnosis")
}

fn yes(value: Option<&str>) -> bool {
    matches!(
        value
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .replace('ё', "е")
            .as_str(),
        "да" | "yes" | "true" | "1" | "+" | "нужен" | "нужна"
    )
}

fn mark_regular_rows(rows: &mut [SemanticRecord]) {
    for row in rows {
        row.entry("is_dynamic_epicrisis".into())
            .or_insert(SemanticAtom::Boolean(false));
        row.entry("kind".into())
            .or_insert_with(|| SemanticAtom::Text("diary".into()));
    }
}

/// The working donor removes the discharge date from the ordinary rotating status
/// sequence and appends one dedicated final diary row. Preserve an explicitly
/// specialist-owned final source when one exists; otherwise the final row must use
/// the donor-neutral discharge text even if the generic row builder happened to
/// carry a regular diagnosis-library status into that row.
fn ensure_donor_final_diary_text(rows: &mut [SemanticRecord], case: &SemanticCase) {
    let specialist_final = has_explicit_final_diary_source(case);
    for row in rows {
        if !record_bool(row, "is_final") {
            continue;
        }
        let has_text = row
            .get("text")
            .is_some_and(|value| !value.as_text().trim().is_empty());
        if !specialist_final || !has_text {
            row.insert(
                "text".into(),
                SemanticAtom::Text(NEUTRAL_FINAL_DIARY_TEXT.into()),
            );
        }
    }
}

fn has_explicit_final_diary_source(case: &SemanticCase) -> bool {
    if case
        .blocks
        .get("medical.diary.final_text")
        .is_some_and(|value| !value.trim().is_empty())
        || case
            .get("medical.discharge_condition")
            .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }

    for collection_id in MEDICAL_DIARY_TEXT_COLLECTIONS {
        if case.collection(collection_id).is_some_and(|rows| {
            rows.iter().any(|row| {
                source_row_is_final(row)
                    && row
                        .get("text")
                        .or_else(|| row.get("body"))
                        .is_some_and(|value| !value.as_text().trim().is_empty())
            })
        }) {
            return true;
        }
    }

    case.blocks.iter().any(|(id, value)| {
        id.starts_with("professional.medical.diary.final.") && !value.trim().is_empty()
    })
}

fn source_row_is_final(row: &SemanticRecord) -> bool {
    match row.get("is_final") {
        Some(SemanticAtom::Boolean(value)) => *value,
        Some(value) => matches!(
            value.as_text().trim().to_lowercase().as_str(),
            "1" | "true" | "да" | "final" | "итоговый"
        ),
        None => row.get("kind").is_some_and(|value| {
            matches!(
                value.as_text().trim().to_lowercase().as_str(),
                "final" | "discharge" | "итоговый" | "выписной"
            )
        }),
    }
}

fn merge_dynamic_epicrises(rows: &mut Vec<SemanticRecord>, case: &SemanticCase) {
    rows.retain(|row| !record_bool(row, "is_dynamic_epicrisis"));
    let Some(admission) = semantic_date(case, "medical.admission_date") else {
        return;
    };
    let Some(discharge) = semantic_date(case, "medical.discharge_date") else {
        return;
    };
    if discharge <= admission {
        return;
    }
    let sick_leave_from = semantic_date(case, "medical.sick_leave_from");
    let base = dynamic_epicrisis_base_date(admission, sick_leave_from);
    let data = DynamicEpicrisisInput {
        patient_name: value(case, "subject.name"),
        birth_date: value(case, "subject.birth_date"),
        sick_leave_from: base.format("%d.%m.%Y").to_string(),
        complaints: first_value(case, &["medical.complaints", "medical.current_complaints"]),
        treatment: value(case, "medical.treatment"),
        profile_status: first_value(case, &["medical.profile_status", "medical.status"]),
        treatment_correction: value(case, DIARY_TREATMENT_CORRECTION),
    };
    let text = build_dynamic_epicrisis_text(&data);
    for date in dynamic_epicrisis_dates(base, Some(discharge), MAX_DYNAMIC_EPICRISES) {
        rows.push(dynamic_row(date, admission, &text));
    }
    rows.sort_by(|left, right| {
        row_sort_date(left).cmp(&row_sort_date(right)).then(
            record_bool(left, "is_dynamic_epicrisis")
                .cmp(&record_bool(right, "is_dynamic_epicrisis")),
        )
    });
    for (index, row) in rows.iter_mut().enumerate() {
        row.insert(
            "sequence".into(),
            SemanticAtom::Integer(i64::try_from(index + 1).unwrap_or(i64::MAX)),
        );
    }
}

fn dynamic_row(date: NaiveDate, admission: NaiveDate, text: &str) -> SemanticRecord {
    let date_text = date.format("%d.%m.%Y").to_string();
    let mut row = SemanticRecord::new();
    row.insert(
        "kind".into(),
        SemanticAtom::Text("dynamic_epicrisis".into()),
    );
    row.insert("date".into(), SemanticAtom::Date(date_text.clone()));
    row.insert("datetime".into(), SemanticAtom::Text(date_text));
    row.insert(
        "offset_days".into(),
        SemanticAtom::Integer((date - admission).num_days()),
    );
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
    row.insert("is_final".into(), SemanticAtom::Boolean(false));
    row.insert("is_dynamic_epicrisis".into(), SemanticAtom::Boolean(true));
    row.insert("text".into(), SemanticAtom::Text(text.to_string()));
    // Dynamic epicrisis text already contains the donor signature block. Empty semantic
    // signature slots keep strict custom diary templates renderable without duplicating it.
    row.insert(
        "treating_physician_signature".into(),
        SemanticAtom::Text(String::new()),
    );
    row.insert(
        "department_head_signature".into(),
        SemanticAtom::Text(String::new()),
    );
    row
}

fn semantic_date(case: &SemanticCase, field_id: &str) -> Option<NaiveDate> {
    let raw = case.get(field_id)?.trim();
    let year = explicit_year(raw)
        .or_else(|| case.get("medical.admission_date").and_then(explicit_year))
        .or_else(|| case.get("medical.discharge_date").and_then(explicit_year))
        .unwrap_or_else(|| chrono::Local::now().year());
    let parsed = parse_flexible_date(raw, year)?;
    NaiveDate::parse_from_str(&parsed, "%d.%m.%Y").ok()
}

fn explicit_year(value: &str) -> Option<i32> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .find_map(|part| part.parse::<i32>().ok())
}

fn value(case: &SemanticCase, field_id: &str) -> String {
    case.get(field_id).unwrap_or_default().trim().to_string()
}

fn first_value(case: &SemanticCase, field_ids: &[&str]) -> String {
    field_ids
        .iter()
        .find_map(|field_id| {
            case.get(field_id)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default()
        .to_string()
}

fn record_bool(row: &SemanticRecord, field_id: &str) -> bool {
    match row.get(field_id) {
        Some(SemanticAtom::Boolean(value)) => *value,
        Some(value) => matches!(
            value.as_text().trim().to_lowercase().as_str(),
            "true" | "1" | "да"
        ),
        None => false,
    }
}

fn row_date(row: &SemanticRecord) -> String {
    row.get("date")
        .map(SemanticAtom::as_text)
        .unwrap_or_default()
}

fn row_sort_date(row: &SemanticRecord) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&row_date(row), "%d.%m.%Y").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DomainKind, SemanticValue, ValueSource};

    fn insert(case: &mut SemanticCase, field_id: &str, value: &str) {
        case.values.insert(
            field_id.into(),
            SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
        );
    }

    fn base_case(enabled: &str) -> SemanticCase {
        let mut case = SemanticCase::default();
        case.active_domains.push(DomainKind::Medical);
        insert(&mut case, "medical.admission_date", "10.05.2026");
        insert(&mut case, "medical.discharge_date", "10.06.2026");
        insert(&mut case, "medical.diary_schedule_style", "Каждый день");
        insert(
            &mut case,
            "medical.diary_intraday_rhythm",
            "Один раз в день",
        );
        insert(&mut case, DIARY_SICK_LEAVE_EPICRISIS, enabled);
        insert(&mut case, "subject.name", "Иванов Иван Иванович");
        insert(
            &mut case,
            "medical.treatment",
            "Терапия по листу назначений",
        );
        case.set_collection(
            "medical_diary_texts",
            vec![SemanticRecord::from([(
                "text".into(),
                SemanticAtom::Text(
                    "Состояние стабильное, жалоб не предъявляет, лечение переносит удовлетворительно."
                        .into(),
                ),
            )])],
        );
        case
    }

    #[test]
    fn donor_dynamic_epicrisis_is_inserted_every_ten_days_only_when_enabled() {
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &base_case("Да"),
        );
        let rows = prepared.collection("diaries").unwrap();
        let dynamic = rows
            .iter()
            .filter(|row| record_bool(row, "is_dynamic_epicrisis"))
            .collect::<Vec<_>>();
        assert_eq!(dynamic.len(), 3);
        assert_eq!(row_date(dynamic[0]), "20.05.2026");
        // 30.05.2026 is Saturday, therefore the donor shifts it to Monday 01.06.2026.
        assert_eq!(row_date(dynamic[1]), "01.06.2026");
        // The donor cadence remains anchored to +10 days, so +30 is 09.06 before discharge.
        assert_eq!(row_date(dynamic[2]), "09.06.2026");
        let ordered_dates = rows.iter().filter_map(row_sort_date).collect::<Vec<_>>();
        let mut sorted_dates = ordered_dates.clone();
        sorted_dates.sort();
        assert_eq!(ordered_dates, sorted_dates);
        let same_day = rows
            .iter()
            .filter(|row| row_date(row) == "20.05.2026")
            .collect::<Vec<_>>();
        assert_eq!(same_day.len(), 2);
        assert!(!record_bool(same_day[0], "is_dynamic_epicrisis"));
        assert!(record_bool(same_day[1], "is_dynamic_epicrisis"));
        assert!(dynamic[0]
            .get("text")
            .unwrap()
            .as_text()
            .starts_with("Динамический эпикриз."));
    }

    #[test]
    fn donor_dynamic_epicrisis_is_absent_for_no_and_never_reaches_discharge_day() {
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &base_case("Нет"),
        );
        assert!(prepared
            .collection("diaries")
            .unwrap()
            .iter()
            .all(|row| !record_bool(row, "is_dynamic_epicrisis")));

        let mut short = base_case("Да");
        insert(&mut short, "medical.discharge_date", "20.05.2026");
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &short,
        );
        assert!(prepared
            .collection("diaries")
            .unwrap()
            .iter()
            .all(|row| !record_bool(row, "is_dynamic_epicrisis")));
    }

    #[test]
    fn final_diary_uses_neutral_donor_text_instead_of_rotating_regular_status() {
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &base_case("Нет"),
        );
        let final_row = prepared
            .collection("diaries")
            .unwrap()
            .iter()
            .find(|row| record_bool(row, "is_final"))
            .expect("final discharge diary must exist");
        let text = final_row.get("text").unwrap().as_text();
        assert_eq!(text, NEUTRAL_FINAL_DIARY_TEXT);
        assert!(!text.to_lowercase().contains("психот"));
        assert!(!text.contains("лечение переносит удовлетворительно"));
    }

    #[test]
    fn specialist_owned_final_diary_text_overrides_neutral_fallback() {
        let mut case = base_case("Нет");
        case.blocks.insert(
            "medical.diary.final_text".into(),
            "Финальная запись, подтверждённая специалистом.".into(),
        );
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &case,
        );
        let final_row = prepared
            .collection("diaries")
            .unwrap()
            .iter()
            .find(|row| record_bool(row, "is_final"))
            .expect("final discharge diary must exist");
        assert_eq!(
            final_row.get("text").unwrap().as_text(),
            "Финальная запись, подтверждённая специалистом."
        );
    }
}
