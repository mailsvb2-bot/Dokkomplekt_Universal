use dokkomplekt_core::*;

fn medical_spec(id: &str, role_id: &str, required_fields: Vec<&str>) -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: id.into(),
        button_label: id.into(),
        template_path: format!("templates/{id}.docx"),
        category: DomainKind::Medical,
        role_id: role_id.into(),
        required_fields: required_fields.into_iter().map(str::to_string).collect(),
        placeholders: vec![],
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    }
}

#[test]
fn user_confirmed_value_wins_over_scanner_value() {
    let mut case = SemanticCase::default();
    set_user_value(&mut case, "medical.case_number", "123");
    set_scanner_value(&mut case, "medical.case_number", "Иванов Иван", 0.92);
    assert_eq!(case.get("medical.case_number"), Some("123"));
}

#[test]
fn scanner_can_fill_empty_case_then_user_can_correct_it() {
    let mut case = SemanticCase::default();
    set_scanner_value(&mut case, "medical.case_number", "Иванов Иван", 0.92);
    set_user_value(&mut case, "medical.case_number", "456");
    assert_eq!(case.get("medical.case_number"), Some("456"));
}

#[test]
fn parser_does_not_accept_fio_as_case_number() {
    let text = "12.01.2026 Первичный осмотр\nФИО: Иванов Иван Иванович\nИстория болезни № Иванов Иван Иванович\nДиагноз: тестовый\nЛечение: назначено";
    let (case, report) = parse_source_text(text, 2026);
    assert_eq!(case.get("subject.name"), Some("Иванов Иван Иванович"));
    assert_eq!(case.get("medical.case_number"), None);
    assert!(report.warnings.iter().any(|w| w.contains("похож на ФИО")));
}

#[test]
fn source_parser_finds_title_admission_treatment_and_work_fields() {
    let text = "12.01.2026 Первичный осмотр\nФИО: Иванов Иван Иванович\nИстория болезни № 123\nДиагноз: F00 тест\nЛечение: терапия\nМесто работы: Завод\nДолжность: инженер";
    let (case, report) = parse_source_text(text, 2026);
    assert_eq!(report.recognized_title.as_deref(), Some("Первичный осмотр"));
    assert_eq!(case.get("medical.admission_date"), Some("12.01.2026"));
    assert_eq!(case.get("medical.case_number"), Some("123"));
    assert_eq!(case.get("medical.treatment"), Some("терапия"));
    assert_eq!(case.get("medical.workplace"), Some("Завод"));
    assert_eq!(case.get("medical.position"), Some("инженер"));
}

#[test]
fn template_title_uses_top_document_name_not_random_body_phrase() {
    let text = "12.01.2026 Выписной эпикриз\nПациент: {{subject.name}}\nЛечение";
    let analysis = analyze_template_text(text);
    assert_eq!(analysis.title, "Выписной эпикриз");
    assert_eq!(analysis.suggested_button_label, "Выписной эпикриз");
    assert_eq!(analysis.role_id, "discharge");
}

#[test]
fn plain_docx_template_becomes_static_button_not_failed_workflow() {
    let analysis = analyze_template_text("Справка\nСтатический текст формы без меток");
    let spec = create_document_spec(
        "plain_certificate",
        "templates/cert.docx",
        &analysis,
        Some("Справка"),
    );
    assert!(spec.is_static_copy);
    assert_eq!(spec.category, DomainKind::Generic);
    let plan = plan_workflow(&spec, &SemanticCase::default(), &WorkflowFlags::default());
    assert!(plan.prompts.is_empty());
}

#[test]
fn role_detection_never_turns_unmarked_example_text_into_a_dynamic_template() {
    let analysis = analyze_template_text(
        "Выписной эпикриз\nПациент Иванов Иван Иванович\nЛечение: примерная терапия",
    );
    assert_eq!(analysis.role_id, "discharge");
    assert!(analysis.is_static);
    let spec = create_document_spec(
        "unmarked_discharge",
        "templates/unmarked.docx",
        &analysis,
        Some("Выписной эпикриз"),
    );
    assert!(spec.is_static_copy);
}

#[test]
fn custom_placeholders_are_allowed_for_universal_constructor() {
    let analysis =
        analyze_template_text("Договор № {{custom.contractor_name}} / {{legal.contract_number}}");
    assert!(analysis.unknown_placeholders.is_empty());
    let spec = create_document_spec("contract", "templates/contract.docx", &analysis, None);
    assert!(spec
        .required_fields
        .contains(&"custom.contractor_name".to_string()));
}

#[test]
fn invalid_placeholder_ids_are_reported_but_safe_custom_ids_are_not() {
    let analysis = analyze_template_text("Документ {{../bad}} {{custom.good_field}}");
    assert_eq!(analysis.unknown_placeholders, vec!["../bad".to_string()]);
}

#[test]
fn strict_renderer_reports_missing_valid_custom_fields_and_invalid_fields() {
    let result = render_text_template(
        "Договор № {{document.number}} / {{custom.note}} / {{../bad}}",
        &SemanticCase::default(),
        true,
    );
    assert_eq!(
        result.missing_fields,
        vec!["document.number".to_string(), "custom.note".to_string()]
    );
    assert_eq!(result.unknown_fields, vec!["../bad".to_string()]);
}

