use dokkomplekt_core::{
    plan_workflow, DocumentTemplateSpec, DomainKind, PromptAskMode, SemanticCase, WorkflowFlags,
    DIARY_INTRADAY_RHYTHM, DIARY_SCHEDULE_STYLE,
};

#[test]
fn customized_diary_popup_cannot_hide_donor_schedule_confirmation() {
    let document = DocumentTemplateSpec {
        id: "medical-diaries".into(),
        button_label: "Дневники наблюдения".into(),
        template_path: "diaries.docx".into(),
        category: DomainKind::Medical,
        role_id: "diaries".into(),
        required_fields: Vec::new(),
        placeholders: Vec::new(),
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: true,
    };

    let plan = plan_workflow(
        &document,
        &SemanticCase::default(),
        &WorkflowFlags::default(),
    );
    for field_id in [DIARY_SCHEDULE_STYLE, DIARY_INTRADAY_RHYTHM] {
        let prompt = plan
            .prompts
            .iter()
            .find(|prompt| prompt.field_id == field_id)
            .unwrap_or_else(|| panic!("missing donor diary runtime prompt: {field_id}"));
        assert!(
            prompt.required,
            "{field_id} must be confirmed before generation"
        );
        assert_eq!(prompt.ask_mode, PromptAskMode::Always);
        assert!(
            prompt.current_value.is_none(),
            "{field_id} must not silently default"
        );
    }

    let style = plan
        .prompts
        .iter()
        .find(|prompt| prompt.field_id == DIARY_SCHEDULE_STYLE)
        .unwrap();
    assert!(style
        .options
        .iter()
        .any(|option| option.contains("1, 2, 3, 7") && option.contains("2 раза в неделю")));
    assert!(style.allow_custom_option);
}
