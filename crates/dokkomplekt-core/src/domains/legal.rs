use crate::core::FieldExtractionRule;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalProfile {
    pub id: String,
    pub field_rules: Vec<FieldExtractionRule>,
}

pub fn legal_profile() -> LegalProfile {
    LegalProfile {
        id: "legal".into(),
        field_rules: vec![
            rule(
                "contract.number",
                &["Номер договора", "Договор №", "Контракт №"],
            ),
            FieldExtractionRule {
                field_id: "contract.date".into(),
                aliases: vec!["Дата договора".into(), "Дата контракта".into()],
                required: false,
            },
            FieldExtractionRule {
                field_id: "contract.party_a".into(),
                aliases: vec!["Заказчик".into(), "Продавец".into(), "Арендодатель".into()],
                required: false,
            },
            FieldExtractionRule {
                field_id: "contract.party_b".into(),
                aliases: vec![
                    "Исполнитель".into(),
                    "Покупатель".into(),
                    "Арендатор".into(),
                ],
                required: false,
            },
            rule(
                "contract.subject",
                &["Предмет договора", "Предмет контракта"],
            ),
            rule(
                "contract.amount",
                &["Цена договора", "Сумма договора", "Стоимость"],
            ),
            rule("contract.start_date", &["Дата начала", "Начало срока"]),
            rule("contract.end_date", &["Дата окончания", "Действует до"]),
            rule("legal.claim_subject", &["Предмет претензии", "Требование"]),
            rule(
                "legal.claim_amount",
                &["Сумма претензии", "Сумма требования"],
            ),
            rule(
                "counterparty.name",
                &["Контрагент", "Получатель", "Адресат"],
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

pub fn canonical_legal_role(raw: &str) -> String {
    let value = raw.trim().to_lowercase();
    if value.contains("претенз") || value.contains("claim") {
        "claim".into()
    } else if value.contains("сопровод") || value.contains("cover") {
        "cover_letter".into()
    } else if value.contains("акт") || value.contains("acceptance") {
        "acceptance_act".into()
    } else if value.contains("договор") || value.contains("контракт") || value.contains("contract")
    {
        "contract".into()
    } else {
        value
    }
}
