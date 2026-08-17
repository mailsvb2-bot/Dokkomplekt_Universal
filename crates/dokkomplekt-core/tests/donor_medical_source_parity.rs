use dokkomplekt_core::{canonical_field_id_for_domain, parse_source_text, DomainKind};

#[test]
fn compact_primary_populates_donor_clinical_fields_without_bleed() {
    let text = "Первичный осмотр 05.06.2026 История болезни № 12345 Ф.И.О.: Иванов Иван Иванович Возраст: 45 лет Место жительства: г. Нижний Новгород, ул. Пушкина, 1 Место работы: ООО Ромашка Должность: инженер Жалобы: головная боль Анамнез заболевания: заболел вчера Анамнез жизни: рос и развивался нормально Соматический статус: стабилен Профильный статус: ориентирован План обследования: ОАК Лечение: режим, терапия Диагноз: J20 Острый бронхит";
    let (case, _) = parse_source_text(text, 2026);
    for (field, expected) in [
        ("medical.admission_date", "05.06.2026"),
        ("medical.case_number", "12345"),
        ("subject.name", "Иванов Иван Иванович"),
        ("subject.age", "45 лет"),
        ("medical.workplace", "ООО Ромашка"),
        ("medical.position", "инженер"),
        ("medical.complaints", "головная боль"),
        ("medical.anamnesis_disease", "заболел вчера"),
        ("medical.anamnesis_life", "рос и развивался нормально"),
        ("medical.somatic_status", "стабилен"),
        ("medical.profile_status", "ориентирован"),
        ("medical.examination_plan", "ОАК"),
        ("medical.treatment", "режим, терапия"),
        ("medical.diagnosis", "J20 Острый бронхит"),
    ] {
        assert_eq!(case.get(field), Some(expected), "field {field}");
    }
}

#[test]
fn donor_expansion_preserves_preexisting_russian_medical_aliases() {
    let text = "История болезни № 41\nПоступил: 01.06.2026\nВыписан: 10.06.2026\nРаботает: ООО Ромашка\nв должности: инженер\nДиагноз: J20 Острый бронхит";
    let (case, _) = parse_source_text(text, 2026);
    assert_eq!(case.get("medical.admission_date"), Some("01.06.2026"));
    assert_eq!(case.get("medical.discharge_date"), Some("10.06.2026"));
    assert_eq!(case.get("medical.workplace"), Some("ООО Ромашка"));
    assert_eq!(case.get("medical.position"), Some("инженер"));
}

#[test]
fn historical_medical_placeholders_resolve_to_current_schema() {
    let medical = Some(&DomainKind::Medical);
    for (legacy, canonical) in [
        ("patient.age", "subject.age"),
        ("complaints", "medical.complaints"),
        ("anamnesis.disease", "medical.anamnesis_disease"),
        ("anamnesis.life", "medical.anamnesis_life"),
        ("status.objective", "medical.somatic_status"),
        ("status.profile", "medical.profile_status"),
        ("examination.plan", "medical.examination_plan"),
        ("treatment.result", "medical.treatment_result"),
        ("condition.discharge", "medical.discharge_condition"),
        ("labs.results", "medical.labs"),
        ("labs.date", "medical.labs_date"),
    ] {
        assert_eq!(
            canonical_field_id_for_domain(legacy, medical).as_deref(),
            Some(canonical),
            "alias {legacy}"
        );
    }
}

#[test]
fn labs_are_explicit_and_never_synthesized() {
    let with_labs = "ПЕРВИЧНЫЙ ОСМОТР
История болезни № 77
Лабораторные исследования:
ОАК от 01.06.2026: Hb 140 г/л
Диагноз: J20 Острый бронхит";
    let (case, _) = parse_source_text(with_labs, 2026);
    assert_eq!(
        case.get("medical.labs"),
        Some("ОАК от 01.06.2026: Hb 140 г/л")
    );

    let without_labs = "ПЕРВИЧНЫЙ ОСМОТР
История болезни № 78
Диагноз: J20 Острый бронхит
Лечение: режим";
    let (case, _) = parse_source_text(without_labs, 2026);
    assert_eq!(case.get("medical.labs"), None);
    assert_eq!(case.get("medical.labs_date"), None);
}
