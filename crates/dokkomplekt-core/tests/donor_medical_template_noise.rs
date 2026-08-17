use dokkomplekt_core::parse_source_text;

#[test]
fn inline_template_instruction_is_not_kept_inside_real_diagnosis() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения — сюда подставлять диагноз из шаблона\nЛечение: рисперидон";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(
        case.get("medical.diagnosis"),
        Some("F20.0 Параноидная шизофрения —")
    );
    assert_eq!(case.get("medical.icd10"), Some("F20.0"));
}

#[test]
fn service_lines_are_removed_from_multiline_medical_sections() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nЖалобы:\nТревога, нарушение сна.\nсюда подставлять жалобы пациента\nАнамнез заболевания: состояние ухудшилось неделю назад";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(case.get("medical.complaints"), Some("Тревога, нарушение сна"));
    assert_eq!(
        case.get("medical.anamnesis_disease"),
        Some("состояние ухудшилось неделю назад")
    );
}

#[test]
fn exact_choice_placeholder_is_not_patient_treatment() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nЛечение: нужно / не нужно";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(case.get("medical.treatment"), None);
}

#[test]
fn legitimate_clinical_phrase_with_needed_word_is_preserved() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nДиагноз: F20.0 Параноидная шизофрения\nЛечение: Нужно продолжить приём рисперидона 4 мг/сут";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(
        case.get("medical.treatment"),
        Some("Нужно продолжить приём рисперидона 4 мг/сут")
    );
}

#[test]
fn ui_service_marker_is_not_stored_as_medical_data() {
    let text = "ПЕРВИЧНЫЙ ОСМОТР\nРекомендации: выбирается в UI\nЛечащий врач: Иванов И.И.";
    let (case, _) = parse_source_text(text, 2026);

    assert_eq!(case.get("medical.recommendations"), None);
    assert_eq!(case.get("medical.attending_doctor"), Some("Иванов И.И"));
}
