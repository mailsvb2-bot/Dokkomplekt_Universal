//! Medical-profile compatibility façade over the domain-neutral record-series engine.
//!
//! The generic engine remains profession-neutral. This module carries the proven
//! medical diary behaviour from the legacy applications: D0+1 start, discharge
//! boundary, specialist-confirmed cadence priority, final discharge entry,
//! signatures and 01-31 template-folder compatibility.

use crate::{
    build_series_plan, parse_flexible_date, SeriesCadence, SeriesEntryPlan, SeriesPlanError,
    SeriesPlanRequest,
};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaryEntryPlan {
    pub day_number: u32,
    pub date: String,
    pub month: u32,
    pub year: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiaryPlanError {
    MissingAdmissionDate,
    MissingDischargeDate,
    InvalidDate(String),
    DischargeBeforeAdmission,
    Series(String),
}

pub fn build_diary_plan(
    admission: Option<&str>,
    discharge: Option<&str>,
    default_year: i32,
) -> Result<Vec<DiaryEntryPlan>, DiaryPlanError> {
    let admission = admission.ok_or(DiaryPlanError::MissingAdmissionDate)?;
    let discharge = discharge.ok_or(DiaryPlanError::MissingDischargeDate)?;
    let series = build_series_plan(&SeriesPlanRequest {
        start_date: admission.into(),
        end_date: discharge.into(),
        default_year,
        start_offset_days: 1,
        cadence: SeriesCadence::Daily,
        day_start_time: None,
        day_end_time: None,
        skip_weekdays: Vec::new(),
        excluded_dates: Vec::new(),
    })
    .map_err(map_series_error)?;
    series
        .into_iter()
        .map(|entry| {
            let date = NaiveDate::parse_from_str(&entry.date, "%d.%m.%Y")
                .map_err(|_| DiaryPlanError::InvalidDate(entry.date.clone()))?;
            Ok(DiaryEntryPlan {
                day_number: entry.sequence,
                date: entry.date,
                month: date.month(),
                year: date.year(),
            })
        })
        .collect()
}

fn map_series_error(error: SeriesPlanError) -> DiaryPlanError {
    match error {
        SeriesPlanError::MissingStartDate => DiaryPlanError::MissingAdmissionDate,
        SeriesPlanError::MissingEndDate => DiaryPlanError::MissingDischargeDate,
        SeriesPlanError::InvalidStartDate(value) | SeriesPlanError::InvalidEndDate(value) => {
            DiaryPlanError::InvalidDate(value)
        }
        SeriesPlanError::EndBeforeStart => DiaryPlanError::DischargeBeforeAdmission,
        other => DiaryPlanError::Series(other.to_string()),
    }
}

/// Extract the first plausible day number from names such as `1.docx`,
/// `01.docx`, `№12.docx` or `12 (дежурный).docx`.
///
/// We intentionally do not concatenate every digit in the name: the legacy
/// implementation could turn `12 (2026).docx` into `122026` and lose the match.
pub fn normalize_diary_template_number(name: &str) -> Option<u32> {
    let stem = name
        .rsplit_once('.')
        .map(|(value, _)| value)
        .unwrap_or(name);
    let mut digits = String::new();
    for ch in stem.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if !digits.is_empty() {
            if let Ok(value) = digits.parse::<u32>() {
                if (1..=31).contains(&value) {
                    return Some(value);
                }
            }
            digits.clear();
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaryTemplateSelection {
    pub file_name: String,
    pub template_day: u32,
    pub used_next_day_fallback: bool,
}

/// Select a file from a specialist-owned `01-31` template folder.
/// Exact admission-day match wins. The next-day fallback preserves compatibility
/// with old packs whose file number denoted the first diary day (D0+1).
pub fn select_diary_template_for_admission(
    file_names: &[String],
    admission_day: u32,
) -> Option<DiaryTemplateSelection> {
    if !(1..=31).contains(&admission_day) {
        return None;
    }
    let find = |day| {
        file_names
            .iter()
            .find(|name| normalize_diary_template_number(name) == Some(day))
            .map(|name| name.to_string())
    };
    if let Some(file_name) = find(admission_day) {
        return Some(DiaryTemplateSelection {
            file_name,
            template_day: admission_day,
            used_next_day_fallback: false,
        });
    }
    let fallback_day = if admission_day == 31 {
        1
    } else {
        admission_day + 1
    };
    find(fallback_day).map(|file_name| DiaryTemplateSelection {
        file_name,
        template_day: fallback_day,
        used_next_day_fallback: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalDiarySeriesRequest {
    pub admission_date: String,
    pub discharge_date: String,
    pub default_year: i32,
    /// Values explicitly confirmed by the specialist always outrank a profile default.
    #[serde(default)]
    pub confirmed_cadence: Option<SeriesCadence>,
    #[serde(default)]
    pub profile_cadence: Option<SeriesCadence>,
    #[serde(default)]
    pub day_start_time: Option<String>,
    #[serde(default)]
    pub day_end_time: Option<String>,
    #[serde(default)]
    pub skip_weekdays: Vec<u32>,
    #[serde(default)]
    pub excluded_dates: Vec<String>,
    #[serde(default = "default_true")]
    pub force_final_discharge_entry: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalDiarySeriesEntry {
    pub sequence: u32,
    pub offset_days: i32,
    pub date: String,
    pub time: Option<String>,
    pub datetime: String,
    pub is_final_discharge_entry: bool,
    pub signatures: Vec<String>,
}

/// Build the complete medical-profile diary schedule using the generic series engine.
///
/// No ready-made clinical text is embedded. Only behavioural rules are carried:
/// D0+1, discharge limit, specialist-confirmed rhythm, optional final discharge
/// record and signature slots.
pub fn build_medical_diary_series(
    request: &MedicalDiarySeriesRequest,
) -> Result<Vec<MedicalDiarySeriesEntry>, DiaryPlanError> {
    let cadence = request
        .confirmed_cadence
        .clone()
        .or_else(|| request.profile_cadence.clone())
        .unwrap_or(SeriesCadence::Daily);
    let mut series = build_series_plan(&SeriesPlanRequest {
        start_date: request.admission_date.clone(),
        end_date: request.discharge_date.clone(),
        default_year: request.default_year,
        start_offset_days: 1,
        cadence,
        day_start_time: request.day_start_time.clone(),
        day_end_time: request.day_end_time.clone(),
        skip_weekdays: request.skip_weekdays.clone(),
        excluded_dates: request.excluded_dates.clone(),
    })
    .map_err(map_series_error)?;

    let normalized_discharge =
        parse_flexible_date(&request.discharge_date, request.default_year)
            .ok_or_else(|| DiaryPlanError::InvalidDate(request.discharge_date.clone()))?;
    if request.force_final_discharge_entry
        && !series
            .iter()
            .any(|entry| entry.date == normalized_discharge)
    {
        let admission = parse_flexible_date(&request.admission_date, request.default_year)
            .ok_or_else(|| DiaryPlanError::InvalidDate(request.admission_date.clone()))?;
        let admission = NaiveDate::parse_from_str(&admission, "%d.%m.%Y")
            .map_err(|_| DiaryPlanError::InvalidDate(request.admission_date.clone()))?;
        let discharge = NaiveDate::parse_from_str(&normalized_discharge, "%d.%m.%Y")
            .map_err(|_| DiaryPlanError::InvalidDate(request.discharge_date.clone()))?;
        let offset_days = (discharge - admission).num_days();
        let offset_days = i32::try_from(offset_days)
            .map_err(|_| DiaryPlanError::Series("слишком большой период дневников".into()))?;
        series.push(SeriesEntryPlan {
            sequence: 0,
            offset_days,
            date: normalized_discharge.clone(),
            time: None,
            datetime: normalized_discharge.clone(),
        });
    }

    for (index, entry) in series.iter_mut().enumerate() {
        entry.sequence = u32::try_from(index + 1).unwrap_or(u32::MAX);
    }
    let final_index = if request.force_final_discharge_entry {
        series
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.date == normalized_discharge)
            .map(|(index, _)| index)
            .next_back()
    } else {
        None
    };

    Ok(series
        .into_iter()
        .enumerate()
        .map(|(index, entry)| MedicalDiarySeriesEntry {
            sequence: entry.sequence,
            offset_days: entry.offset_days,
            date: entry.date,
            time: entry.time,
            datetime: entry.datetime,
            is_final_discharge_entry: final_index == Some(index),
            signatures: vec![
                "Лечащий врач __________________ /____________/".to_string(),
                "Заведующий отделением __________ /____________/".to_string(),
            ],
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_number_uses_first_day_group_instead_of_concatenating_year() {
        assert_eq!(normalize_diary_template_number("№12 (2026).docx"), Some(12));
        assert_eq!(
            normalize_diary_template_number("шаблон 31 финал.docm"),
            Some(31)
        );
        assert_eq!(normalize_diary_template_number("шаблон 2026.docx"), None);
    }

    #[test]
    fn template_selection_prefers_exact_then_legacy_next_day() {
        let exact = vec!["13 (старый набор).docx".to_string(), "12.docx".to_string()];
        let selected = select_diary_template_for_admission(&exact, 12).unwrap();
        assert_eq!(selected.file_name, "12.docx");
        assert!(!selected.used_next_day_fallback);

        let fallback = vec!["13 (старый набор).docx".to_string()];
        let selected = select_diary_template_for_admission(&fallback, 12).unwrap();
        assert_eq!(selected.template_day, 13);
        assert!(selected.used_next_day_fallback);
    }

    #[test]
    fn confirmed_schedule_wins_and_final_discharge_entry_is_preserved() {
        let plan = build_medical_diary_series(&MedicalDiarySeriesRequest {
            admission_date: "10.05.2026".into(),
            discharge_date: "13.05.2026".into(),
            default_year: 2026,
            confirmed_cadence: Some(SeriesCadence::DayOffsets(vec![1])),
            profile_cadence: Some(SeriesCadence::Daily),
            day_start_time: None,
            day_end_time: None,
            skip_weekdays: Vec::new(),
            excluded_dates: Vec::new(),
            force_final_discharge_entry: true,
        })
        .unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].date, "11.05.2026");
        assert_eq!(plan[1].date, "13.05.2026");
        assert!(plan[1].is_final_discharge_entry);
        assert_eq!(plan[1].signatures.len(), 2);
    }

    #[test]
    fn hourly_or_minute_series_never_crosses_discharge_boundary() {
        let plan = build_medical_diary_series(&MedicalDiarySeriesRequest {
            admission_date: "10.05.2026".into(),
            discharge_date: "11.05.2026".into(),
            default_year: 2026,
            confirmed_cadence: Some(SeriesCadence::MinuteInterval(60)),
            profile_cadence: None,
            day_start_time: Some("08:00".into()),
            day_end_time: Some("10:00".into()),
            skip_weekdays: Vec::new(),
            excluded_dates: Vec::new(),
            force_final_discharge_entry: true,
        })
        .unwrap();
        assert_eq!(plan.len(), 3);
        assert!(plan.iter().all(|entry| entry.date == "11.05.2026"));
        assert!(plan.last().unwrap().is_final_discharge_entry);
    }
}
