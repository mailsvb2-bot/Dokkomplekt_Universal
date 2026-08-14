//! Domain-neutral engine for repeated records.
//!
//! A series can be a shift report, inspection log, lesson record, legal action log,
//! medical observation, laboratory protocol or any other repeated document section.

use crate::parse_flexible_date;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const MAX_SERIES_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SeriesCadence {
    Daily,
    DayOffsets(Vec<i32>),
    FixedTimes(Vec<String>),
    MinuteInterval(u32),
    DayOffsetsFixedTimes {
        day_offsets: Vec<i32>,
        times: Vec<String>,
    },
    DayOffsetsMinuteInterval {
        day_offsets: Vec<i32>,
        minutes: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesPlanRequest {
    pub start_date: String,
    pub end_date: String,
    pub default_year: i32,
    #[serde(default)]
    pub start_offset_days: i32,
    pub cadence: SeriesCadence,
    #[serde(default)]
    pub day_start_time: Option<String>,
    #[serde(default)]
    pub day_end_time: Option<String>,
    /// ISO weekday numbers to omit (1 = Monday, 7 = Sunday).
    #[serde(default)]
    pub skip_weekdays: Vec<u32>,
    /// Explicit dates to omit, accepted in the same flexible formats as start/end.
    #[serde(default)]
    pub excluded_dates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesEntryPlan {
    pub sequence: u32,
    pub offset_days: i32,
    pub date: String,
    pub time: Option<String>,
    pub datetime: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum SeriesPlanError {
    #[error("не задана дата начала")]
    MissingStartDate,
    #[error("не задана дата окончания")]
    MissingEndDate,
    #[error("некорректная дата начала: {0}")]
    InvalidStartDate(String),
    #[error("некорректная дата окончания: {0}")]
    InvalidEndDate(String),
    #[error("дата окончания раньше даты начала")]
    EndBeforeStart,
    #[error("некорректное расписание: {0}")]
    InvalidCadence(String),
    #[error("расписание создаёт слишком много записей")]
    TooManyEntries,
}

pub fn build_series_plan(
    request: &SeriesPlanRequest,
) -> Result<Vec<SeriesEntryPlan>, SeriesPlanError> {
    let start = parse_required_date(
        &request.start_date,
        request.default_year,
        SeriesPlanError::MissingStartDate,
        SeriesPlanError::InvalidStartDate,
    )?;
    let end = parse_required_date(
        &request.end_date,
        request.default_year,
        SeriesPlanError::MissingEndDate,
        SeriesPlanError::InvalidEndDate,
    )?;
    if end < start {
        return Err(SeriesPlanError::EndBeforeStart);
    }
    validate_skip_weekdays(&request.skip_weekdays)?;
    let excluded_dates = normalize_excluded_dates(&request.excluded_dates, request.default_year)?;
    let first = start + Duration::days(i64::from(request.start_offset_days));
    if first > end {
        return Ok(Vec::new());
    }
    let should_skip = |date: NaiveDate| {
        request
            .skip_weekdays
            .contains(&date.weekday().number_from_monday())
            || excluded_dates.contains(&date)
    };

    let mut entries = Vec::new();
    match &request.cadence {
        SeriesCadence::Daily => {
            let mut current = first;
            while current <= end {
                if !should_skip(current) {
                    push_entry(&mut entries, start, current, None)?;
                }
                current += Duration::days(1);
            }
        }
        SeriesCadence::DayOffsets(offsets) => {
            let mut normalized = offsets.clone();
            normalized.sort_unstable();
            normalized.dedup();
            for offset in normalized {
                if offset < request.start_offset_days {
                    continue;
                }
                let date = start + Duration::days(i64::from(offset));
                if date >= first && date <= end && !should_skip(date) {
                    push_entry(&mut entries, start, date, None)?;
                }
            }
        }
        SeriesCadence::DayOffsetsFixedTimes { day_offsets, times } => {
            let times = normalize_times(times)?;
            let mut offsets = day_offsets.clone();
            offsets.sort_unstable();
            offsets.dedup();
            for offset in offsets {
                if offset < request.start_offset_days {
                    continue;
                }
                let date = start + Duration::days(i64::from(offset));
                if date < first || date > end || should_skip(date) {
                    continue;
                }
                for time in &times {
                    push_entry(&mut entries, start, date, Some(*time))?;
                }
            }
        }
        SeriesCadence::DayOffsetsMinuteInterval {
            day_offsets,
            minutes,
        } => {
            if *minutes == 0 || *minutes > 24 * 60 {
                return Err(SeriesPlanError::InvalidCadence(
                    "интервал должен быть от 1 до 1440 минут".into(),
                ));
            }
            let start_time = parse_time(request.day_start_time.as_deref().unwrap_or("00:00"))?;
            let end_time = parse_time(request.day_end_time.as_deref().unwrap_or("23:59"))?;
            if end_time < start_time {
                return Err(SeriesPlanError::InvalidCadence(
                    "время окончания дня раньше времени начала".into(),
                ));
            }
            let mut offsets = day_offsets.clone();
            offsets.sort_unstable();
            offsets.dedup();
            for offset in offsets {
                if offset < request.start_offset_days {
                    continue;
                }
                let date = start + Duration::days(i64::from(offset));
                if date < first || date > end || should_skip(date) {
                    continue;
                }
                let mut current_time = start_time;
                while current_time <= end_time {
                    push_entry(&mut entries, start, date, Some(current_time))?;
                    let next = NaiveDateTime::new(date, current_time)
                        + Duration::minutes(i64::from(*minutes));
                    if next.date() != date {
                        break;
                    }
                    current_time = next.time();
                }
            }
        }
        SeriesCadence::FixedTimes(raw_times) => {
            let times = normalize_times(raw_times)?;
            let mut current = first;
            while current <= end {
                if !should_skip(current) {
                    for time in &times {
                        push_entry(&mut entries, start, current, Some(*time))?;
                    }
                }
                current += Duration::days(1);
            }
        }
        SeriesCadence::MinuteInterval(minutes) => {
            if *minutes == 0 || *minutes > 24 * 60 {
                return Err(SeriesPlanError::InvalidCadence(
                    "интервал должен быть от 1 до 1440 минут".into(),
                ));
            }
            let start_time = parse_time(request.day_start_time.as_deref().unwrap_or("00:00"))?;
            let end_time = parse_time(request.day_end_time.as_deref().unwrap_or("23:59"))?;
            if end_time < start_time {
                return Err(SeriesPlanError::InvalidCadence(
                    "время окончания дня раньше времени начала".into(),
                ));
            }
            let mut current_date = first;
            while current_date <= end {
                if !should_skip(current_date) {
                    let mut current_time = start_time;
                    while current_time <= end_time {
                        push_entry(&mut entries, start, current_date, Some(current_time))?;
                        let next = NaiveDateTime::new(current_date, current_time)
                            + Duration::minutes(i64::from(*minutes));
                        if next.date() != current_date {
                            break;
                        }
                        current_time = next.time();
                    }
                }
                current_date += Duration::days(1);
            }
        }
    }
    Ok(entries)
}

fn validate_skip_weekdays(values: &[u32]) -> Result<(), SeriesPlanError> {
    if values.iter().any(|value| !(1..=7).contains(value)) {
        return Err(SeriesPlanError::InvalidCadence(
            "дни недели для исключения должны быть от 1 до 7".into(),
        ));
    }
    Ok(())
}

fn normalize_excluded_dates(
    values: &[String],
    default_year: i32,
) -> Result<BTreeSet<NaiveDate>, SeriesPlanError> {
    values
        .iter()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            let normalized = parse_flexible_date(value, default_year).ok_or_else(|| {
                SeriesPlanError::InvalidCadence(format!("некорректная исключённая дата: {value}"))
            })?;
            NaiveDate::parse_from_str(&normalized, "%d.%m.%Y").map_err(|_| {
                SeriesPlanError::InvalidCadence(format!("некорректная исключённая дата: {value}"))
            })
        })
        .collect()
}

fn parse_required_date<F>(
    raw: &str,
    default_year: i32,
    missing: SeriesPlanError,
    invalid: F,
) -> Result<NaiveDate, SeriesPlanError>
where
    F: FnOnce(String) -> SeriesPlanError,
{
    if raw.trim().is_empty() {
        return Err(missing);
    }
    let normalized = parse_flexible_date(raw, default_year).ok_or_else(|| invalid(raw.into()))?;
    NaiveDate::parse_from_str(&normalized, "%d.%m.%Y")
        .map_err(|_| SeriesPlanError::InvalidCadence(format!("некорректная дата: {raw}")))
}

fn normalize_times(raw_times: &[String]) -> Result<Vec<NaiveTime>, SeriesPlanError> {
    let mut times = raw_times
        .iter()
        .map(|value| parse_time(value))
        .collect::<Result<Vec<_>, _>>()?;
    times.sort_unstable();
    times.dedup();
    if times.is_empty() {
        return Err(SeriesPlanError::InvalidCadence(
            "не указано ни одного времени".into(),
        ));
    }
    Ok(times)
}

fn parse_time(value: &str) -> Result<NaiveTime, SeriesPlanError> {
    let trimmed = value.trim().replace('.', ":");
    NaiveTime::parse_from_str(&trimmed, "%H:%M")
        .map_err(|_| SeriesPlanError::InvalidCadence(format!("некорректное время: {value}")))
}

fn push_entry(
    entries: &mut Vec<SeriesEntryPlan>,
    origin: NaiveDate,
    date: NaiveDate,
    time: Option<NaiveTime>,
) -> Result<(), SeriesPlanError> {
    if entries.len() >= MAX_SERIES_ENTRIES {
        return Err(SeriesPlanError::TooManyEntries);
    }
    let time_text = time.map(|value| value.format("%H:%M").to_string());
    let datetime = match time {
        Some(value) => NaiveDateTime::new(date, value)
            .format("%d.%m.%Y %H:%M")
            .to_string(),
        None => date.format("%d.%m.%Y").to_string(),
    };
    entries.push(SeriesEntryPlan {
        sequence: entries.len() as u32 + 1,
        offset_days: (date - origin).num_days() as i32,
        date: date.format("%d.%m.%Y").to_string(),
        time: time_text,
        datetime,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(cadence: SeriesCadence) -> SeriesPlanRequest {
        SeriesPlanRequest {
            start_date: "01.06.2026".into(),
            end_date: "03.06.2026".into(),
            default_year: 2026,
            start_offset_days: 1,
            cadence,
            day_start_time: None,
            day_end_time: None,
            skip_weekdays: Vec::new(),
            excluded_dates: Vec::new(),
        }
    }

    #[test]
    fn daily_series_can_start_next_day_and_stop_on_end_date() {
        let plan = build_series_plan(&request(SeriesCadence::Daily)).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.date.as_str()).collect::<Vec<_>>(),
            vec!["02.06.2026", "03.06.2026"]
        );
    }

    #[test]
    fn selected_offsets_are_sorted_deduplicated_and_bounded() {
        let plan =
            build_series_plan(&request(SeriesCadence::DayOffsets(vec![3, 1, 2, 2]))).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.offset_days).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn fixed_times_create_multiple_profession_neutral_entries() {
        let mut req = request(SeriesCadence::FixedTimes(vec![
            "18:00".into(),
            "09:00".into(),
        ]));
        req.end_date = "02.06.2026".into();
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(plan[0].datetime, "02.06.2026 09:00");
        assert_eq!(plan[1].datetime, "02.06.2026 18:00");
    }

    #[test]
    fn minute_rhythm_is_bounded_by_working_window() {
        let mut req = request(SeriesCadence::MinuteInterval(30));
        req.end_date = "02.06.2026".into();
        req.day_start_time = Some("09:00".into());
        req.day_end_time = Some("10:00".into());
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.time.as_deref()).collect::<Vec<_>>(),
            vec![Some("09:00"), Some("09:30"), Some("10:00")]
        );
    }

    #[test]
    fn selected_days_can_use_fixed_times_without_expanding_to_other_days() {
        let mut req = request(SeriesCadence::DayOffsetsFixedTimes {
            day_offsets: vec![1, 3],
            times: vec!["08:00".into(), "20:00".into()],
        });
        req.end_date = "05.06.2026".into();
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.datetime.as_str()).collect::<Vec<_>>(),
            vec![
                "02.06.2026 08:00",
                "02.06.2026 20:00",
                "04.06.2026 08:00",
                "04.06.2026 20:00",
            ]
        );
    }

    #[test]
    fn selected_days_can_use_minute_rhythm_without_expanding_to_other_days() {
        let mut req = request(SeriesCadence::DayOffsetsMinuteInterval {
            day_offsets: vec![1, 3],
            minutes: 240,
        });
        req.end_date = "05.06.2026".into();
        req.day_start_time = Some("08:00".into());
        req.day_end_time = Some("12:00".into());
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.datetime.as_str()).collect::<Vec<_>>(),
            vec![
                "02.06.2026 08:00",
                "02.06.2026 12:00",
                "04.06.2026 08:00",
                "04.06.2026 12:00",
            ]
        );
    }

    #[test]
    fn weekends_and_explicit_dates_can_be_omitted_for_any_profession() {
        let mut req = request(SeriesCadence::Daily);
        req.start_date = "05.06.2026".into();
        req.end_date = "09.06.2026".into();
        req.start_offset_days = 0;
        req.skip_weekdays = vec![6, 7];
        req.excluded_dates = vec!["08.06.2026".into()];
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter()
                .map(|entry| entry.date.as_str())
                .collect::<Vec<_>>(),
            vec!["05.06.2026", "09.06.2026"]
        );
    }

    #[test]
    fn invalid_weekday_filter_is_rejected() {
        let mut req = request(SeriesCadence::Daily);
        req.skip_weekdays = vec![0, 8];
        assert!(matches!(
            build_series_plan(&req),
            Err(SeriesPlanError::InvalidCadence(_))
        ));
    }
}
