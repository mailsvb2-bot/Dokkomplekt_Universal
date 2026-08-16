use std::collections::BTreeMap;

use dokkomplekt_core::core::{SourceDocument, TargetTemplate};
use dokkomplekt_core::{
    run_universal_constructor_pipeline, UniversalDomain, UniversalPipelineFlags,
    UniversalPipelineInput,
};

fn detected_domain(template: &str) -> UniversalDomain {
    run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "source".into(),
            text: "source".into(),
            metadata: BTreeMap::new(),
        },
        target_template: TargetTemplate {
            id: "template".into(),
            path: "template.docx".into(),
            text: template.into(),
        },
        domain_hint: None,
        flags: UniversalPipelineFlags::default(),
    })
    .domain
}

#[test]
fn domain_detection_uses_template_semantics_without_a_hint() {
    let scenarios = [
        ("Contract\n{{contract.number}}", UniversalDomain::Legal),
        ("Employee card\n{{employee.name}}", UniversalDomain::Hr),
        ("Invoice\n{{amount.total}}", UniversalDomain::Accounting),
        (
            "Certificate\n{{education.student_name}}",
            UniversalDomain::Education,
        ),
        ("Project report\n{{custom.project}}", UniversalDomain::Custom),
    ];

    for (template, expected) in scenarios {
        assert_eq!(detected_domain(template), expected, "template: {template}");
    }
}
