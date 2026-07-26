use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Icd10Suggestion {
    pub code: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MedicalSourceKind {
    PrimaryInspection,
    Referral,
    DischargeEpicrisis,
    DiaryTexts,
    DiaryDates,
    Other,
}

/// Recognize only the medical-profile role of an input. The universal intake
/// router stays domain-neutral and may ignore this helper outside the profile.
pub fn recognize_medical_source(file_name: &str, extracted_text: &str) -> MedicalSourceKind {
    let haystack = format!("{}\n{}", file_name, extracted_text).to_lowercase();
    if contains_any(
        &haystack,
        &["первичный осмотр", "первичный прием", "первичный приём"],
    ) {
        MedicalSourceKind::PrimaryInspection
    } else if contains_any(
        &haystack,
        &["направление на госпитализацию", "направление", "referral"],
    ) {
        MedicalSourceKind::Referral
    } else if contains_any(
        &haystack,
        &["выписной эпикриз", "эпикриз", "discharge summary"],
    ) {
        MedicalSourceKind::DischargeEpicrisis
    } else if contains_any(
        &haystack,
        &["тексты дневников", "тексты наблюдений", "статусы дневников"],
    ) {
        MedicalSourceKind::DiaryTexts
    } else if contains_any(
        &haystack,
        &[
            "даты дневников",
            "таблица дневников",
            "01-31",
            "календарь дневников",
        ],
    ) {
        MedicalSourceKind::DiaryDates
    } else {
        MedicalSourceKind::Other
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// Full offline ICD-10 search routed through the medical domain profile.
/// The universal constructor remains domain-neutral; only this profile knows about ICD-10.
pub fn suggest_icd10(input: &str) -> Vec<Icd10Suggestion> {
    crate::search_icd10(input, 25)
        .into_iter()
        .map(|row| Icd10Suggestion {
            code: row.code,
            title: row.title,
        })
        .collect()
}

pub fn decline_rvk_district(value: &str) -> String {
    let clean = value.trim();
    match clean.to_lowercase().as_str() {
        "автозаводский" => "Автозаводского".into(),
        "ленинский" => "Ленинского".into(),
        "сормовский" => "Сормовского".into(),
        "канавинский" => "Канавинского".into(),
        "московский" => "Московского".into(),
        _ if clean.ends_with("ский") => format!("{}ого", clean.trim_end_matches("ий")),
        _ => clean.to_string(),
    }
}

pub fn treatment_section_is_present(source_text: &str) -> bool {
    let lower = source_text.to_lowercase();
    lower.contains("\nлечение")
        || lower.contains("назначенное лечение")
        || lower.contains("лечение:")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rvk_declension_contract() {
        assert_eq!(decline_rvk_district("Ленинский"), "Ленинского");
        assert_eq!(decline_rvk_district("Канавинский"), "Канавинского");
    }
    #[test]
    fn icd10_accepts_digits_or_text() {
        assert_eq!(suggest_icd10("F20")[0].code, "F20");
        assert!(suggest_icd10("диабет").iter().any(|x| x.code == "E11"));
    }

    #[test]
    fn medical_input_recognition_uses_name_and_content_without_leaking_into_core() {
        assert_eq!(
            recognize_medical_source("Направление.docx", ""),
            MedicalSourceKind::Referral
        );
        assert_eq!(
            recognize_medical_source("unknown.docx", "12.01.2026 Первичный осмотр"),
            MedicalSourceKind::PrimaryInspection
        );
        assert_eq!(
            recognize_medical_source("01-31", ""),
            MedicalSourceKind::DiaryDates
        );
        assert_eq!(
            recognize_medical_source("договор.docx", "Договор оказания услуг"),
            MedicalSourceKind::Other
        );
    }
}
