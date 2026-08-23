use dokkomplekt_core::*;
use std::path::Path;
use std::time::{Duration, SystemTime};

fn doc(role: &str, label: &str) -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: role.into(),
        button_label: label.into(),
        template_path: format!("{}.docx", role),
        category: DomainKind::Medical,
        role_id: role.into(),
        required_fields: vec![],
        placeholders: vec![],
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    }
}

#[test]
fn block_03_empty_on_first_launch() {
    let pack = empty_first_run_pack("default", "Пакет врача");
    assert!(pack.documents.is_empty());
}

#[test]
fn diaries_button_not_created_until_user_template_uploaded() {
    let pack = empty_first_run_pack("default", "Пакет врача");
    assert!(!pack
        .documents
        .iter()
        .any(|d| d.button_label.to_lowercase().contains("дневник")));
}

#[test]
fn uploaded_template_becomes_button_from_top_title() {
    let rows = prepare_template_confirmations(&[TemplateCandidate {
        document_id: "doc1".into(),
        template_path: "templates/Выписной.docx".into(),
        extracted_text: "12.01.2026 Выписной эпикриз\nПациент {{subject.name}}".into(),
        preferred_button_label: None,
        domain_override: None,
    }]);
    assert_eq!(rows[0].detected_title, "Выписной эпикриз");
    assert_eq!(rows[0].editable_button_label, "Выписной эпикриз");
}

