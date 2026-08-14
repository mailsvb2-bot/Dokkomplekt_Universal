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
    // Backward-compatible medical-profile names. They resolve through generic
    // period fields first, then medical aliases.
    AdmissionDate,
    DischargeDate,
    AdmissionAndDischargeDates,
    AdmissionMonth,
    DischargeMonth,
}

pub fn build_output_folder_name(case: &SemanticCase, parts: &[FolderNamePart]) -> String {
    let mut chunks = Vec::new();
    for part in parts {
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
    let mut out = parts[0].to_string();
    for part in parts.iter().skip(1).take(2) {
        if let Some(ch) = part.chars().next() {
            out.push(' ');
            out.push(ch);
            out.push('.');
        }
    }
    out
}
fn surname_given_name(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
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
    fn reserved_windows_names_and_trailing_dots_are_neutralized() {
        assert_eq!(sanitize_folder_name("CON."), "_CON");
        assert_eq!(sanitize_folder_name("Отчёт..."), "Отчёт");
    }
}
