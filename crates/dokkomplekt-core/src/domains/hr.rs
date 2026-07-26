use crate::core::FieldExtractionRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HrProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn hr_profile() -> HrProfile {
    HrProfile {
        id: "hr".into(),
        field_rules: vec![
            rule(
                "employee.name",
                &["Сотрудник", "Работник", "ФИО сотрудника"],
            ),
            FieldExtractionRule {
                field_id: "employee.position".into(),
                aliases: vec!["Должность".into()],
                required: false,
            },
            FieldExtractionRule {
                field_id: "employee.department".into(),
                aliases: vec!["Подразделение".into()],
                required: false,
            },
            rule(
                "employee.hire_date",
                &["Дата приёма", "Дата приема", "Приступить к работе"],
            ),
            rule("employee.salary", &["Оклад", "Заработная плата"]),
            rule(
                "employee.contract_number",
                &["Трудовой договор №", "Номер трудового договора"],
            ),
            rule("employee.tab_number", &["Табельный номер"]),
            rule("hr.order_number", &["Приказ №", "Номер приказа"]),
            rule("hr.order_date", &["Дата приказа"]),
            rule("org.name", &["Работодатель", "Организация"]),
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

pub fn canonical_hr_role(raw: &str) -> String {
    let value = raw.trim().to_lowercase();
    if value.contains("соглас") && value.contains("персон") {
        "personal_data_consent".into()
    } else if value.contains("ознаком") || value.contains("familiarization") {
        "familiarization_sheet".into()
    } else if value.contains("приказ") || value.contains("order") {
        "employment_order".into()
    } else if value.contains("трудов") || value.contains("employment_contract") {
        "employment_contract".into()
    } else {
        value
    }
}
