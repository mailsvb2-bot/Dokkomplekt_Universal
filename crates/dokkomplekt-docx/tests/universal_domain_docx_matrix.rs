use dokkomplekt_core::{
    apply_popup_answers, parse_source_text, plan_workflow, set_user_value, DocumentTemplateSpec,
    DomainKind, PopupAnswer, SemanticCase, WorkflowFlags,
};
use dokkomplekt_docx::{create_docx_from_text, extract_docx_text, render_docx_file};

#[test]
fn real_docx_renderer_accepts_fields_from_multiple_domains() {
    let cases = [
        ("document.number", "L1"),
        ("employee.name", "Employee"),
        ("amount.total", "125000"),
        ("education.student_name", "Student"),
        ("custom.project", "Project"),
    ];
    let root = std::env::temp_dir().join(format!("dokkomplekt-docx-matrix-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create temp directory");

    for (index, (field_id, value)) in cases.into_iter().enumerate() {
        let template = format!("{{{{{field_id}}}}}");
        let template_path = root.join(format!("template-{index}.docx"));
        let output_path = root.join(format!("output-{index}.docx"));
        create_docx_from_text(&template_path, &template).expect("create DOCX template");

        let mut case = SemanticCase::default();
        set_user_value(&mut case, field_id, value);
        let result =
            render_docx_file(&template_path, &output_path, &case, true).expect("render DOCX");
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
        assert!(result.template_errors.is_empty());

        let rendered = extract_docx_text(&output_path).expect("read rendered DOCX");
        assert!(rendered.contains(value));
        assert!(!rendered.contains("{{"));
    }

    let _ = std::fs::remove_dir_all(root);
}

struct ProfessionScenario {
    profession: &'static str,
    category: DomainKind,
    role_id: &'static str,
    source: &'static str,
    fields: &'static [(&'static str, &'static str)],
}

fn profession_scenarios() -> Vec<ProfessionScenario> {
    vec![
        ProfessionScenario {
            profession: "врач",
            category: DomainKind::Medical,
            role_id: "primary",
            source: "Первичный осмотр\nПациент: Иванов Иван Иванович\nПсихический статус: Контактен, ориентирован, спокоен.",
            fields: &[
                ("subject.name", "Иванов Иван Иванович"),
                ("medical.profile_status", "Контактен, ориентирован, спокоен"),
            ],
        },
        ProfessionScenario {
            profession: "юрист",
            category: DomainKind::Legal,
            role_id: "contract",
            source: "Договор № Ю-77 от 03.03.2026\nСторона 1: ООО Альфа\nСторона 2: ООО Бета",
            fields: &[
                ("contract.number", "Ю-77"),
                ("contract.date", "03.03.2026"),
                ("contract.party_a", "ООО Альфа"),
                ("contract.party_b", "ООО Бета"),
            ],
        },
        ProfessionScenario {
            profession: "кадровик",
            category: DomainKind::Hr,
            role_id: "employment_order",
            source: "Приказ № 44 от 16.02.2026\nСотрудник: Петров Пётр Петрович\nДолжность: инженер\nДата приёма: 17.02.2026",
            fields: &[
                ("hr.order_number", "44"),
                ("hr.order_date", "16.02.2026"),
                ("employee.name", "Петров Пётр Петрович"),
                ("employee.position", "инженер"),
                ("employee.hire_date", "17.02.2026"),
            ],
        },
        ProfessionScenario {
            profession: "бухгалтер",
            category: DomainKind::Accounting,
            role_id: "invoice",
            source: "Счёт № 148 от 01.02.2026\nПокупатель: ООО Василёк\nК оплате: 120000 руб.",
            fields: &[
                ("accounting.invoice_number", "148"),
                ("accounting.invoice_date", "01.02.2026"),
                ("counterparty.name", "ООО Василёк"),
                ("amount.total", "120000 руб"),
            ],
        },
        ProfessionScenario {
            profession: "педагог",
            category: DomainKind::Education,
            role_id: "grade_report",
            source: "Ведомость успеваемости\nСтудент: Смирнова Анна Сергеевна\nУчебное заведение: Университет № 1\nКурс: Физика\nОценка: отлично",
            fields: &[
                ("education.student_name", "Смирнова Анна Сергеевна"),
                ("education.institution", "Университет № 1"),
                ("education.course", "Физика"),
                ("education.grade", "отлично"),
            ],
        },
        ProfessionScenario {
            profession: "универсальный офисный пользователь",
            category: DomainKind::Generic,
            role_id: "document",
            source: "Документ № G-12 от 05.04.2026\nФИО: Орлов Олег Олегович\nОрганизация: ООО Универсал",
            fields: &[
                ("document.number", "G-12"),
                ("document.date", "05.04.2026"),
                ("subject.name", "Орлов Олег Олегович"),
                ("org.name", "ООО Универсал"),
            ],
        },
        ProfessionScenario {
            profession: "любая пользовательская профессия (ветеринар как пример)",
            category: DomainKind::Custom("veterinary".into()),
            role_id: "visit_record",
            source: "Ветеринарный осмотр. Владелец: Сидоров Сергей. Кличка: Барсик.",
            fields: &[
                ("custom.owner", "Сидоров Сергей"),
                ("custom.animal_name", "Барсик"),
                ("custom.procedure", "Вакцинация от бешенства"),
            ],
        },
    ]
}

#[test]
fn profession_release_matrix_tracks_every_builtin_profile_and_custom_fallback() {
    let scenarios = profession_scenarios();
    for profile in dokkomplekt_core::builtin_profiles() {
        assert!(
            scenarios
                .iter()
                .any(|scenario| scenario.category == profile.kind),
            "builtin profile {:?} has no end-to-end release-path scenario",
            profile.kind
        );
    }
    assert!(
        scenarios
            .iter()
            .any(|scenario| matches!(scenario.category, DomainKind::Custom(_))),
        "custom professions need an end-to-end release-path scenario"
    );
}

#[test]
fn every_profession_release_path_reaches_a_real_readable_docx() {
    let root = std::env::temp_dir().join(format!(
        "dokkomplekt-profession-release-path-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create profession release-path root");

    for (index, scenario) in profession_scenarios().into_iter().enumerate() {
        let (source_case, report) = parse_source_text(scenario.source, 2026);
        assert!(
            !report.filled_fields.is_empty(),
            "{}: source parser returned no usable facts",
            scenario.profession
        );

        let field_ids = scenario
            .fields
            .iter()
            .map(|(field_id, _)| (*field_id).to_string())
            .collect::<Vec<_>>();
        let document = DocumentTemplateSpec {
            id: format!("profession-{index}"),
            button_label: scenario.profession.to_string(),
            template_path: format!("profession-{index}.docx"),
            category: scenario.category.clone(),
            role_id: scenario.role_id.to_string(),
            required_fields: field_ids.clone(),
            placeholders: field_ids.clone(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        let plan = plan_workflow(&document, &source_case, &WorkflowFlags::default());
        assert!(
            !plan.blocked,
            "{}: user workflow was blocked: {:?}",
            scenario.profession, plan.block_reasons
        );

        let answers = plan
            .prompts
            .iter()
            .map(|prompt| {
                let value = scenario
                    .fields
                    .iter()
                    .find(|(field_id, _)| *field_id == prompt.field_id)
                    .map(|(_, value)| *value)
                    .or(prompt.current_value.as_deref())
                    .unwrap_or_else(|| {
                        panic!(
                            "{}: popup asked an unexpected field {}",
                            scenario.profession, prompt.field_id
                        )
                    });
                PopupAnswer {
                    field_id: prompt.field_id.clone(),
                    value: value.to_string(),
                    continue_without_value: false,
                }
            })
            .collect::<Vec<_>>();
        let applied = apply_popup_answers(&source_case, &plan, &answers);
        assert!(
            applied.accepted,
            "{}: popup answers rejected: {:?}; still missing: {:?}",
            scenario.profession, applied.errors, applied.still_missing
        );

        let fixed_marker = format!("FIXED-LAYOUT-{index}");
        let body = std::iter::once(fixed_marker.clone())
            .chain(
                scenario
                    .fields
                    .iter()
                    .map(|(field_id, _)| format!("{field_id}: {{{{{field_id}}}}}")),
            )
            .collect::<Vec<_>>()
            .join("\n");
        let template_path = root.join(format!("profession-{index}-template.docx"));
        let output_path = root.join(format!("profession-{index}-output.docx"));
        create_docx_from_text(&template_path, &body).expect("create profession template DOCX");
        let render_case = dokkomplekt_core::domains::case_for_document_render(
            &applied.semantic_case,
            &scenario.category,
            scenario.role_id,
        );
        let rendered = render_docx_file(&template_path, &output_path, &render_case, true)
            .unwrap_or_else(|error| panic!("{}: render failed: {error}", scenario.profession));
        assert!(
            rendered.missing_fields.is_empty(),
            "{}",
            scenario.profession
        );
        assert!(
            rendered.unknown_fields.is_empty(),
            "{}",
            scenario.profession
        );
        assert!(
            rendered.template_errors.is_empty(),
            "{}",
            scenario.profession
        );
        assert!(
            output_path.is_file(),
            "{}: no physical DOCX",
            scenario.profession
        );

        let text = extract_docx_text(&output_path).unwrap_or_else(|error| {
            panic!(
                "{}: generated DOCX unreadable: {error}",
                scenario.profession
            )
        });
        assert!(
            text.contains(&fixed_marker),
            "{}: fixed template structure marker was lost",
            scenario.profession
        );
        for (field_id, expected_value) in scenario.fields {
            assert!(
                text.contains(expected_value),
                "{}: final DOCX missed current value for {field_id}: {text}",
                scenario.profession
            );
        }
        assert!(
            !text.contains("{{"),
            "{}: unresolved technical placeholder leaked into final DOCX: {text}",
            scenario.profession
        );
    }

    let _ = std::fs::remove_dir_all(root);
}