#[test]
fn discharge_sick_leave_prompt_only_when_flag_enabled() {
    let case = SemanticCase::default();
    let mut discharge = doc("discharge", "Выписной эпикриз");
    discharge.placeholders = vec!["medical.sick_leave_number".into()];
    let off = plan_workflow(
        &discharge,
        &case,
        &WorkflowFlags {
            sick_leave_enabled: false,
        },
    );
    assert!(!off
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.sick_leave_number"));
    let on = plan_workflow(
        &discharge,
        &case,
        &WorkflowFlags {
            sick_leave_enabled: true,
        },
    );
    assert!(on
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.sick_leave_number"));
}

#[test]
fn diaries_require_discharge_date_but_never_treatment() {
    let case = SemanticCase::default();
    let mut diaries = doc("diaries", "Дневники");
    diaries.placeholders = vec!["medical.discharge_date".into(), "medical.treatment".into()];
    let plan = plan_workflow(&diaries, &case, &WorkflowFlags::default());
    assert!(plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.discharge_date"));
    assert!(!plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.treatment"));
}

#[test]
fn primary_with_treatment_does_not_ask_treatment_again() {
    let (case, report) = parse_source_text(
        "Первичный осмотр\nФИО: Иванов Иван\nДиагноз: F20\nЛечение: терапия",
        2026,
    );
    assert!(report
        .filled_fields
        .contains(&"medical.treatment".to_string()));
    let plan = plan_workflow(
        &doc("primary", "Первичный осмотр"),
        &case,
        &WorkflowFlags::default(),
    );
    assert!(!plan
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.treatment"));
}

#[test]
fn popup_stays_open_on_wrong_empty_required_value() {
    let case = SemanticCase::default();
    let plan = WorkflowPlan {
        document_id: "x".into(),
        prompts: vec![PromptSpec {
            field_id: "medical.case_number".into(),
            title: "Номер истории болезни".into(),
            required: true,
            skippable: false,
            current_value: None,
            validation_hint: None,
            input_kind: PromptInputKind::Text,
            ask_mode: PromptAskMode::IfMissing,
            options: Vec::new(),
            allow_custom_option: false,
            help_text: None,
            section: None,
            linked_to: None,
            order: 500,
        }],
        blocked: false,
        block_reasons: vec![],
    };
    let result = apply_popup_answers(
        &case,
        &plan,
        &[PopupAnswer {
            field_id: "medical.case_number".into(),
            value: "".into(),
            continue_without_value: false,
        }],
    );
    assert!(!result.accepted);
    assert!(result.message.contains("Номер истории болезни"));
}

#[test]
fn popup_values_are_reused_between_documents() {
    let mut case = SemanticCase::default();
    remember_shared_answers(
        &mut case,
        &[
            ("medical.case_number", "123"),
            ("medical.workplace", "ООО Ромашка"),
            ("medical.position", "инженер"),
        ],
    );
    let discharge = plan_workflow(
        &doc("discharge", "Выписной эпикриз"),
        &case,
        &WorkflowFlags::default(),
    );
    assert!(!discharge
        .prompts
        .iter()
        .any(|p| p.field_id == "medical.case_number"));
    let vk = plan_workflow(
        &doc("vk_mse", "ВК на МСЭ"),
        &case,
        &WorkflowFlags::default(),
    );
    assert!(!vk.prompts.iter().any(|p| p.field_id == "medical.workplace"));
    assert!(!vk.prompts.iter().any(|p| p.field_id == "medical.position"));
}

#[test]
fn diary_dates_start_admission_plus_one_and_stop_on_discharge() {
    let entries = build_diary_plan(Some("01.06.2026"), Some("03.06.2026"), 2026).unwrap();
    assert_eq!(
        entries.iter().map(|e| e.date.as_str()).collect::<Vec<_>>(),
        vec!["02.06.2026", "03.06.2026"]
    );
}

#[test]
fn diary_signatures_are_never_lost() {
    let text = render_diary_text_with_signatures("Состояние стабильное.");
    assert!(text.contains("Лечащий врач"));
    assert!(text.contains("Заведующий отделением"));
}

#[test]
fn scanner_cannot_override_user_popup_value() {
    let mut case = SemanticCase::default();
    set_user_value(&mut case, "medical.case_number", "123");
    apply_scanner_marks(
        &mut case,
        &[ScannerMark {
            field_id: "medical.case_number".into(),
            selected_text: "Иванов Иван".into(),
            page_index: 0,
            confidence: 0.95,
        }],
    );
    assert_eq!(case.get("medical.case_number"), Some("123"));
}

#[test]
fn drag_into_created_documents_folder_does_not_start_two_ui_windows() {
    let mut dedup = IntakeDeduplicator::new(Duration::from_secs(3));
    let path = Path::new("C:/Users/Пользователь/Desktop/Выписанные пациенты/Первичный.docx");
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
    assert_eq!(dedup.decide(path, t), IntakeDecision::Accept);
    assert_eq!(
        dedup.decide(path, t + Duration::from_millis(100)),
        IntakeDecision::IgnoreDuplicateWithinDebounce
    );
    let route = route_intake_event(true, true);
    assert!(!route.should_start_ui);
}

#[test]
fn rvc_commissariat_declension_matches_contract() {
    assert_eq!(decline_rvk_district("Автозаводский"), "Автозаводского");
    assert_eq!(decline_rvk_district("Московский"), "Московского");
}

#[test]
fn output_patient_folder_has_spaces_no_underscores() {
    let mut case = SemanticCase::default();
    set_user_value(&mut case, "subject.name", "Иванов Иван Иванович");
    let plan = plan_output_paths(
        Path::new("C:/Users/Пользователь/Desktop/Выписанные пациенты"),
        &case,
        &[FolderNamePart::FullSubjectName],
        &["Выписной эпикриз".into()],
    );
    let path = plan.patient_folder.to_string_lossy();
    assert!(path.contains("Иванов Иван Иванович"));
    assert!(!path.contains('_'));
}

#[test]
fn custom_template_fields_are_valid_and_become_prompts() {
    let analysis = analyze_template_text("Договор\n{{custom.local_note}}\n{{data.any_field}}");
    assert!(analysis.unknown_placeholders.is_empty());
    assert!(analysis
        .placeholders
        .contains(&"custom.local_note".to_string()));
}

#[test]
fn invalid_placeholders_are_blocked() {
    let analysis = analyze_template_text("Документ {{../bad}}");
    assert_eq!(analysis.unknown_placeholders, vec!["../bad".to_string()]);
}
