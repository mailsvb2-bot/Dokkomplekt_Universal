use crate::SemanticCase;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderNamePart {
    FullSubjectName,
    ShortInitials,
    SurnameGivenName,
    OrganizationName,
    DocumentNumber,
    DocumentDate,
    PeriodStartDate,
    PeriodEndDate,
    PeriodRange,
    PeriodStartMonth,
    PeriodEndMonth,
    ShortPeriodStartDate,
    ShortPeriodEndDate,
    ShortPeriodRange,
    PeriodStartMonthName,
    PeriodEndMonthName,
    // Backward-compatible medical-profile names. They resolve through generic
    // period fields first, then medical aliases.
    AdmissionDate,
    DischargeDate,
    AdmissionAndDischargeDates,
    AdmissionMonth,
    DischargeMonth,
}

const DEFAULT_OUTPUT_FOLDER_PARTS: [FolderNamePart; 2] =
    [FolderNamePart::DocumentNumber, FolderNamePart::DocumentDate];

fn effective_output_folder_parts(parts: &[FolderNamePart]) -> &[FolderNamePart] {
    if parts.is_empty() {
        &DEFAULT_OUTPUT_FOLDER_PARTS
    } else {
        parts
    }
}

pub fn missing_output_folder_fields(case: &SemanticCase, parts: &[FolderNamePart]) -> Vec<String> {
    let mut missing = std::collections::BTreeSet::<String>::new();
    let mut require = |present: bool, field_id: &str| {
        if !present {
            missing.insert(field_id.to_string());
        }
    };
    for part in effective_output_folder_parts(parts) {
        match part {
            FolderNamePart::FullSubjectName
            | FolderNamePart::ShortInitials
            | FolderNamePart::SurnameGivenName => require(
                first(case, &["subject.name", "person.full_name", "patient.fio"]).is_some(),
                "subject.name",
            ),
            FolderNamePart::OrganizationName => require(
                first(
                    case,
                    &["organization.name", "org.name", "subject.organization"],
                )
                .is_some(),
                "org.name",
            ),
            FolderNamePart::DocumentNumber => require(
                first(
                    case,
                    &["document.number", "case.number", "medical.case_number"],
                )
                .is_some(),
                "document.number",
            ),
            FolderNamePart::DocumentDate => {
                require(case.get("document.date").is_some(), "document.date")
            }
            FolderNamePart::PeriodStartDate
            | FolderNamePart::PeriodStartMonth
            | FolderNamePart::ShortPeriodStartDate
            | FolderNamePart::PeriodStartMonthName
            | FolderNamePart::AdmissionDate
            | FolderNamePart::AdmissionMonth => require(
                first(case, &["period.start_date", "medical.admission_date"]).is_some(),
                "period.start_date",
            ),
            FolderNamePart::PeriodEndDate
            | FolderNamePart::PeriodEndMonth
            | FolderNamePart::ShortPeriodEndDate
            | FolderNamePart::PeriodEndMonthName
            | FolderNamePart::DischargeDate
            | FolderNamePart::DischargeMonth => require(
                first(case, &["period.end_date", "medical.discharge_date"]).is_some(),
                "period.end_date",
            ),
            FolderNamePart::PeriodRange
            | FolderNamePart::ShortPeriodRange
            | FolderNamePart::AdmissionAndDischargeDates => {
                require(
                    first(case, &["period.start_date", "medical.admission_date"]).is_some(),
                    "period.start_date",
                );
                require(
                    first(case, &["period.end_date", "medical.discharge_date"]).is_some(),
                    "period.end_date",
                );
            }
        }
    }
    missing.into_iter().collect()
}

