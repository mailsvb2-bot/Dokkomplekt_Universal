use dokkomplekt_core::core::{SourceDocument, TargetTemplate};
use dokkomplekt_core::{
    analyze_template_structure_v2, build_button_scenario_v2, required_fields_for_plugin_role,
    run_universal_constructor_pipeline, DomainPluginId, UnifiedDataSchema, UniversalDomain,
    UniversalPipelineFlags, UniversalPipelineInput, WorkflowFlagSetV2,
};
use std::collections::{BTreeMap, BTreeSet};

fn set(values: impl IntoIterator<Item = impl Into<String>>) -> BTreeSet<String> {
    values.into_iter().map(Into::into).collect()
}

#[test]
fn canonical_nonmedical_role_rules_preserve_pre_e1_requirements() {
    let empty_flags = BTreeMap::new();
    let empty_present = BTreeSet::new();
    let cases = [
        (
            DomainPluginId::Legal,
            "acceptance_act",
            set([
                "document.number",
                "document.date",
                "contract.number",
                "contract.date",
                "contract.party_a",
                "contract.party_b",
            ]),
        ),
        (
            DomainPluginId::Hr,
            "employment_contract",
            set([
                "document.date",
                "org.name",
                "employee.name",
                "employee.position",
                "employee.hire_date",
                "employee.contract_number",
            ]),
        ),
        (
            DomainPluginId::Education,
            "certificate",
            set([
                "document.number",
                "document.date",
                "education.student_name",
                "education.institution",
            ]),
        ),
        (
            DomainPluginId::Education,
            "grade_report",
            set([
                "document.date",
                "education.student_name",
                "education.group",
                "education.course",
                "education.grade",
            ]),
        ),
        (
            DomainPluginId::Accounting,
            "service_act",
            set([
                "document.number",
                "document.date",
                "org.name",
                "counterparty.name",
                "amount.total",
            ]),
        ),
    ];

    for (domain, role, expected) in cases {
        let actual = required_fields_for_plugin_role(&domain, role, &empty_flags, &empty_present)
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "role requirement drift for {domain:?}/{role}"
        );
    }
}

#[test]
fn accounting_service_act_pipeline_and_scenario_share_one_rule_resolver() {
    let template_text = concat!(
        "АКТ ОКАЗАННЫХ УСЛУГ № {{document.number}}\n",
        "Дата: {{document.date}}\n",
        "Исполнитель: {{org.name}}\n",
        "Заказчик: {{counterparty.name}}\n",
        "Договор: {{contract.number}} от {{contract.date}}\n",
        "Услуги: {{contract.subject}}\n",
        "Стоимость: {{amount.total}} {{amount.currency}}\n",
        "НДС: {{amount.vat}}\n"
    );
    let pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "accounting-source".into(),
            text: "АКТ ОКАЗАННЫХ УСЛУГ № 17".into(),
            metadata: BTreeMap::new(),
        },
        target_template: TargetTemplate {
            id: "accounting.service_act".into(),
            path: "service_act.docx".into(),
            text: template_text.into(),
        },
        domain_hint: Some(UniversalDomain::Accounting),
        flags: UniversalPipelineFlags::default(),
    });
    assert_eq!(pipeline.domain, UniversalDomain::Accounting);

    let analysis = analyze_template_structure_v2(template_text);
    assert_eq!(analysis.domain, DomainPluginId::Accounting);
    assert_eq!(analysis.document_type, "service_act");
    let scenario = build_button_scenario_v2(
        &analysis,
        &UnifiedDataSchema::default(),
        &WorkflowFlagSetV2::default(),
    );

    let pipeline_required = pipeline
        .workflow
        .requires
        .into_iter()
        .collect::<BTreeSet<_>>();
    let scenario_required = scenario
        .requires
        .into_iter()
        .map(|field| field.field_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(pipeline_required, scenario_required);
    assert_eq!(
        pipeline_required,
        set([
            "document.number",
            "document.date",
            "org.name",
            "counterparty.name",
            "contract.number",
            "contract.date",
            "contract.subject",
            "amount.total",
            "amount.currency",
            "amount.vat",
        ])
    );
    assert!(pipeline_required
        .iter()
        .all(|field| !field.starts_with("medical.")));
}
