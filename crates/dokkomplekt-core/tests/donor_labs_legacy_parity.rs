use dokkomplekt_core::{
    apply_popup_answers, canonical_storage_field_id, plan_workflow, storage_equivalent_field_ids,
    DocumentTemplateSpec, DomainKind, PopupAnswer, PromptInputKind, SemanticCase, WorkflowFlags,
};

fn labs_document() -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: "legacy-labs".into(),
        button_label: "Старый шаблон с анализами".into(),
        template_path: "legacy-labs.docx".into(),
        category: DomainKind::Medical,
        role_id: "medical_generic".into(),
        required_fields: vec!["medical.labs".into()],
        placeholders: vec!["medical.labs".into()],
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    }
}

#[test]
fn predecessor_lab_placeholders_resolve_to_one_canonical_field() {
    for alias in ["laboratory.results", "LAB_BLOCK", "labs_block"] {
        assert_eq!(canonical_storage_field_id(alias), "medical.labs");
        assert!(storage_equivalent_field_ids("medical.labs").contains(&alias));
    }
}

#[test]
fn explicit_without_labs_satisfies_required_labs_in_one_popup() {
    let plan = plan_workflow(
        &labs_document(),
        &SemanticCase::default(),
        &WorkflowFlags::default(),
    );
    assert!(plan
        .prompts
        .iter()
        .any(|prompt| prompt.field_id == "medical.labs"));
    let without = plan
        .prompts
        .iter()
        .find(|prompt| prompt.field_id == "medical.labs_without")
        .expect("medical labs template must expose explicit no-labs choice");
    assert_eq!(without.input_kind, PromptInputKind::YesNo);
    let result = apply_popup_answers(
        &SemanticCase::default(),
        &plan,
        &[PopupAnswer {
            field_id: "medical.labs_without".into(),
            value: "Да".into(),
            continue_without_value: false,
        }],
    );
    assert!(result.accepted, "{:#?}", result.errors);
    assert_eq!(
        result.semantic_case.get("medical.labs"),
        Some("Нет анализов")
    );
    assert_eq!(result.semantic_case.get("medical.labs_without"), Some("Да"));
}

#[test]
fn declining_without_labs_keeps_required_labs_missing() {
    let plan = plan_workflow(
        &labs_document(),
        &SemanticCase::default(),
        &WorkflowFlags::default(),
    );
    let result = apply_popup_answers(
        &SemanticCase::default(),
        &plan,
        &[PopupAnswer {
            field_id: "medical.labs_without".into(),
            value: "Нет".into(),
            continue_without_value: false,
        }],
    );
    assert!(!result.accepted);
    assert!(result
        .still_missing
        .iter()
        .any(|prompt| prompt.field_id == "medical.labs"));
}

#[test]
fn existing_labs_do_not_trigger_redundant_without_labs_question() {
    let mut case = SemanticCase::default();
    dokkomplekt_core::set_user_value(&mut case, "medical.labs", "ОАК: без отклонений");
    let plan = plan_workflow(&labs_document(), &case, &WorkflowFlags::default());
    assert!(!plan
        .prompts
        .iter()
        .any(|prompt| prompt.field_id == "medical.labs_without"));
}
