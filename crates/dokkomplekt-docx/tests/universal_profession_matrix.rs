use dokkomplekt_core::core::{SourceDocument, TargetTemplate};
use dokkomplekt_core::{
    run_universal_constructor_pipeline, set_user_value, SemanticCase, UniversalDomain,
    UniversalPipelineFlags, UniversalPipelineInput,
};
use dokkomplekt_docx::{create_docx_from_text, extract_docx_text, render_docx_file};

struct Scenario {
    id: &'static str,
    domain: UniversalDomain,
    template: &'static str,
    values: &'static [(&'static str, &'static str)],
    expected: &'static [&'static str],
}

#[test]
fn the_same_real_docx_engine_renders_multiple_professions_and_an_arbitrary_custom_profile() {
    let scenarios = [
        Scenario {
            id: "legal",
            domain: UniversalDomain::Legal,
            template: "Претензия № {{document.number}}\nЗаявитель: {{subject.name}}\nТребование: {{legal.claim_subject}}",
            values: &[
                ("document.number", "П-17"),
                ("subject.name", "Иванов Иван"),
                ("legal.claim_subject", "Возврат оплаты"),
            ],
            expected: &["П-17", "Иванов Иван", "Возврат оплаты"],
        },
        Scenario {
            id: "hr",
            domain: UniversalDomain::Hr,
            template: "Приказ № {{hr.order_number}}\nСотрудник: {{employee.name}}\nДолжность: {{employee.position}}",
            values: &[
                ("hr.order_number", "42-к"),
                ("employee.name", "Петрова Анна"),
                ("employee.position", "Инженер"),
            ],
            expected: &["42-к", "Петрова Анна", "Инженер"],
        },
        Scenario {
            id: "accounting",
            domain: UniversalDomain::Accounting,
            template: "Счёт № {{accounting.invoice_number}}\nКонтрагент: {{counterparty.name}}\nИтого: {{amount.total}}",
            values: &[
                ("accounting.invoice_number", "С-901"),
                ("counterparty.name", "ООО «Вектор»"),
                ("amount.total", "125000"),
            ],
            expected: &["С-901", "ООО «Вектор»", "125000"],
        },
        Scenario {
            id: "education",
            domain: UniversalDomain::Education,
            template: "Справка № {{document.number}}\nОбучающийся: {{education.student_name}}\nОрганизация: {{education.institution}}",
            values: &[
                ("document.number", "У-55"),
                ("education.student_name", "Сидоров Максим"),
                ("education.institution", "Учебный центр"),
            ],
            expected: &["У-55", "Сидоров Максим", "Учебный центр"],
        },
        Scenario {
            id: "custom",
            domain: UniversalDomain::Custom,
            template: "Пользовательский отчёт\nПроект: {{custom.project}}\nОтветственный: {{custom.responsible}}\nРезультат: {{custom.result}}",
            values: &[
                ("custom.project", "Север"),
                ("custom.responsible", "Орлова Мария"),
                ("custom.result", "Этап принят"),
            ],
            expected: &["Север", "Орлова Мария", "Этап принят"],
        },
    ];

    let root = std::env::temp_dir().join(format!(
        "dokkomplekt-universal-profession-matrix-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create temp root");

    for scenario in scenarios {
        let pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
            source_document: SourceDocument {
                id: format!("source-{}", scenario.id),
                text: String::new(),
                metadata: Default::default(),
            },
            target_template: TargetTemplate {
                id: format!("template-{}", scenario.id),
                path: format!("{}.docx", scenario.id),
                text: scenario.template.to_string(),
            },
            domain_hint: Some(scenario.domain.clone()),
            flags: UniversalPipelineFlags::default(),
        });
        assert_eq!(pipeline.domain, scenario.domain, "{} lost its domain", scenario.id);

        let template_path = root.join(format!("{}-template.docx", scenario.id));
        let output_path = root.join(format!("{}-output.docx", scenario.id));
        create_docx_from_text(&template_path, scenario.template).expect("create real DOCX template");

        let mut case = SemanticCase::default();
        for (field_id, value) in scenario.values {
            set_user_value(&mut case, field_id, value);
        }
        let render = render_docx_file(&template_path, &output_path, &case, true)
            .expect("render real DOCX");
        assert!(render.missing_fields.is_empty(), "{} missing fields: {:?}", scenario.id, render.missing_fields);
        assert!(render.unknown_fields.is_empty(), "{} unknown fields: {:?}", scenario.id, render.unknown_fields);
        assert!(render.template_errors.is_empty(), "{} template errors: {:?}", scenario.id, render.template_errors);

        let rendered = extract_docx_text(&output_path).expect("read rendered DOCX back");
        for expected in scenario.expected {
            assert!(rendered.contains(expected), "{} did not render {expected:?}: {rendered:?}", scenario.id);
        }
        assert!(!rendered.contains("{{"), "{} left unresolved placeholders: {rendered:?}", scenario.id);
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unknown_profession_template_defaults_to_custom_instead_of_a_builtin_profession() {
    let template = "Полевой отчёт\nОбъект: {{custom.object}}\nНаблюдение: {{custom.note}}";
    let pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "unknown-source".into(),
            text: String::new(),
            metadata: Default::default(),
        },
        target_template: TargetTemplate {
            id: "unknown-template".into(),
            path: "unknown.docx".into(),
            text: template.into(),
        },
        domain_hint: None,
        flags: UniversalPipelineFlags::default(),
    });

    assert_eq!(pipeline.domain, UniversalDomain::Custom);
}
