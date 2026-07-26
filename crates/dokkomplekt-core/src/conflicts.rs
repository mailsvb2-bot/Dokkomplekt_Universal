use crate::{SemanticCase, SemanticValue};

#[derive(Debug, Clone, PartialEq)]
pub struct FieldConflict {
    pub field_id: String,
    pub existing_value: String,
    pub incoming_value: String,
    pub message: String,
}

/// Detects conflicts before overwriting a value. UI must show this instead of silently duplicating dates.
pub fn detect_field_conflict(
    case: &SemanticCase,
    incoming: &SemanticValue,
) -> Option<FieldConflict> {
    let existing = case.values.get(&incoming.field_id)?;
    let left = existing.value.trim();
    let right = incoming.value.trim();
    if left.is_empty() || right.is_empty() || left == right {
        return None;
    }
    Some(FieldConflict {
        field_id: incoming.field_id.clone(),
        existing_value: left.to_string(),
        incoming_value: right.to_string(),
        message: format!("Поле '{}' уже имеет значение '{}', новое значение: '{}'. Нужно подтверждение пользователя.", incoming.field_id, left, right),
    })
}

pub fn date_fields_that_need_confirmation(case: &SemanticCase) -> Vec<String> {
    let admission = case.get("medical.admission_date");
    let discharge = case.get("medical.discharge_date");
    match (admission, discharge) {
        (Some(a), Some(d)) if a == d => vec![
            "medical.admission_date".into(),
            "medical.discharge_date".into(),
        ],
        _ => vec![],
    }
}
