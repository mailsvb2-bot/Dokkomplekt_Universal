use crate::core::FieldExtractionRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn accounting_profile() -> AccountingProfile {
    AccountingProfile {
        id: "accounting".into(),
        field_rules: vec![
            rule(
                "accounting.invoice_number",
                &["Счёт №", "Счет №", "Номер счёта"],
                false,
            ),
            rule(
                "accounting.invoice_date",
                &["Дата счёта", "Дата счета"],
                false,
            ),
            rule(
                "org.name",
                &["Поставщик", "Исполнитель", "Организация"],
                false,
            ),
            rule(
                "counterparty.name",
                &["Покупатель", "Заказчик", "Контрагент"],
                false,
            ),
            rule("amount.total", &["Итого", "К оплате", "Сумма"], false),
            rule("amount.vat", &["НДС", "В том числе НДС"], false),
            rule("amount.currency", &["Валюта", "Код валюты"], false),
            rule("contract.number", &["Договор №", "Основание"], false),
            rule("contract.date", &["Дата договора"], false),
        ],
    }
}

fn rule(field_id: &str, aliases: &[&str], required: bool) -> FieldExtractionRule {
    FieldExtractionRule {
        field_id: field_id.into(),
        aliases: aliases.iter().map(|value| value.to_string()).collect(),
        required,
    }
}

pub fn canonical_accounting_role(raw: &str) -> String {
    let value = raw.trim().to_lowercase();
    if value.contains("сверк") || value.contains("reconciliation") {
        "reconciliation".into()
    } else if value.contains("акт")
        || value.contains("service_act")
        || value.contains("service act")
    {
        "service_act".into()
    } else if value.contains("счёт") || value.contains("счет") || value.contains("invoice")
    {
        "invoice".into()
    } else {
        value
    }
}
