use dokkomplekt_core::{
    learn_template_from_examples, TemplateLearningInput, TemplateLearningValidationState,
};

fn inn_line(value: &str) -> String {
    format!("Card\nMode: standard\n\u{0418}\u{041d}\u{041d}: {value}")
}

fn source(value: &str) -> String {
    format!("Source\n\u{0418}\u{041d}\u{041d}: {value}")
}

#[test]
fn controlled_intervention_rejects_mutated_immutable_text() {
    let report = learn_template_from_examples(&TemplateLearningInput {
        blank_template_text: inn_line("__________"),
        completed_examples: vec![
            inn_line("7736050003"),
            inn_line("7707083893"),
            inn_line("7812014560"),
            "Card\nMode: changed\n\u{0418}\u{041d}\u{041d}: 7708004767".into(),
        ],
        source_examples: vec![
            source("7736050003"),
            source("7707083893"),
            source("7812014560"),
            source("7708004767"),
        ],
        default_year: 2026,
        locale: "ru-RU".into(),
    });

    assert!(report.validation.replay_passed);
    assert_eq!(
        report.validation.intervention_matches,
        report.validation.intervention_fields
    );
    assert!(report.validation.intervention_fields > 0);
    assert!(!report.validation.controlled_intervention_passed);
    assert!(
        report.validation.immutable_lines_preserved < report.validation.immutable_lines_checked
    );
    assert_eq!(
        report.validation.verdict,
        TemplateLearningValidationState::Failed
    );
    assert!(!report.validation.publishable);
}
