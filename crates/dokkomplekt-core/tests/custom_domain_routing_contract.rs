use dokkomplekt_core::core::{SourceDocument, TargetTemplate};
use dokkomplekt_core::{
    plan_workflow, DocumentTemplateSpec, DomainKind, SemanticCase, UniversalDomain,
    UniversalPipelineFlags, UniversalPipelineInput, WorkflowFlags,
};

#[test]
fn custom_profession_stays_custom_in_universal_pipeline() {
    let input = UniversalPipelineInput {
        source_document: SourceDocument {
            id: "case".into(),
            text: "Проект: Север\nОтветственный: Иванов".into(),
            metadata: Default::default(),
        },
        target_template: TargetTemplate {
            id: "architecture_report".into(),
            path: "architecture_report.docx".into(),
            text: "Отчёт архитектора\n{{custom.project}}\n{{custom.responsible}}".into(),
        },
        domain_hint: Some(UniversalDomain::Custom),
        flags: UniversalPipelineFlags::default(),
    };

    let result = dokkomplekt_core::run_universal_constructor_pipeline(input);
    assert_eq!(result.domain, UniversalDomain::Custom);
}

#[test]
fn custom_document_workflow_uses_only_its_declared_fields() {
    let document = DocumentTemplateSpec {
        id: "architecture_report".into(),
        button_label: "Отчёт архитектора".into(),
        template_path: "architecture_report.docx".into(),
        category: DomainKind::Custom("architecture".into()),
        role_id: "site_report".into(),
        required_fields: vec!["custom.project".into()],
        placeholders: vec!["custom.project".into(), "custom.responsible".into()],
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    };

    let plan = plan_workflow(
        &document,
        &SemanticCase::default(),
        &WorkflowFlags::default(),
    );
    let ids = plan
        .prompts
        .iter()
        .map(|prompt| prompt.field_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        ids,
        std::collections::BTreeSet::from(["custom.project", "custom.responsible"])
    );
    assert!(!plan.blocked);
}
