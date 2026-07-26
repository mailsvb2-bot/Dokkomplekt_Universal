use crate::core::FieldExtractionRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EducationProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn education_profile() -> EducationProfile {
    EducationProfile {
        id: "education".into(),
        field_rules: vec![
            rule(
                "education.student_name",
                &["Студент", "Учащийся", "Ученик", "Обучающийся"],
            ),
            FieldExtractionRule {
                field_id: "education.group".into(),
                aliases: vec!["Группа".into(), "Класс".into()],
                required: false,
            },
            rule("education.course", &["Курс", "Предмет", "Дисциплина"]),
            rule("education.grade", &["Оценка", "Балл", "Результат"]),
            rule(
                "education.institution",
                &["Учебное заведение", "Образовательная организация"],
            ),
        ],
    }
}

fn rule(field_id: &str, aliases: &[&str]) -> FieldExtractionRule {
    FieldExtractionRule {
        field_id: field_id.into(),
        aliases: aliases.iter().map(|value| value.to_string()).collect(),
        required: false,
    }
}

pub fn canonical_education_role(raw: &str) -> String {
    let value = raw.trim().to_lowercase();
    if value.contains("ведом") || value.contains("успеваем") || value.contains("grade")
    {
        "grade_report".into()
    } else if value.contains("справ") || value.contains("об обуч") || value.contains("certificate")
    {
        "certificate".into()
    } else {
        value
    }
}