#[test]
fn discharge_merges_date_treatment_and_sick_leave_prompts() {
    let spec = medical_spec("discharge", "discharge", vec!["medical.case_number"]);
    let plan = plan_workflow(
        &spec,
        &SemanticCase::default(),
        &WorkflowFlags {
            sick_leave_enabled: true,
        },
    );
    let fields: Vec<_> = plan.prompts.iter().map(|p| p.field_id.as_str()).collect();
    assert!(fields.contains(&"medical.case_number"));
    assert!(fields.contains(&"medical.discharge_date"));
    assert!(fields.contains(&"medical.treatment"));
    assert!(fields.contains(&"medical.sick_leave_number"));
}

#[test]
fn sick_leave_number_is_not_requested_for_non_discharge_documents() {
    let spec = medical_spec("commission", "commission", vec![]);
    let plan = plan_workflow(
        &spec,
        &SemanticCase::default(),
        &WorkflowFlags {
            sick_leave_enabled: true,
        },
    );
    assert!(!plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.sick_leave_number"));
}

#[test]
fn medical_non_diary_documents_ask_treatment_if_source_did_not_have_it() {
    let spec = medical_spec("rvk", "rvk_act", vec![]);
    let plan = plan_workflow(&spec, &SemanticCase::default(), &WorkflowFlags::default());
    assert!(plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.treatment"));
}

#[test]
fn treatment_prompt_disappears_after_source_or_user_value_exists() {
    let spec = medical_spec("discharge", "discharge", vec![]);
    let mut case = SemanticCase::default();
    set_scanner_value(&mut case, "medical.treatment", "назначенное лечение", 0.81);
    let plan = plan_workflow(&spec, &case, &WorkflowFlags::default());
    assert!(!plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.treatment"));
}

#[test]
fn diaries_require_discharge_date_but_skip_treatment_prompt() {
    let spec = medical_spec("diaries", "diaries", vec![]);
    let plan = plan_workflow(&spec, &SemanticCase::default(), &WorkflowFlags::default());
    let fields: Vec<_> = plan.prompts.iter().map(|p| p.field_id.as_str()).collect();
    assert!(fields.contains(&"medical.discharge_date"));
    assert!(!fields.contains(&"medical.treatment"));
}

#[test]
fn diary_texts_keep_doctor_and_head_signatures() {
    let out = render_diary_text_with_signatures("02.06.26 Состояние стабильное");
    assert!(out.contains("Лечащий врач"));
    assert!(out.contains("Зав. отделением"));
}

#[test]
fn diary_plan_starts_day_after_admission_and_stops_on_discharge() {
    let plan = build_diary_plan(Some("01.06.2026"), Some("03.06.2026"), 2026).unwrap();
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].date, "02.06.2026");
    assert_eq!(plan[1].date, "03.06.2026");
    assert_eq!(plan[0].month, 6);
    assert_eq!(plan[0].year, 2026);
}

#[test]
fn diary_template_numbers_accept_1_and_01_docx() {
    assert_eq!(normalize_diary_template_number("1.docx"), Some(1));
    assert_eq!(normalize_diary_template_number("01.docx"), Some(1));
    assert_eq!(normalize_diary_template_number("31"), Some(31));
    assert_eq!(normalize_diary_template_number("32.docx"), None);
}

#[test]
fn date_parser_accepts_user_formats() {
    assert_eq!(
        parse_flexible_date("10052026", 2026).as_deref(),
        Some("10.05.2026")
    );
    assert_eq!(
        parse_flexible_date("100526", 2026).as_deref(),
        Some("10.05.2026")
    );
    assert_eq!(
        parse_flexible_date("1", 2026).as_deref(),
        Some("01.01.2026")
    );
    assert_eq!(
        parse_flexible_date("01", 2026).as_deref(),
        Some("01.01.2026")
    );
    // Ambiguous historical shorthand is intentionally rejected instead of
    // silently turning 1126 into 01.01.2026.
    assert_eq!(parse_flexible_date("1126", 2026), None);
}

#[test]
fn unknown_role_defaults_to_generic_not_medical() {
    let analysis = analyze_template_text("Справка\nОбычный офисный документ");
    assert_eq!(analysis.role_id, "unknown");
    assert_eq!(best_domain(&analysis), DomainKind::Generic);
}

#[test]
fn conflicting_field_values_require_confirmation() {
    let mut case = SemanticCase::default();
    set_user_value(&mut case, "medical.discharge_date", "03.06.2026");
    let incoming = SemanticValue::new(
        "medical.discharge_date",
        "04.06.2026",
        ValueSource::Scanner,
        0.7,
    );
    let conflict = detect_field_conflict(&case, &incoming).unwrap();
    assert_eq!(conflict.existing_value, "03.06.2026");
    assert_eq!(conflict.incoming_value, "04.06.2026");
}

#[test]
fn output_folder_name_uses_spaces_not_underscores() {
    let mut case = SemanticCase::default();
    set_user_value(&mut case, "subject.name", "Иванов Иван Иванович");
    set_user_value(&mut case, "medical.admission_date", "01.06.2026");
    set_user_value(&mut case, "medical.discharge_date", "03.06.2026");
    let name = build_output_folder_name(
        &case,
        &[
            FolderNamePart::ShortInitials,
            FolderNamePart::AdmissionAndDischargeDates,
        ],
    );
    assert_eq!(name, "Иванов И. И. 01.06.2026 - 03.06.2026");
    assert!(!name.contains('_'));
}
