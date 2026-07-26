use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedFlexibleDate {
    pub normalized: String,
    /// Human-readable note when a compact legacy value required an assumption.
    pub assumption: Option<String>,
}

/// Parses common user-entered date forms into `DD.MM.YYYY`.
/// Supports ISO `YYYY-MM-DD`, compact numeric values plus Russian, English and
/// Polish month names. Four-digit compact input is interpreted only as `DDMM`
/// in the supplied default year; ambiguous historical shorthand is rejected.
pub fn parse_flexible_date(input: &str, default_year: i32) -> Option<String> {
    parse_flexible_date_detailed(input, default_year).map(|value| value.normalized)
}

pub fn parse_flexible_date_detailed(input: &str, default_year: i32) -> Option<ParsedFlexibleDate> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // ISO is intentionally checked before replacing dashes: `2026-05-12`
    // means year-month-day, not day-month-year.
    let iso_candidate = trimmed.get(..10).unwrap_or(trimmed);
    if let Ok(date) = NaiveDate::parse_from_str(iso_candidate, "%Y-%m-%d") {
        return Some(parsed(date.format("%d.%m.%Y").to_string(), None));
    }
    if let Some(date) = parse_word_date(trimmed, default_year) {
        return Some(parsed(date, None));
    }

    let normalized = trimmed.replace(['/', '-'], ".");
    let parts: Vec<&str> = normalized.split('.').filter(|x| !x.is_empty()).collect();
    if parts.len() == 3 {
        let day = parts[0].trim().parse::<u32>().ok()?;
        let month = parts[1].trim().parse::<u32>().ok()?;
        let year = normalize_year(parts[2].trim().parse::<i32>().ok()?, default_year);
        return format_date(day, month, year).map(|value| parsed(value, None));
    }
    if parts.len() == 2 {
        let day = parts[0].trim().parse::<u32>().ok()?;
        let month = parts[1].trim().parse::<u32>().ok()?;
        return format_date(day, month, default_year).map(|value| parsed(value, None));
    }

    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        1 | 2 => format_date(digits.parse().ok()?, 1, default_year).map(|value| {
            parsed(
                value,
                Some("указан только день; использованы январь и год по умолчанию"),
            )
        }),
        4 => {
            // Four compact digits have exactly one interpretation: DDMM in the
            // default year. We intentionally do not guess the old DMY compact
            // form (`1126` -> 01.01.2026), because that silently turns many
            // invalid dates into a different valid date.
            let day = digits[0..2].parse::<u32>().ok()?;
            let month = digits[2..4].parse::<u32>().ok()?;
            format_date(day, month, default_year).map(|value| {
                parsed(
                    value,
                    Some("четыре цифры интерпретированы как ДДММ с годом по умолчанию"),
                )
            })
        }
        6 => format_date(
            digits[0..2].parse().ok()?,
            digits[2..4].parse().ok()?,
            normalize_year(digits[4..6].parse().ok()?, default_year),
        )
        .map(|value| parsed(value, None)),
        8 => format_date(
            digits[0..2].parse().ok()?,
            digits[2..4].parse().ok()?,
            normalize_year(digits[4..8].parse().ok()?, default_year),
        )
        .map(|value| parsed(value, None)),
        _ => None,
    }
}

fn parsed(normalized: String, assumption: Option<&str>) -> ParsedFlexibleDate {
    ParsedFlexibleDate {
        normalized,
        assumption: assumption.map(str::to_string),
    }
}

fn parse_word_date(input: &str, default_year: i32) -> Option<String> {
    let tokens = input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        let Some(month) = month_number(token) else {
            continue;
        };
        let previous = index
            .checked_sub(1)
            .and_then(|i| tokens.get(i))
            .and_then(|v| v.parse::<u32>().ok());
        let next = tokens.get(index + 1).and_then(|v| v.parse::<u32>().ok());
        let after_next = tokens.get(index + 2).and_then(|v| v.parse::<i32>().ok());
        let (day, year) = if let Some(day) = previous {
            let year = next
                .map(|value| normalize_year(value as i32, default_year))
                .unwrap_or(default_year);
            (day, year)
        } else {
            let day = next?;
            let year = after_next
                .map(|value| normalize_year(value, default_year))
                .unwrap_or(default_year);
            (day, year)
        };
        if let Some(date) = format_date(day, month, year) {
            return Some(date);
        }
    }
    None
}

