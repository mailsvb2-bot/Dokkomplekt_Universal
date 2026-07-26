use dokkomplekt_core::*;

#[test]
fn functional_port_creates_dynamic_button_from_template() {
    let doc = create_button_from_template_text(
        "12.01.2026 Выписной эпикриз\nИстория болезни № {{medical.case_number}}",
        "d1",
        "tpl.docx",
        None,
    );
    assert_eq!(doc.button_label, "Выписной эпикриз");
    assert_eq!(doc.role_id, "discharge");
    assert!(doc
        .required_fields
        .contains(&"medical.case_number".to_string()));
}

#[test]
fn functional_port_blocks_unsafe_placeholder_but_keeps_custom_fields() {
    let doc = create_button_from_template_text(
        "Документ\n{{custom.local_note}}\n{{../bad}}",
        "d1",
        "tpl.docx",
        None,
    );
    assert!(doc
        .required_fields
        .contains(&"custom.local_note".to_string()));
    let plan = ported_workflow_plan(&doc, &SemanticCase::default(), false);
    assert!(plan.blocked);
    assert!(plan.block_reasons.iter().any(|x| x.contains("../bad")));
}

#[test]
fn functional_port_diary_schedule_stops_on_discharge_date() {
    let schedule = build_diary_schedule("01.06.2026", "03.06.2026", 2026);
    let dates: Vec<String> = schedule.into_iter().map(|x| x.date).collect();
    assert_eq!(
        dates,
        vec!["02.06.2026".to_string(), "03.06.2026".to_string()]
    );
}

#[test]
fn functional_port_sick_leave_only_for_discharge() {
    let mut case = SemanticCase::default();
    merge_value(
        &mut case,
        SemanticValue::new(
            "medical.case_number",
            "123",
            ValueSource::UserConfirmed,
            1.0,
        ),
    );
    merge_value(
        &mut case,
        SemanticValue::new(
            "medical.diagnosis",
            "F00 тест",
            ValueSource::UserConfirmed,
            1.0,
        ),
    );
    merge_value(
        &mut case,
        SemanticValue::new(
            "medical.treatment",
            "терапия",
            ValueSource::UserConfirmed,
            1.0,
        ),
    );
    let discharge = create_button_from_template_text("Выписной эпикриз", "dis", "d.docx", None);
    let rvk = create_button_from_template_text("АКТ для РВК", "rvk", "r.docx", None);
    let discharge_fields: Vec<String> = ported_workflow_plan(&discharge, &case, true)
        .prompts
        .into_iter()
        .map(|p| p.field_id)
        .collect();
    let rvk_fields: Vec<String> = ported_workflow_plan(&rvk, &case, true)
        .prompts
        .into_iter()
        .map(|p| p.field_id)
        .collect();
    assert!(discharge_fields.contains(&"medical.sick_leave_number".to_string()));
    assert!(!rvk_fields.contains(&"medical.sick_leave_number".to_string()));
}
