//! Donor-compatible dynamic epicrisis scheduling and text for the medical diary profile.
//!
//! This module is intentionally pure and UI-free. The universal renderer remains profession-
//! neutral; the medical profile decides whether these rows exist and supplies confirmed data.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

pub const DEFAULT_TREATMENT_CORRECTION: &str = "Лекарства принимает согласно назначениям.";
pub const MAX_DYNAMIC_EPICRISES: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DynamicEpicrisisInput {
    pub patient_name: String,
    pub birth_date: String,
    pub sick_leave_from: String,
    pub complaints: String,
    pub treatment: String,
    pub profile_status: String,
    pub treatment_correction: String,
}

/// The donor starts the ten-day counter no earlier than admission and no earlier than the
/// explicitly supplied sick-leave start date.
pub fn dynamic_epicrisis_base_date(
    admission: NaiveDate,
    sick_leave_from: Option<NaiveDate>,
) -> NaiveDate {
    sick_leave_from.map_or(admission, |date| admission.max(date))
}

/// The working donor treats weekends plus 1-9 January and 1-9 May as non-working days.
pub fn donor_non_working_day(day: NaiveDate) -> bool {
    matches!(day.weekday(), Weekday::Sat | Weekday::Sun)
        || ((day.month() == 1 || day.month() == 5) && (1..=9).contains(&day.day()))
}

pub fn next_donor_working_day(day: NaiveDate, used: &[NaiveDate]) -> Option<NaiveDate> {
    let mut current = day;
    for _ in 0..370 {
        if !donor_non_working_day(current) && !used.contains(&current) {
            return Some(current);
        }
        current = current.checked_add_signed(Duration::days(1))?;
    }
    None
}

/// Dynamic epicrises are planned every ten treatment days, moved forward to the donor working
/// calendar, limited to twelve entries and never emitted on/after discharge.
pub fn dynamic_epicrisis_dates(
    base: NaiveDate,
    discharge: Option<NaiveDate>,
    limit: usize,
) -> Vec<NaiveDate> {
    let limit = limit.min(MAX_DYNAMIC_EPICRISES);
    let mut result = Vec::new();
    let Some(mut current) = base.checked_add_signed(Duration::days(10)) else {
        return result;
    };
    while result.len() < limit {
        if discharge.is_some_and(|date| current >= date) {
            break;
        }
        let Some(adjusted) = next_donor_working_day(current, &result) else {
            break;
        };
        if discharge.is_some_and(|date| adjusted >= date) {
            break;
        }
        result.push(adjusted);
        let Some(next) = current.checked_add_signed(Duration::days(10)) else {
            break;
        };
        current = next;
    }
    result
}

pub fn build_dynamic_epicrisis_text(data: &DynamicEpicrisisInput) -> String {
    let correction = if data.treatment_correction.trim().is_empty() {
        DEFAULT_TREATMENT_CORRECTION
    } else {
        data.treatment_correction.trim()
    };
    [
        "Динамический эпикриз.".to_string(),
        format!("ФИО: {}.", non_empty_or(&data.patient_name, "не указано")),
        format!(
            "Дата рождения: {}.",
            non_empty_or(&data.birth_date, "не указана")
        ),
        format!(
            "Лечится с: {}.",
            non_empty_or(&data.sick_leave_from, "не указано")
        ),
        format!(
            "Жалобы: {}.",
            non_empty_or(&data.complaints, "без существенной динамики")
        ),
        format!(
            "Принимает: {}.",
            non_empty_or(&data.treatment, "согласно листу назначений")
        ),
        format!(
            "Профильный статус: {}.",
            non_empty_or(&data.profile_status, "без существенной динамики")
        ),
        correction.to_string(),
        "Продолжение лечения по листу нетрудоспособности.".to_string(),
        "Заведующий отделением ____________________".to_string(),
        "Лечащий врач ____________________".to_string(),
    ]
    .join("\n")
}

fn non_empty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%d.%m.%Y").unwrap()
    }

    #[test]
    fn base_date_never_precedes_admission_or_sick_leave_start() {
        assert_eq!(
            dynamic_epicrisis_base_date(d("10.05.2026"), None),
            d("10.05.2026")
        );
        assert_eq!(
            dynamic_epicrisis_base_date(d("10.05.2026"), Some(d("15.05.2026"))),
            d("15.05.2026")
        );
        assert_eq!(
            dynamic_epicrisis_base_date(d("10.05.2026"), Some(d("01.05.2026"))),
            d("10.05.2026")
        );
    }

    #[test]
    fn every_ten_days_moves_forward_to_donor_working_day_and_stays_before_discharge() {
        // 20.05.2026 is Wednesday; 30.05.2026 is Saturday and shifts to Monday 01.06.
        let dates = dynamic_epicrisis_dates(d("10.05.2026"), Some(d("10.06.2026")), 12);
        assert_eq!(dates, vec![d("20.05.2026"), d("01.06.2026")]);
    }

    #[test]
    fn fixed_january_and_may_holidays_are_shifted_forward() {
        assert_eq!(
            next_donor_working_day(d("01.05.2026"), &[]),
            Some(d("11.05.2026"))
        );
        assert_eq!(
            next_donor_working_day(d("01.01.2027"), &[]),
            Some(d("11.01.2027"))
        );
    }

    #[test]
    fn discharge_day_is_never_used_for_dynamic_epicrisis() {
        assert!(dynamic_epicrisis_dates(d("10.05.2026"), Some(d("20.05.2026")), 12).is_empty());
    }

    #[test]
    fn donor_text_and_fallbacks_are_preserved() {
        let text = build_dynamic_epicrisis_text(&DynamicEpicrisisInput {
            patient_name: "Иванов Иван Иванович".into(),
            sick_leave_from: "10.05.2026".into(),
            ..DynamicEpicrisisInput::default()
        });
        assert!(text.starts_with("Динамический эпикриз.\nФИО: Иванов Иван Иванович."));
        assert!(text.contains("Дата рождения: не указана."));
        assert!(text.contains("Жалобы: без существенной динамики."));
        assert!(text.contains("Принимает: согласно листу назначений."));
        assert!(text.contains(DEFAULT_TREATMENT_CORRECTION));
        assert!(text.ends_with(
            "Заведующий отделением ____________________\nЛечащий врач ____________________"
        ));
    }
}
