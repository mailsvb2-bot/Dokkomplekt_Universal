use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedValueSource {
    Default,
    Template,
    SourceDocument,
    Scanner,
    Session,
    User,
}

impl UnifiedValueSource {
    fn rank(&self) -> u8 {
        match self {
            Self::Default => 10,
            Self::Template => 15,
            Self::SourceDocument => 20,
            Self::Scanner => 30,
            Self::Session => 40,
            Self::User => 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedFieldValue {
    pub field_id: String,
    pub value: String,
    pub source: UnifiedValueSource,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedConflict {
    pub field_id: String,
    pub current: String,
    pub incoming: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UnifiedDataSchema {
    pub values: BTreeMap<String, UnifiedFieldValue>,
    pub conflicts: Vec<UnifiedConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedFieldKind {
    Text,
    Date,
    Number,
    Money,
    LongText,
    Choice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedFieldDefinition {
    pub id: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub kind: UnifiedFieldKind,
}

pub fn is_safe_field_id(field_id: &str) -> bool {
    let trimmed = field_id.trim();
    if trimmed.is_empty()
        || trimmed.len() > 96
        || trimmed.starts_with('.')
        || trimmed.ends_with('.')
        || trimmed.contains("..")
    {
        return false;
    }
    trimmed.split('.').all(|part| {
        !part.is_empty()
            && part.len() <= 40
            && part.chars().next().is_some_and(|c| c.is_alphabetic())
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    })
}

pub fn normalize_field_id(raw: &str) -> String {
    let key = raw.trim().to_lowercase().replace(' ', "_");
    let normalized = match key.as_str() {
        "фио" | "пациент" | "клиент" => "subject.name",
        "сотрудник" => "employee.name",
        "дата_рождения" => "subject.birth_date",
        "адрес" => "subject.address",
        "номер_договора" => "contract.number",
        "дата_договора" => "contract.date",
        "заказчик" => "contract.party_a",
        "исполнитель" => "contract.party_b",
        "должность" => "employee.position",
        "подразделение" => "employee.department",
        "сумма" | "итого" => "amount.total",
        "валюта" => "amount.currency",
        "инн" => "org.inn",
        "кпп" => "org.kpp",
        "код_мкб" | "мкб-10" | "мкб_10" => "medical.icd10",
        _ => raw.trim(),
    };
    crate::canonical_storage_field_id(normalized)
}

pub fn set_unified_value(
    schema: &mut UnifiedDataSchema,
    field_id: &str,
    value: &str,
    source: UnifiedValueSource,
    confidence: f32,
) {
    let canonical = normalize_field_id(field_id);
    let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() || !is_safe_field_id(&canonical) {
        return;
    }
    if let Some(current) = schema.values.get(&canonical) {
        if current.value != clean && source.rank() >= current.source.rank() {
            schema.conflicts.push(UnifiedConflict {
                field_id: canonical.clone(),
                current: current.value.clone(),
                incoming: clean.clone(),
                message: format!(
                    "Поле {canonical}: есть «{}», новое значение «{clean}». Нужно подтверждение.",
                    current.value
                ),
            });
        }
        if source.rank() < current.source.rank()
            || (source.rank() == current.source.rank() && confidence < current.confidence)
        {
            return;
        }
    }
    schema.values.insert(
        canonical.clone(),
        UnifiedFieldValue {
            field_id: canonical,
            value: clean,
            source,
            confidence,
        },
    );
}
