use crate::{
    canonical_storage_field_id, storage_equivalent_field_ids, SemanticCase, SemanticValue,
    ValueSource,
};

/// Inserts a value using the preserved product priority:
/// user-confirmed UI/popup > explicit current-session selection > scanner > safe default.
/// Historical ids that mean exactly the same thing are migrated into one canonical key.
pub fn merge_value(case: &mut SemanticCase, mut incoming: SemanticValue) -> bool {
    let key = canonical_storage_field_id(&incoming.field_id);
    incoming.field_id = key.clone();

    let equivalent = storage_equivalent_field_ids(&key);
    let mut existing = equivalent
        .iter()
        .filter_map(|candidate| case.values.get(*candidate))
        .cloned()
        .collect::<Vec<_>>();
    if equivalent.is_empty() {
        if let Some(value) = case.values.get(&key).cloned() {
            existing.push(value);
        }
    }
    existing.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.confidence.total_cmp(&right.confidence))
    });
    let current = existing.last();
    let should_replace = current.is_none_or(|value| {
        incoming.source > value.source
            || (incoming.source == value.source && incoming.confidence >= value.confidence)
    });

    if !should_replace {
        // Opportunistically migrate an old persisted alias without changing its value.
        if !case.values.contains_key(&key) {
            if let Some(mut value) = current.cloned() {
                value.field_id = key.clone();
                case.values.insert(key.clone(), value);
            }
        }
        for alias in equivalent {
            if *alias != key.as_str() {
                case.values.remove(*alias);
            }
        }
        return false;
    }

    for alias in equivalent {
        case.values.remove(*alias);
    }
    case.values.insert(key, incoming);
    true
}

/// Normalize every persisted value after loading an older project state.
pub fn normalize_semantic_case_aliases(case: &mut SemanticCase) {
    let values = std::mem::take(&mut case.values);
    for (_, value) in values {
        merge_value(case, value);
    }
}

pub fn set_user_value(
    case: &mut SemanticCase,
    field_id: impl Into<String>,
    value: impl Into<String>,
) {
    merge_value(
        case,
        SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
    );
}

pub fn set_scanner_value(
    case: &mut SemanticCase,
    field_id: impl Into<String>,
    value: impl Into<String>,
    confidence: f32,
) {
    merge_value(
        case,
        SemanticValue::new(field_id, value, ValueSource::Scanner, confidence),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_alias_is_migrated_and_visible_through_both_ids() {
        let mut case = SemanticCase::default();
        set_scanner_value(&mut case, "medical.diagnosis_code", "J45.0", 0.9);
        assert_eq!(case.get("medical.icd10"), Some("J45.0"));
        assert_eq!(case.get("medical.diagnosis_code"), Some("J45.0"));
        assert!(case.values.contains_key("medical.icd10"));
        assert!(!case.values.contains_key("medical.diagnosis_code"));
    }

    #[test]
    fn higher_priority_alias_value_wins_during_migration() {
        let mut case = SemanticCase::default();
        set_scanner_value(&mut case, "organization.name", "ООО Старое", 0.95);
        set_user_value(&mut case, "org.name", "ООО Подтверждённое");
        assert_eq!(case.get("organization.name"), Some("ООО Подтверждённое"));
        assert_eq!(case.values.len(), 1);
    }
}
