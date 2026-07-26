//! Offline reference-data primitives. Large official datasets can be supplied as signed resources.
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::OnceLock,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarError {
    UnsupportedYear(i32),
    InvalidData(String),
    DateOverflow,
}

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedYear(year) => write!(
                f,
                "Производственный календарь РФ для {year} года не подтверждён. Обновите справочник; расчёт рабочих дней остановлен."
            ),
            Self::InvalidData(message) => write!(f, "Некорректный производственный календарь: {message}"),
            Self::DateOverflow => write!(f, "Переполнение даты при расчёте рабочих дней"),
        }
    }
}

impl std::error::Error for CalendarError {}

#[derive(Debug, Clone, Default)]
pub struct ProductionCalendar {
    holidays: BTreeSet<NaiveDate>,
    working_weekends: BTreeSet<NaiveDate>,
    complete_years: BTreeSet<i32>,
    listed_years: BTreeSet<i32>,
}

impl ProductionCalendar {
    /// Returns whether a date is a working day, but only for a year explicitly
    /// marked as complete in the bundled reference data. This is intentionally
    /// fail-closed: a weekend-only fallback can silently corrupt legal and HR
    /// deadlines when government transfers are not yet known.
    pub fn is_working_day(&self, d: NaiveDate) -> Result<bool, CalendarError> {
        self.ensure_complete_year(d.year())?;
        Ok(if self.working_weekends.contains(&d) {
            true
        } else if self.holidays.contains(&d) {
            false
        } else {
            !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
        })
    }

    pub fn ensure_complete_year(&self, year: i32) -> Result<(), CalendarError> {
        if self.complete_years.contains(&year) {
            Ok(())
        } else {
            Err(CalendarError::UnsupportedYear(year))
        }
    }

    pub fn is_year_complete(&self, year: i32) -> bool {
        self.complete_years.contains(&year)
    }

    pub fn listed_years(&self) -> impl Iterator<Item = i32> + '_ {
        self.listed_years.iter().copied()
    }
}

static CALENDAR: OnceLock<Result<ProductionCalendar, String>> = OnceLock::new();
static CALENDAR_OVERRIDE: OnceLock<ProductionCalendar> = OnceLock::new();

pub fn install_production_calendar_override(
    calendar: ProductionCalendar,
) -> Result<(), CalendarError> {
    CALENDAR_OVERRIDE.set(calendar).map_err(|_| {
        CalendarError::InvalidData("обновлённый календарь уже установлен в этом процессе".into())
    })
}

pub fn production_calendar_ru() -> Result<&'static ProductionCalendar, CalendarError> {
    if let Some(calendar) = CALENDAR_OVERRIDE.get() {
        return Ok(calendar);
    }
    CALENDAR
        .get_or_init(|| {
            parse_production_calendar(include_str!(
                "../../../resources/production_calendar_ru.tsv"
            ))
        })
        .as_ref()
        .map_err(|message| CalendarError::InvalidData(message.clone()))
}

pub fn parse_production_calendar(input: &str) -> Result<ProductionCalendar, String> {
    let mut calendar = ProductionCalendar::default();
    for (index, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = line.split('\t').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
            return Err(format!(
                "Строка календаря {} должна содержать ровно два непустых TSV-поля",
                index + 1
            ));
        }
        let first = parts[0];
        let kind = parts[1];

        if let Ok(year) = first.parse::<i32>() {
            if !matches!(kind, "complete" | "provisional") {
                return Err(format!(
                    "Неизвестный статус календарного года в строке {}: {kind}",
                    index + 1
                ));
            }
            if !calendar.listed_years.insert(year) {
                return Err(format!("Год {year} объявлен в календаре более одного раза"));
            }
            if kind == "complete" {
                calendar.complete_years.insert(year);
            }
            continue;
        }

        let date = NaiveDate::parse_from_str(first, "%Y-%m-%d").map_err(|_| {
            format!(
                "Некорректная дата производственного календаря в строке {}: {first}",
                index + 1
            )
        })?;
        match kind {
            "holiday" => {
                if calendar.working_weekends.contains(&date) || !calendar.holidays.insert(date) {
                    return Err(format!(
                        "Дата {date} продублирована или одновременно объявлена рабочей и выходной"
                    ));
                }
            }
            "working" => {
                if calendar.holidays.contains(&date) || !calendar.working_weekends.insert(date) {
                    return Err(format!(
                        "Дата {date} продублирована или одновременно объявлена рабочей и выходной"
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "Неизвестный тип календарной даты в строке {}: {kind}",
                    index + 1
                ));
            }
        }
    }

    for date in calendar.holidays.iter().chain(&calendar.working_weekends) {
        if !calendar.listed_years.contains(&date.year()) {
            return Err(format!(
                "Дата {date} относится к году без строки YYYY<TAB>complete|provisional"
            ));
        }
    }
    if calendar.complete_years.is_empty() {
        return Err("В производственном календаре нет ни одного подтверждённого года".into());
    }
    Ok(calendar)
}

