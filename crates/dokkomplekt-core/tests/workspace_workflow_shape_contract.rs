use dokkomplekt_core::{
    analyze_template_text_with_context, best_domain, infer_workspace_workflow_shape,
    predict_document_role, DomainKind, WorkspaceShapeDocumentInput,
};

#[test]
fn specialist_button_label_reuses_canonical_domain_and_role_detection() {
    let analysis = analyze_template_text_with_context(
        "Документ\n{{subject.name}}",
        None,
        Some("Исковое заявление"),
    );
    assert_eq!(analysis.title, "Документ");
    assert_eq!(analysis.role_id, "claim");
    assert_eq!(best_domain(&analysis), DomainKind::Legal);
}

#[test]
fn canonical_role_predictor_is_shared_with_workspace_shape() {
    let (role, confidence) =
        predict_document_role("Трудовой договор\nРаботодатель\nРаботник").expect("role");
    assert_eq!(role, "employment_contract");
    assert!(confidence >= 0.45);
}

#[test]
fn mixed_workspace_stays_split_and_common_fields_are_case_level() {
    let shape = infer_workspace_workflow_shape(&[
        WorkspaceShapeDocumentInput {
            document_id: "claim".into(),
            title: "Иск".into(),
            role_id: "claim".into(),
            domain: DomainKind::Legal,
            field_ids: vec!["subject.name".into(), "document.number".into()],
        },
        WorkspaceShapeDocumentInput {
            document_id: "hire".into(),
            title: "Приказ".into(),
            role_id: "employment_order".into(),
            domain: DomainKind::Hr,
            field_ids: vec!["subject.name".into(), "employee.position".into()],
        },
    ]);
    assert!(shape.mixed_workflows);
    assert_eq!(shape.groups.len(), 2);
    assert!(shape
        .common_fields
        .iter()
        .any(|field| field.field_id == "subject.name"));
    assert!(!shape
        .local_fields
        .get("claim")
        .unwrap()
        .iter()
        .any(|field| field.field_id == "subject.name"));
}
