use crate::{merge_value, SemanticCase, SemanticValue, ValueEvidence, ValueSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScannerMark {
    pub field_id: String,
    pub selected_text: String,
    pub page_index: usize,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScannerApplyReport {
    pub applied_fields: Vec<String>,
    pub rejected_fields: Vec<String>,
}

/// Universal "scanner": user can mark text fragments in a document and map them to semantic fields.
pub fn apply_scanner_marks(case: &mut SemanticCase, marks: &[ScannerMark]) -> ScannerApplyReport {
    let mut report = ScannerApplyReport {
        applied_fields: vec![],
        rejected_fields: vec![],
    };
    for mark in marks {
        if mark.field_id.trim().is_empty() || mark.selected_text.trim().is_empty() {
            report.rejected_fields.push(mark.field_id.clone());
            continue;
        }
        let applied = merge_value(
            case,
            SemanticValue::new(
                &mark.field_id,
                mark.selected_text.trim(),
                ValueSource::Scanner,
                mark.confidence.clamp(0.0, 1.0),
            )
            .with_evidence(ValueEvidence {
                source_kind: "scanner_selection".into(),
                source_reference: None,
                excerpt: mark.selected_text.trim().to_string(),
                page_index: Some(mark.page_index),
                extractor: "guided_scanner".into(),
                confidence: mark.confidence.clamp(0.0, 1.0),
            }),
        );
        if applied {
            report.applied_fields.push(mark.field_id.clone());
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set_user_value;

    #[test]
    fn user_value_wins_over_scanner_mark() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "medical.diagnosis", "Диагноз из popup");
        let report = apply_scanner_marks(
            &mut case,
            &[ScannerMark {
                field_id: "medical.diagnosis".into(),
                selected_text: "Диагноз из сканера".into(),
                page_index: 0,
                confidence: 0.9,
            }],
        );
        assert!(report.applied_fields.is_empty());
        assert_eq!(case.get("medical.diagnosis"), Some("Диагноз из popup"));
    }
}
