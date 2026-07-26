//! Confidence-aware auto-print gate.
//!
//! Generation and printing are deliberately separate decisions. A document may be
//! rendered for review while automatic printing stays blocked until every material
//! field is grounded, confident and the exact template revision is approved by the
//! organisation. Manual printing remains an explicit specialist action.

use crate::{
    canonical_storage_field_id, evaluate_automation_quality_with_floor, AutomationBlocker,
    CalibratedFloor, DocumentTemplateSpec, FieldRisk, SemanticCase, ValueSource,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintBucket {
    AutoPrint,
    ReviewFields,
    HoldForReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibratedThresholds {
    pub auto_min_confidence: f32,
    pub review_min_confidence: f32,
    pub max_auto_error_rate: f32,
    #[serde(default)]
    pub calibration_evidence_sha256: Option<String>,
}

impl Default for CalibratedThresholds {
    fn default() -> Self {
        Self {
            // Conservative bootstrap values. Production autonomy should replace
            // them with a signed corpus calibration, never with a lower ad-hoc UI value.
            auto_min_confidence: 0.995,
            review_min_confidence: 0.85,
            max_auto_error_rate: 0.005,
            calibration_evidence_sha256: None,
        }
    }
}

impl CalibratedThresholds {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.auto_min_confidence)
            || !(0.0..=1.0).contains(&self.review_min_confidence)
            || !(0.0..=1.0).contains(&self.max_auto_error_rate)
            || self.review_min_confidence > self.auto_min_confidence
        {
            return Err("Некорректные пороги confidence-триажа.".into());
        }
        if let Some(evidence) = self.calibration_evidence_sha256.as_deref() {
            if evidence.len() != 64 || !evidence.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("Некорректный SHA-256 доказательства калибровки.".into());
            }
        }
        Ok(())
    }

    pub fn has_calibration_evidence(&self) -> bool {
        self.calibration_evidence_sha256
            .as_deref()
            .is_some_and(|value| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintFieldDiff {
    pub field_id: String,
    pub value: String,
    pub source: String,
    pub confidence: f32,
    pub risk: FieldRisk,
    pub evidence: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintTriageReport {
    pub decision: String,
    pub auto_print_allowed: bool,
    pub confidence_score: f32,
    pub checked_document_ids: Vec<String>,
    pub unapproved_document_ids: Vec<String>,
    pub missing_fields: Vec<String>,
    pub blockers: Vec<AutomationBlocker>,
    pub diff: Vec<PrintFieldDiff>,
    pub reasons: Vec<String>,
}

pub fn evaluate_print_triage<'a>(
    case: &SemanticCase,
    documents: impl IntoIterator<Item = &'a DocumentTemplateSpec>,
    approved_document_ids: &BTreeSet<String>,
) -> PrintTriageReport {
    evaluate_print_triage_with_thresholds(
        case,
        documents,
        approved_document_ids,
        &CalibratedThresholds::default(),
    )
}

pub fn evaluate_print_triage_with_thresholds<'a>(
    case: &SemanticCase,
    documents: impl IntoIterator<Item = &'a DocumentTemplateSpec>,
    approved_document_ids: &BTreeSet<String>,
    thresholds: &CalibratedThresholds,
) -> PrintTriageReport {
    let thresholds_valid = thresholds.validate().is_ok();
    let documents = documents.into_iter().collect::<Vec<_>>();
    let checked_document_ids = documents
        .iter()
        .map(|document| document.id.clone())
        .collect::<Vec<_>>();
    let unapproved_document_ids = documents
        .iter()
        .filter(|document| !approved_document_ids.contains(&document.id))
        .map(|document| document.id.clone())
        .collect::<Vec<_>>();

    let mut required = BTreeSet::<String>::new();
    for document in &documents {
        for field_id in document
            .required_fields
            .iter()
            .chain(document.placeholders.iter())
        {
            let canonical = canonical_storage_field_id(field_id);
            if !canonical.trim().is_empty() {
                required.insert(canonical);
            }
        }
    }

    let missing_fields = required
        .iter()
        .filter(|field_id| {
            case.value(field_id)
                .map(|value| value.value.trim().is_empty())
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    // Калиброванный порог обязан дойти до внутреннего гейта качества.
    //
    // Раньше здесь стоял безусловный `evaluate_automation_quality`, бравший
    // жёсткие пороги реестра. Подписанная калибровка гейтила только итоговый
    // агрегат ниже, но внутренний хардкод уже успевал наложить блокеры,
    // и калибровка не могла вступить в силу. Теперь порог передаётся внутрь;
    // безопасность сохранена: калибровка применяется лишь при наличии
    // подписанного доказательства (`has_calibration_evidence`).
    let floor = CalibratedFloor {
        auto_min_confidence: Some(thresholds.auto_min_confidence),
        evidence_backed: thresholds_valid && thresholds.has_calibration_evidence(),
    };
    let quality =
        evaluate_automation_quality_with_floor(case, required.iter().map(String::as_str), floor);

    let blocker_by_field = quality
        .blockers
        .iter()
        .map(|blocker| (blocker.field_id.clone(), blocker.reason.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut diff = required
        .iter()
        .filter_map(|field_id| {
            let value = case.value(field_id)?;
            let evidence = value
                .evidence
                .iter()
                .map(|item| {
                    if let Some(reference) = item.source_reference.as_deref() {
                        format!("{}: {} ({reference})", item.extractor, item.excerpt)
                    } else {
                        format!("{}: {}", item.extractor, item.excerpt)
                    }
                })
                .collect::<Vec<_>>();
            Some(PrintFieldDiff {
                field_id: field_id.clone(),
                value: value.value.clone(),
                source: value_source_name(value.source).into(),
                // Показываем ИСТИННУЮ уверенность, не срезанную в [0,1]:
                // если восходящий код сообщил невозможное значение (2.0, +inf),
                // проверяющий человек обязан это видеть, а не получить
                // причёсанную 1.0. Решение о блокировке уже принято выше
                // в value_blocker — здесь только отображение.
                confidence: value.confidence,
                risk: crate::field_risk(field_id),
                evidence,
                status: blocker_by_field
                    .get(field_id)
                    .cloned()
                    .unwrap_or_else(|| "готово к автопечати".into()),
            })
        })
        .collect::<Vec<_>>();
    diff.sort_by(|left, right| {
        right
            .risk
            .cmp(&left.risk)
            .then_with(|| left.field_id.cmp(&right.field_id))
    });

    let confidence_score = if diff.is_empty() {
        0.0
    } else {
        diff.iter()
            .map(|item| item.confidence)
            .fold(1.0_f32, f32::min)
    };
    let structural_ready =
        !documents.is_empty() && unapproved_document_ids.is_empty() && missing_fields.is_empty();
    let bucket = if !thresholds_valid || !structural_ready {
        PrintBucket::HoldForReview
    } else if quality.ready
        && thresholds.has_calibration_evidence()
        && confidence_score >= thresholds.auto_min_confidence
    {
        PrintBucket::AutoPrint
    } else if confidence_score >= thresholds.review_min_confidence {
        PrintBucket::ReviewFields
    } else {
        PrintBucket::HoldForReview
    };
    let auto_print_allowed = bucket == PrintBucket::AutoPrint;
    let mut reasons = Vec::new();
    if !thresholds_valid {
        reasons.push(
            "Подписанные пороги confidence повреждены или некорректны; автопечать запрещена."
                .into(),
        );
    } else if !thresholds.has_calibration_evidence() {
        reasons.push("Нет подписанного доказательства калибровки на корпусе; автопечать запрещена, доступен только review.".into());
    }
    if documents.is_empty() {
        reasons.push("Не передан ни один документ для проверки автопечати.".into());
    }
    if !unapproved_document_ids.is_empty() {
        reasons.push(format!(
            "Точная ревизия шаблона не утверждена организацией: {}.",
            unapproved_document_ids.join(", ")
        ));
    }
    if !missing_fields.is_empty() {
        reasons.push(format!(
            "В комплекте отсутствуют обязательные значения: {}.",
            missing_fields.join(", ")
        ));
    }
    if !quality.ready {
        reasons.push(format!(
            "Проверка уверенности обнаружила блокирующих полей: {}.",
            quality.blockers.len()
        ));
    }
    match bucket {
        PrintBucket::AutoPrint => reasons.push(
            "Все обязательные значения проверяемы, выше калиброванного порога и шаблоны утверждены."
                .into(),
        ),
        PrintBucket::ReviewFields => reasons.push(
            "Комплект собран, но перед печатью требуется обязательный diff-review полей."
                .into(),
        ),
        PrintBucket::HoldForReview => reasons.push(
            "Комплект удержан в очереди ревью; автоматическая печать не выполнялась.".into(),
        ),
    }

    PrintTriageReport {
        decision: match bucket {
            PrintBucket::AutoPrint => "auto_print",
            PrintBucket::ReviewFields => "review_fields",
            PrintBucket::HoldForReview => "hold_for_review",
        }
        .into(),
        auto_print_allowed,
        confidence_score,
        checked_document_ids,
        unapproved_document_ids,
        missing_fields,
        blockers: quality.blockers,
        diff,
        reasons,
    }
}

fn value_source_name(source: ValueSource) -> &'static str {
    match source {
        ValueSource::SafeDefault => "safe_default",
        ValueSource::Model => "local_model",
        ValueSource::Scanner => "scanner",
        ValueSource::SessionSelection => "session_selection",
        ValueSource::UserConfirmed => "user_confirmed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DomainKind, SemanticValue, ValueEvidence};

    fn document() -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            template_path: "invoice.docx".into(),
            category: DomainKind::Accounting,
            role_id: "invoice".into(),
            required_fields: vec!["amount.total".into()],
            placeholders: Vec::new(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn rendered_document_is_review_only_until_template_revision_is_approved() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "amount.total".into(),
            SemanticValue::new("amount.total", "1000.00", ValueSource::UserConfirmed, 1.0),
        );
        let report = evaluate_print_triage(&case, [&document()], &BTreeSet::new());
        assert!(!report.auto_print_allowed);
        assert_eq!(report.decision, "hold_for_review");
        assert_eq!(report.unapproved_document_ids, vec!["invoice"]);
    }

    #[test]
    fn approved_grounded_high_confidence_document_can_auto_print() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "amount.total".into(),
            SemanticValue::new("amount.total", "1000.00", ValueSource::Scanner, 0.999)
                .with_evidence(ValueEvidence::new(
                    "document_text",
                    "Итого: 1000.00",
                    "label_parser",
                    0.999,
                )),
        );
        let approved = BTreeSet::from(["invoice".into()]);
        let thresholds = CalibratedThresholds {
            auto_min_confidence: 0.995,
            review_min_confidence: 0.85,
            max_auto_error_rate: 0.005,
            calibration_evidence_sha256: Some("a".repeat(64)),
        };
        let report =
            evaluate_print_triage_with_thresholds(&case, [&document()], &approved, &thresholds);
        assert!(report.auto_print_allowed);
        assert_eq!(report.decision, "auto_print");
    }

    #[test]
    fn unsigned_bootstrap_threshold_never_auto_prints() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "amount.total".into(),
            SemanticValue::new("amount.total", "1000.00", ValueSource::UserConfirmed, 1.0),
        );
        let approved = BTreeSet::from(["invoice".into()]);
        let report = evaluate_print_triage(&case, [&document()], &approved);
        assert!(!report.auto_print_allowed);
        assert_eq!(report.decision, "review_fields");
        assert!(report
            .reasons
            .iter()
            .any(|reason| reason.contains("калибров")));
    }

    #[test]
    fn medium_confidence_uses_mandatory_review_bucket() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "amount.total".into(),
            SemanticValue::new("amount.total", "1000.00", ValueSource::Model, 0.92).with_evidence(
                ValueEvidence::new("document_text", "Итого: 1000.00", "model-grounding", 0.92),
            ),
        );
        let approved = BTreeSet::from(["invoice".into()]);
        let thresholds = CalibratedThresholds {
            auto_min_confidence: 0.99,
            review_min_confidence: 0.90,
            max_auto_error_rate: 0.005,
            calibration_evidence_sha256: Some("a".repeat(64)),
        };
        let report =
            evaluate_print_triage_with_thresholds(&case, [&document()], &approved, &thresholds);
        assert!(!report.auto_print_allowed);
        assert_eq!(report.decision, "review_fields");
    }

    #[test]
    fn low_confidence_value_produces_review_diff() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "amount.total".into(),
            SemanticValue::new("amount.total", "1000.00", ValueSource::Model, 0.90),
        );
        let approved = BTreeSet::from(["invoice".into()]);
        let report = evaluate_print_triage(&case, [&document()], &approved);
        assert!(!report.auto_print_allowed);
        assert_eq!(report.diff.len(), 1);
        assert_ne!(report.diff[0].status, "готово к автопечати");
    }
}