pub fn build_output_folder_name(case: &SemanticCase, parts: &[FolderNamePart]) -> String {
    let mut chunks = Vec::new();
    for part in effective_output_folder_parts(parts) {
        match part {
            FolderNamePart::FullSubjectName => push(
                first(case, &["subject.name", "person.full_name", "patient.fio"]),
                &mut chunks,
            ),
            FolderNamePart::ShortInitials => {
                if let Some(name) =
                    first(case, &["subject.name", "person.full_name", "patient.fio"])
                {
                    chunks.push(short_initials(name));
                }
            }
            FolderNamePart::SurnameGivenName => {
                if let Some(name) =
                    first(case, &["subject.name", "person.full_name", "patient.fio"])
                {
                    chunks.push(surname_given_name(name));
                }
            }
            FolderNamePart::OrganizationName => push(
                first(
                    case,
                    &["organization.name", "org.name", "subject.organization"],
                ),
                &mut chunks,
            ),
            FolderNamePart::DocumentNumber => push(
                first(
                    case,
                    &["document.number", "case.number", "medical.case_number"],
                ),
                &mut chunks,
            ),
            FolderNamePart::DocumentDate => push(case.get("document.date"), &mut chunks),
            FolderNamePart::PeriodStartDate | FolderNamePart::AdmissionDate => push(
                first(case, &["period.start_date", "medical.admission_date"]),
                &mut chunks,
            ),
            FolderNamePart::PeriodEndDate | FolderNamePart::DischargeDate => push(
                first(case, &["period.end_date", "medical.discharge_date"]),
                &mut chunks,
            ),
            FolderNamePart::PeriodRange | FolderNamePart::AdmissionAndDischargeDates => {
                if let (Some(start), Some(end)) = (
                    first(case, &["period.start_date", "medical.admission_date"]),
                    first(case, &["period.end_date", "medical.discharge_date"]),
                ) {
                    chunks.push(format!("{start} - {end}"));
                }
            }
            FolderNamePart::PeriodStartMonth | FolderNamePart::AdmissionMonth => push(
                month_from_date(first(
                    case,
                    &["period.start_date", "medical.admission_date"],
                ))
                .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::PeriodEndMonth | FolderNamePart::DischargeMonth => push(
                month_from_date(first(case, &["period.end_date", "medical.discharge_date"]))
                    .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodStartDate => push(
                short_date(first(
                    case,
                    &["period.start_date", "medical.admission_date"],
                ))
                .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodEndDate => push(
                short_date(first(case, &["period.end_date", "medical.discharge_date"])).as_deref(),
                &mut chunks,
            ),
            FolderNamePart::ShortPeriodRange => {
                if let (Some(start), Some(end)) = (
                    short_date(first(
                        case,
                        &["period.start_date", "medical.admission_date"],
                    )),
                    short_date(first(case, &["period.end_date", "medical.discharge_date"])),
                ) {
                    chunks.push(format!("{start}-{end}"));
                }
            }
            FolderNamePart::PeriodStartMonthName => push(
                month_name_from_date(first(
                    case,
                    &["period.start_date", "medical.admission_date"],
                ))
                .as_deref(),
                &mut chunks,
            ),
            FolderNamePart::PeriodEndMonthName => push(
                month_name_from_date(first(case, &["period.end_date", "medical.discharge_date"]))
                    .as_deref(),
                &mut chunks,
            ),
        }
    }
    let name = sanitize_folder_name(&chunks.join(" "));
    if name.is_empty() {
        "Созданные документы".into()
    } else {
        name
    }
}

pub fn sanitize_folder_name(value: &str) -> String {
    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let mut out = value
        .chars()
        .map(|ch| {
            if invalid.contains(&ch) || ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    out = out.trim_matches([' ', '.']).to_string();
    if is_windows_reserved(&out) {
        out = format!("_{out}");
    }
    if out.chars().count() > 120 {
        out = out
            .chars()
            .take(120)
            .collect::<String>()
            .trim_end_matches([' ', '.'])
            .to_string();
    }
    out
}

fn first<'a>(case: &'a SemanticCase, ids: &[&str]) -> Option<&'a str> {
    ids.iter().find_map(|id| case.get(id))
}
fn push(value: Option<&str>, chunks: &mut Vec<String>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        chunks.push(value.to_string());
    }
}
fn short_initials(name: &str) -> String {
    let parts = name.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return name.to_string();
    }
    let mut initials = String::new();
    for part in parts.iter().skip(1).take(2) {
        if let Some(ch) = part.chars().next() {
            initials.push(ch);
            initials.push('.');
        }
    }
    if initials.is_empty() {
        parts[0].to_string()
    } else {
        format!("{} {initials}", parts[0])
    }
}
fn surname_given_name(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}
fn short_date(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts[2].len() < 2 {
        return Some(value.to_string());
    }
    let year = &parts[2][parts[2].len() - 2..];
    Some(format!("{}.{}.{year}", parts[0], parts[1]))
}

fn month_name_from_date(value: Option<&str>) -> Option<String> {
    let mut parts = value?.split('.');
    let _day = parts.next()?;
    let month = parts.next()?.parse::<usize>().ok()?;
    let year = parts.next()?;
    let names = [
        "январь",
        "февраль",
        "март",
        "апрель",
        "май",
        "июнь",
        "июль",
        "август",
        "сентябрь",
        "октябрь",
        "ноябрь",
        "декабрь",
    ];
    let name = names.get(month.checked_sub(1)?)?;
    Some(format!("{name} {year}"))
}

fn month_from_date(value: Option<&str>) -> Option<String> {
    let mut parts = value?.split('.');
    let _day = parts.next()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let year = parts.next()?;
    Some(format!("{month:02}.{year}"))
}
fn is_windows_reserved(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .and_then(|x| x.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
        || stem
            .strip_prefix("LPT")
            .and_then(|x| x.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};
    #[test]
    fn generic_period_and_document_parts_form_safe_folder_name() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Иванов Иван"),
            ("document.number", "A/42"),
            ("period.start_date", "01.06.2026"),
            ("period.end_date", "30.06.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[
                    FolderNamePart::FullSubjectName,
                    FolderNamePart::DocumentNumber,
                    FolderNamePart::PeriodRange
                ]
            ),
            "Иванов Иван A 42 01.06.2026 - 30.06.2026"
        );
    }
    #[test]
    fn surname_given_name_and_generic_months_preserve_old_folder_choices() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Иванов Иван Иванович"),
            ("period.start_date", "01.06.2026"),
            ("period.end_date", "31.07.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[
                    FolderNamePart::SurnameGivenName,
                    FolderNamePart::PeriodStartMonth,
                    FolderNamePart::PeriodEndMonth,
                ],
            ),
            "Иванов Иван 06.2026 07.2026"
        );
    }

    #[test]
    fn empty_parts_use_safe_default_identity_fields() {
        let mut case = SemanticCase::default();
        for (id, value) in [("document.number", "42"), ("document.date", "18.06.2026")] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert!(missing_output_folder_fields(&case, &[]).is_empty());
        assert_eq!(build_output_folder_name(&case, &[]), "42 18.06.2026");

        let empty = SemanticCase::default();
        assert_eq!(
            missing_output_folder_fields(&empty, &[]),
            vec!["document.date".to_string(), "document.number".to_string()]
        );
    }

    #[test]
    fn donor_short_date_range_is_available_without_changing_long_range() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Петров Петр Петрович"),
            ("period.start_date", "01.06.2026"),
            ("period.end_date", "12.06.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[
                    FolderNamePart::ShortInitials,
                    FolderNamePart::ShortPeriodRange
                ],
            ),
            "Петров П.П. 01.06.26-12.06.26"
        );
    }

    #[test]
    fn folder_naming_missing_fields_use_the_same_semantic_fallbacks_as_rendering() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "medical.case_number".into(),
            SemanticValue::new(
                "medical.case_number",
                "42/26",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        case.values.insert(
            "medical.admission_date".into(),
            SemanticValue::new(
                "medical.admission_date",
                "01.06.2026",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        assert_eq!(
            missing_output_folder_fields(
                &case,
                &[
                    FolderNamePart::DocumentNumber,
                    FolderNamePart::AdmissionDate,
                    FolderNamePart::FullSubjectName,
                    FolderNamePart::DischargeDate,
                ],
            ),
            vec!["period.end_date".to_string(), "subject.name".to_string()]
        );
    }

    #[test]
    fn donor_word_month_is_profession_neutral() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("subject.name", "Сидоров Сергей Сергеевич"),
            ("period.start_date", "01.06.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        assert_eq!(
            build_output_folder_name(
                &case,
                &[
                    FolderNamePart::FullSubjectName,
                    FolderNamePart::PeriodStartMonthName
                ],
            ),
            "Сидоров Сергей Сергеевич июнь 2026"
        );
    }

    #[test]
    fn reserved_windows_names_and_trailing_dots_are_neutralized() {
        assert_eq!(sanitize_folder_name("CON."), "_CON");
        assert_eq!(sanitize_folder_name("Отчёт..."), "Отчёт");
    }
}