fn month_number(value: &str) -> Option<u32> {
    let month = match value {
        "январь" | "января" | "january" | "jan" | "styczen" | "styczeń" | "stycznia" => {
            1
        }
        "февраль" | "февраля" | "february" | "feb" | "luty" | "lutego" => 2,
        "март" | "марта" | "march" | "mar" | "marzec" | "marca" => 3,
        "апрель" | "апреля" | "april" | "apr" | "kwiecien" | "kwiecień" | "kwietnia" => {
            4
        }
        "май" | "мая" | "may" | "maj" | "maja" => 5,
        "июнь" | "июня" | "june" | "jun" | "czerwiec" | "czerwca" => 6,
        "июль" | "июля" | "july" | "jul" | "lipiec" | "lipca" => 7,
        "август" | "августа" | "august" | "aug" | "sierpien" | "sierpień" | "sierpnia" => {
            8
        }
        "сентябрь" | "сентября" | "september" | "sep" | "sept" | "wrzesien" | "wrzesień"
        | "września" => 9,
        "октябрь" | "октября" | "october" | "oct" | "pazdziernik" | "październik"
        | "października" => 10,
        "ноябрь" | "ноября" | "november" | "nov" | "listopad" | "listopada" => 11,
        "декабрь" | "декабря" | "december" | "dec" | "grudzien" | "grudzień" | "grudnia" => {
            12
        }
        _ => return None,
    };
    Some(month)
}

pub fn normalize_year(year: i32, reference_year: i32) -> i32 {
    if !(0..100).contains(&year) {
        return year;
    }
    // Sliding pivot: a two-digit year may be at most ten years in the future.
    // Everything beyond that pivot belongs to the previous century. This keeps
    // dates of birth such as 12.05.87 in 1987 while still accepting near-future
    // contract dates such as 01.01.35 when the reference year is 2026.
    let upper_bound = reference_year.saturating_add(10);
    let pivot_century = upper_bound.div_euclid(100) * 100;
    let candidate = pivot_century.saturating_add(year);
    if candidate > upper_bound {
        candidate.saturating_sub(100)
    } else {
        candidate
    }
}

fn format_date(day: u32, month: u32, year: i32) -> Option<String> {
    NaiveDate::from_ymd_opt(year, month, day).map(|_| format!("{day:02}.{month:02}.{year:04}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_legacy_compact_values() {
        assert_eq!(
            parse_flexible_date("1", 2026).as_deref(),
            Some("01.01.2026")
        );
        assert_eq!(parse_flexible_date("1126", 2026), None);
        assert_eq!(
            parse_flexible_date("100526", 2026).as_deref(),
            Some("10.05.2026")
        );
    }
    #[test]
    fn two_digit_year_uses_sliding_pivot_instead_of_always_20xx() {
        assert_eq!(
            parse_flexible_date("12.05.87", 2026).as_deref(),
            Some("12.05.1987")
        );
        assert_eq!(
            parse_flexible_date("12.05.35", 2026).as_deref(),
            Some("12.05.2035")
        );
        assert_eq!(
            parse_flexible_date("12.05.37", 2026).as_deref(),
            Some("12.05.1937")
        );
        assert_eq!(normalize_year(5, 2095), 2105);
        assert_eq!(normalize_year(99, 2095), 2099);
    }

    #[test]
    fn parses_iso_date_and_rfc3339_prefix() {
        assert_eq!(
            parse_flexible_date("2026-05-12", 2025).as_deref(),
            Some("12.05.2026")
        );
        assert_eq!(
            parse_flexible_date("2026-05-12T10:15:00Z", 2025).as_deref(),
            Some("12.05.2026")
        );
    }
    #[test]
    fn reports_ddmm_four_digit_assumption() {
        let parsed = parse_flexible_date_detailed("1205", 2026).expect("date");
        assert_eq!(parsed.normalized, "12.05.2026");
        assert!(parsed
            .assumption
            .as_deref()
            .is_some_and(|value| value.contains("ДДММ")));
    }
    #[test]
    fn parses_russian_english_and_polish_word_dates() {
        assert_eq!(
            parse_flexible_date("14 июля 2026", 2025).as_deref(),
            Some("14.07.2026")
        );
        assert_eq!(
            parse_flexible_date("July 14, 2026", 2025).as_deref(),
            Some("14.07.2026")
        );
        assert_eq!(
            parse_flexible_date("14 lipca 2026", 2025).as_deref(),
            Some("14.07.2026")
        );
    }
    #[test]
    fn rejects_impossible_dates() {
        assert!(parse_flexible_date("31.02.2026", 2026).is_none());
    }
}
