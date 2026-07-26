//! Сквозное доказательство центрального утверждения 18.4.1:
//! подписанный калиброванный порог разблокирует auto_print для документа,
//! который жёсткий порог реестра блокирует — и НЕ разблокирует ничего
//! сверх того, что должен.
//!
//! Почему этот файл существует. В первых версиях 18.4.1 утверждение
//! «auto_print достигнут» демонстрировалось одноразовым примером
//! (`examples/probe_calibrated.rs`), где порог задавался вручную, а затем
//! пример удалялся. В поставке не оставалось ни одного теста, доказывающего
//! сквозной путь: значение НИЖЕ хардкода, но ВЫШЕ калиброванного порога,
//! проходит только при наличии калибровки. Здесь этот пробел закрыт
//! постоянным тестом.
//!
//! Значения подобраны так, чтобы результат зависел ИМЕННО от калибровки:
//! confidence 0.90 ниже хардкода High (0.98), но выше реального
//! калиброванного порога HR (0.883 из verification/18.4.1/calibration/hr.json).

use dokkomplekt_core::{
    evaluate_print_triage_with_thresholds, CalibratedThresholds, DocumentTemplateSpec, DomainKind,
    SemanticCase, SemanticValue, ValueEvidence, ValueSource,
};
use std::collections::BTreeSet;

fn hr_order_template() -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: "hr.employment_order".into(),
        button_label: "Приказ о приёме".into(),
        template_path: "order.docx".into(),
        category: DomainKind::Hr,
        role_id: "employment_order".into(),
        required_fields: vec![
            "employee.name".into(),
            "hr.order_number".into(),
            "hr.order_date".into(),
            "org.name".into(),
        ],
        placeholders: vec![],
        is_static_copy: false,
        popup_fields: vec![],
        popup_configured: false,
    }
}

/// Дело, где каждое обязательное поле извлечено с уверенностью `confidence`
/// и снабжено доказательством происхождения (иначе high-risk блокируется
/// независимо от порога — что и проверяется отдельным тестом ниже).
fn hr_case_at(confidence: f32) -> SemanticCase {
    let mut case = SemanticCase::default();
    for (field, value) in [
        ("employee.name", "Иванов Иван Иванович"),
        ("hr.order_number", "44"),
        ("hr.order_date", "16.02.2026"),
        ("org.name", "ООО Ромашка"),
    ] {
        case.values.insert(
            field.into(),
            SemanticValue::new(field, value, ValueSource::Scanner, confidence).with_evidence(
                ValueEvidence::new(
                    "document_text",
                    value,
                    "deterministic_source_parser",
                    confidence,
                ),
            ),
        );
    }
    case
}

/// Реальный калиброванный порог HR, полученный настоящим калибратором
/// (verification/18.4.1/calibration/hr.json). Не подогнанное вручную число.
const HR_CALIBRATED_AUTO_MIN: f32 = 0.882_962_5;

fn signed_hr_thresholds() -> CalibratedThresholds {
    CalibratedThresholds {
        auto_min_confidence: HR_CALIBRATED_AUTO_MIN,
        review_min_confidence: 0.85,
        max_auto_error_rate: 0.01,
        // 64 hex-символа: наличие подписанного доказательства калибровки.
        calibration_evidence_sha256: Some("a".repeat(64)),
    }
}

#[test]
fn calibrated_threshold_unblocks_exactly_what_hardcode_blocks() {
    // Уверенность 0.90: ниже хардкода High(0.98), выше калиброванного HR(0.883).
    let case = hr_case_at(0.90);
    let approved = BTreeSet::from(["hr.employment_order".to_string()]);

    // Без калибровки (жёсткий дефолт 0.995, без подписи) — печать запрещена.
    let hardcoded = CalibratedThresholds::default();
    let blocked =
        evaluate_print_triage_with_thresholds(&case, [&hr_order_template()], &approved, &hardcoded);
    assert!(
        !blocked.auto_print_allowed,
        "жёсткий порог не должен пускать 0.90; получено decision={}",
        blocked.decision
    );

    // С подписанной калибровкой 0.883 — та же самая печать разрешена.
    let calibrated = signed_hr_thresholds();
    let allowed = evaluate_print_triage_with_thresholds(
        &case,
        [&hr_order_template()],
        &approved,
        &calibrated,
    );
    assert!(
        allowed.auto_print_allowed,
        "калиброванный 0.883 обязан пускать 0.90; получено decision={}",
        allowed.decision
    );
    assert_eq!(allowed.decision, "auto_print");
}

#[test]
fn calibration_does_not_lower_the_bar_below_its_own_threshold() {
    // Уверенность 0.80 ниже даже калиброванного порога 0.883 — печать запрещена.
    // Калибровка сдвигает границу, но не отменяет её.
    let case = hr_case_at(0.80);
    let approved = BTreeSet::from(["hr.employment_order".to_string()]);
    let report = evaluate_print_triage_with_thresholds(
        &case,
        [&hr_order_template()],
        &approved,
        &signed_hr_thresholds(),
    );
    assert!(
        !report.auto_print_allowed,
        "0.80 ниже калиброванного 0.883 — печать недопустима; decision={}",
        report.decision
    );
}

