use dokkomplekt_core::core::{SourceDocument, TargetTemplate};
use dokkomplekt_core::{
    run_universal_constructor_pipeline, UniversalDomain, UniversalPipelineFlags,
    UniversalPipelineInput,
};
use std::collections::BTreeMap;

#[test]
fn core_domains_are_executable_pipeline_not_cosmetic_files() {
    let result = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "s".into(),
            text: "Первичный документ".into(),
            metadata: BTreeMap::new(),
        },
        target_template: TargetTemplate {
            id: "t".into(),
            path: "discharge.docx".into(),
            text: "Выписной эпикриз\n{{medical.case_number}}\n{{medical.diagnosis}}".into(),
        },
        domain_hint: None,
        flags: UniversalPipelineFlags {
            sick_leave_enabled: true,
        },
    });
    assert_eq!(result.domain, UniversalDomain::Medical);
    assert_eq!(result.button.label, "Выписной эпикриз");
    assert!(result
        .workflow
        .requires
        .contains(&"medical.discharge_date".to_string()));
    assert!(result
        .workflow
        .requires
        .contains(&"medical.sick_leave_number".to_string()));
}

#[test]
fn legal_pipeline_never_inherits_medical_fields() {
    let result = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument { id: "s".into(), text: "Договор".into(), metadata: BTreeMap::new() },
        target_template: TargetTemplate { id: "t".into(), path: "contract.docx".into(), text: "Договор оказания услуг\n{{contract.number}}\n{{contract.date}}\n{{contract.party_a}}".into() },
        domain_hint: None,
        flags: UniversalPipelineFlags::default(),
    });
    assert_eq!(result.domain, UniversalDomain::Legal);
    assert!(result
        .workflow
        .requires
        .iter()
        .all(|field| !field.starts_with("medical.")));
}