pub fn add_working_days_with_calendar(
    calendar: &ProductionCalendar,
    start: NaiveDate,
    amount: i32,
) -> Result<NaiveDate, CalendarError> {
    calendar.ensure_complete_year(start.year())?;
    let step = if amount >= 0 { 1_i64 } else { -1_i64 };
    let mut remain = amount.unsigned_abs();
    let mut date = start;
    while remain > 0 {
        date = date
            .checked_add_signed(Duration::days(step))
            .ok_or(CalendarError::DateOverflow)?;
        if calendar.is_working_day(date)? {
            remain -= 1;
        }
    }
    Ok(date)
}

pub fn add_working_days_ru(start: NaiveDate, amount: i32) -> Result<NaiveDate, CalendarError> {
    add_working_days_with_calendar(production_calendar_ru()?, start, amount)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceRecord {
    pub code: String,
    pub title: String,
    pub extra: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_weekend() {
        let fri = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        assert_eq!(
            add_working_days_ru(fri, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 5, 18).unwrap()
        );
    }

    #[test]
    fn complete_2025_and_2026_calendars_are_loaded() {
        let calendar = production_calendar_ru().expect("bundled calendar");
        assert!(calendar.is_year_complete(2025));
        assert!(calendar.is_year_complete(2026));
        assert!(!calendar.is_year_complete(2027));
        assert_eq!(
            calendar.listed_years().collect::<Vec<_>>(),
            vec![2025, 2026, 2027]
        );

        for date in [
            (2025, 1, 1),
            (2025, 5, 2),
            (2025, 5, 8),
            (2025, 6, 13),
            (2025, 11, 3),
            (2025, 12, 31),
            (2026, 1, 1),
            (2026, 1, 8),
            (2026, 3, 9),
            (2026, 5, 11),
        ] {
            assert_eq!(
                calendar.is_working_day(NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap()),
                Ok(false)
            );
        }
        assert_eq!(
            calendar.is_working_day(NaiveDate::from_ymd_opt(2025, 11, 1).unwrap()),
            Ok(true)
        );
        assert_eq!(
            calendar.is_working_day(NaiveDate::from_ymd_opt(2026, 1, 9).unwrap()),
            Ok(true)
        );
    }

    #[test]
    fn working_day_arithmetic_skips_new_year_and_substitute_holidays() {
        assert_eq!(
            add_working_days_ru(NaiveDate::from_ymd_opt(2025, 4, 30).unwrap(), 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 5, 5).unwrap()
        );
        assert_eq!(
            add_working_days_ru(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(), 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 9).unwrap()
        );
        assert_eq!(
            add_working_days_ru(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap(), 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 5, 12).unwrap()
        );
    }

    #[test]
    fn unsupported_year_fails_closed_instead_of_using_weekends_only() {
        let error = add_working_days_ru(NaiveDate::from_ymd_opt(2027, 12, 30).unwrap(), 3)
            .expect_err(
                "2027 must remain blocked until the official transfer calendar is complete",
            );
        assert_eq!(error, CalendarError::UnsupportedYear(2027));
    }

    #[test]
    fn crossing_into_an_unconfirmed_year_fails_closed() {
        let error = add_working_days_ru(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), 1)
            .expect_err("cross-year calculation must validate the destination year");
        assert_eq!(error, CalendarError::UnsupportedYear(2027));
    }

    #[test]
    fn malformed_calendar_resource_is_rejected_instead_of_silently_skipped() {
        assert!(parse_production_calendar("2026\tcomplete\n2026-01-01\tholiday\n").is_ok());
        assert!(parse_production_calendar("2026\tcomplete\nnot-a-date\tholiday\n").is_err());
        assert!(parse_production_calendar("2026\tcomplete\n2026-01-01\tunknown\n").is_err());
        assert!(parse_production_calendar(
            "2026\tcomplete\n2026-01-01\tholiday\n2026-01-01\tworking\n"
        )
        .is_err());
        assert!(parse_production_calendar("2026-01-01\tholiday\n").is_err());
    }

    #[test]
    fn cached() {
        assert!(std::ptr::eq(
            production_calendar_ru().expect("calendar"),
            production_calendar_ru().expect("calendar")
        ));
    }
}