#[test]
fn unsigned_calibration_cannot_unblock_end_to_end() {
    // Тот же низкий порог 0.883, но БЕЗ подписанного доказательства.
    // Неподписанное число не имеет права понижать планку.
    let case = hr_case_at(0.90);
    let approved = BTreeSet::from(["hr.employment_order".to_string()]);
    let unsigned = CalibratedThresholds {
        auto_min_confidence: HR_CALIBRATED_AUTO_MIN,
        review_min_confidence: 0.85,
        max_auto_error_rate: 0.01,
        calibration_evidence_sha256: None,
    };
    let report =
        evaluate_print_triage_with_thresholds(&case, [&hr_order_template()], &approved, &unsigned);
    assert!(
        !report.auto_print_allowed,
        "неподписанная калибровка не должна разблокировать печать; decision={}",
        report.decision
    );
}

#[test]
fn calibration_never_prints_an_unapproved_template() {
    // Даже при идеальной уверенности и подписанной калибровке шаблон,
    // не утверждённый организацией, не может уйти в печать.
    let case = hr_case_at(0.99);
    let empty_approval = BTreeSet::new();
    let report = evaluate_print_triage_with_thresholds(
        &case,
        [&hr_order_template()],
        &empty_approval,
        &signed_hr_thresholds(),
    );
    assert!(
        !report.auto_print_allowed,
        "неутверждённый шаблон не печатается автоматически; decision={}",
        report.decision
    );
}

#[test]
fn a_critical_field_without_checksum_resists_calibration_end_to_end() {
    // Сквозная проверка страховки Critical: поле amount.total с уверенностью
    // 0.88 (выше калиброванного 0.883? нет — ниже) не должно печататься,
    // а даже при 0.92 сдерживается предохранителем 0.90 из CalibratedFloor.
    let mut case = SemanticCase::default();
    for (field, value, conf) in [
        ("amount.total", "120000", 0.92_f32),
        ("accounting.invoice_number", "148", 0.95),
        ("accounting.invoice_date", "01.02.2026", 0.95),
        ("org.name", "ООО Ромашка", 0.95),
    ] {
        case.values.insert(
            field.into(),
            SemanticValue::new(field, value, ValueSource::Scanner, conf).with_evidence(
                ValueEvidence::new("document_text", value, "deterministic_source_parser", conf),
            ),
        );
    }
    let invoice = DocumentTemplateSpec {
        id: "accounting.invoice".into(),
        button_label: "Счёт".into(),
        template_path: "invoice.docx".into(),
        category: DomainKind::Accounting,
        role_id: "invoice".into(),
        required_fields: vec![
            "amount.total".into(),
            "accounting.invoice_number".into(),
            "accounting.invoice_date".into(),
            "org.name".into(),
        ],
        placeholders: vec![],
        is_static_copy: false,
        popup_fields: vec![],
        popup_configured: false,
    };
    let approved = BTreeSet::from(["accounting.invoice".to_string()]);
    // Калибровка с щедрым порогом 0.50 — Critical всё равно держится на 0.90.
    let generous = CalibratedThresholds {
        auto_min_confidence: 0.50,
        review_min_confidence: 0.40,
        max_auto_error_rate: 0.01,
        calibration_evidence_sha256: Some("a".repeat(64)),
    };
    let report = evaluate_print_triage_with_thresholds(&case, [&invoice], &approved, &generous);
    assert!(
        !report.auto_print_allowed,
        "amount.total(0.92) должен сдерживаться предохранителем Critical(0.90); decision={}",
        report.decision
    );
}

#[test]
fn corrupt_confidence_blocks_auto_print_end_to_end() {
    // Аудит границ вскрыл: уверенность вне [0,1] раньше срезалась clamp
    // до 1.0 и проходила порог. Здесь — сквозная гарантия, что невозможная
    // уверенность в обязательном поле блокирует автопечать даже при щедрой
    // подписанной калибровке.
    let mut case = SemanticCase::default();
    case.values.insert(
        "hr.order_number".into(),
        SemanticValue::new("hr.order_number", "44", ValueSource::Scanner, 2.0).with_evidence(
            ValueEvidence::new("document_text", "44", "deterministic_source_parser", 0.9),
        ),
    );
    for (field, value) in [
        ("employee.name", "Иванов Иван Иванович"),
        ("hr.order_date", "16.02.2026"),
        ("org.name", "ООО Ромашка"),
    ] {
        case.values.insert(
            field.into(),
            SemanticValue::new(field, value, ValueSource::Scanner, 0.99).with_evidence(
                ValueEvidence::new("document_text", value, "deterministic_source_parser", 0.99),
            ),
        );
    }
    let approved = BTreeSet::from(["hr.employment_order".to_string()]);
    let generous = CalibratedThresholds {
        auto_min_confidence: 0.50,
        review_min_confidence: 0.40,
        max_auto_error_rate: 0.01,
        calibration_evidence_sha256: Some("a".repeat(64)),
    };
    let report =
        evaluate_print_triage_with_thresholds(&case, [&hr_order_template()], &approved, &generous);
    assert!(
        !report.auto_print_allowed,
        "невозможная уверенность 2.0 обязана блокировать автопечать; decision={}",
        report.decision
    );
}
