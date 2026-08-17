use dokkomplekt_core::domains::medical_semantics::{
    case_for_medical_document_render, set_medical_sick_leave_choice,
    MEDICAL_EXPERT_ANAMNESIS, MEDICAL_SICK_LEAVE_NEEDED,
};
use dokkomplekt_core::{
    build_medical_render_plan, medical_fields, MedicalDocumentRole, SemanticCase, SemanticValue,
    ValueSource,
};

fn put(case: &mut SemanticCase, field_id: &str, value: &str) {
    case.values.insert(
        field_id.to_string(),
        SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
    );
}

#[test]
fn primary_expert_anamnesis_is_short_and_has_no_sick_leave_details() {
    let mut case = SemanticCase::default();
    put(&mut case, "medical.workplace", "ООО Ромашка");
    put(&mut case, "medical.position", "инженер");
    put(&mut case, "medical.sick_leave_number", "123456789");
    put(&mut case, "medical.admission_date", "10.05.2026");
    put(&mut case, "medical.discharge_date", "12.05.2026");
    set_medical_sick_leave_choice(&mut case, true);

    let render = case_for_medical_document_render(&case, "primary");
    assert_eq!(
        render.get(MEDICAL_EXPERT_ANAMNESIS),
        Some("Работает в ООО Ромашка, в должности инженер.")
    );
}

#[test]
fn discharge_expert_anamnesis_contains_number_inclusive_days_and_return_to_work() {
    let mut case = SemanticCase::default();
    put(&mut case, "medical.workplace", "ООО Ромашка");
    put(&mut case, "medical.position", "инженер");
    put(&mut case, "medical.sick_leave_number", "123456789");
    put(&mut case, "medical.admission_date", "10.05.2026");
    put(&mut case, "medical.discharge_date", "12.05.2026");
    set_medical_sick_leave_choice(&mut case, true);

    let render = case_for_medical_document_render(&case, "discharge");
    assert_eq!(
        render.get(MEDICAL_EXPERT_ANAMNESIS),
        Some(
            "Работает в ООО Ромашка, в должности инженер. Больничный лист № 123456789. Срок лечения с 10.05.2026 по 12.05.2026, 3 дня. К труду с 13.05.2026."
        )
    );
}

#[test]
fn discharge_expert_anamnesis_uses_correct_russian_day_word_for_eleven_days() {
    let mut case = SemanticCase::default();
    put(&mut case, "medical.admission_date", "01.05.2026");
    put(&mut case, "medical.discharge_date", "11.05.2026");
    put(&mut case, "medical.sick_leave_number", "77");
    set_medical_sick_leave_choice(&mut case, true);

    let render = case_for_medical_document_render(&case, "discharge");
    let expert = render
        .get(MEDICAL_EXPERT_ANAMNESIS)
        .expect("expert anamnesis must be derived");
    assert!(expert.contains("11 дней"));
    assert!(expert.contains("К труду с 12.05.2026"));
}

#[test]
fn discharge_expert_anamnesis_records_explicit_no_sick_leave_choice() {
    let mut case = SemanticCase::default();
    put(&mut case, "medical.workplace", "ГБОУ Школа № 1");
    put(&mut case, "medical.position", "учитель");
    set_medical_sick_leave_choice(&mut case, false);

    let render = case_for_medical_document_render(&case, "discharge");
    assert_eq!(
        render.get(MEDICAL_EXPERT_ANAMNESIS),
        Some("Работает в ГБОУ Школа № 1, в должности учитель. В выдаче ЛН не нуждается.")
    );
    assert_eq!(case.get(MEDICAL_SICK_LEAVE_NEEDED), Some("Нет"));
}

#[test]
fn sick_leave_number_is_a_safe_yes_fallback_for_migrated_cases() {
    let mut case = SemanticCase::default();
    put(&mut case, "medical.sick_leave_number", "42");
    put(&mut case, "medical.admission_date", "01.06.2026");
    put(&mut case, "medical.discharge_date", "01.06.2026");

    let render = case_for_medical_document_render(&case, "discharge");
    let expert = render
        .get(MEDICAL_EXPERT_ANAMNESIS)
        .expect("legacy sick-leave number must produce expert text");
    assert!(expert.starts_with("Больничный лист № 42."));
    assert!(expert.contains("1 день"));
    assert!(expert.contains("К труду с 02.06.2026"));
}

#[test]
fn primary_and_discharge_request_workplace_and_position_for_expert_section() {
    for role in [
        MedicalDocumentRole::PrimaryInspection,
        MedicalDocumentRole::DischargeEpicrisis,
    ] {
        let plan = build_medical_render_plan(role, false, true);
        assert!(plan.required_fields.contains(&"medical.workplace".into()));
        assert!(plan.required_fields.contains(&"medical.position".into()));
        assert!(!plan.optional_fields.contains(&"medical.workplace".into()));
        assert!(!plan.optional_fields.contains(&"medical.position".into()));
    }
}

#[test]
fn expert_fields_are_registered_in_medical_profile() {
    let fields = medical_fields();
    for field_id in [MEDICAL_EXPERT_ANAMNESIS, MEDICAL_SICK_LEAVE_NEEDED] {
        assert!(
            fields.iter().any(|field| field.id == field_id),
            "missing medical field {field_id}"
        );
    }
}
