use dokkomplekt_core::{
    apply_popup_answers, build_medical_diary_series, plan_workflow_batch,
    render_diary_text_with_signatures, set_user_value, DocumentTemplateSpec, DomainKind,
    MedicalDiarySeriesRequest, PopupAnswer, SemanticCase, WorkflowFlags,
};
use dokkomplekt_docx::{create_docx_from_text, extract_docx_text, render_docx_file};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn medical_document(
    id: &str,
    label: &str,
    role_id: &str,
    placeholders: &[&str],
) -> DocumentTemplateSpec {
    DocumentTemplateSpec {
        id: id.to_string(),
        button_label: label.to_string(),
        template_path: format!("templates/{id}.docx"),
        category: DomainKind::Medical,
        role_id: role_id.to_string(),
        required_fields: placeholders.iter().map(|value| (*value).to_string()).collect(),
        placeholders: placeholders.iter().map(|value| (*value).to_string()).collect(),
        is_static_copy: false,
        popup_fields: Vec::new(),
        popup_configured: false,
    }
}

fn answer_values() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("subject.name", "Иванов Иван Иванович"),
        ("medical.case_number", "ИБ-4242"),
        ("medical.admission_date", "10.05.2026"),
        ("medical.discharge_date", "13.05.2026"),
        ("medical.diagnosis", "F32.1 Депрессивный эпизод средней степени"),
        ("medical.treatment", "Сертралин 50 мг утром"),
        ("medical.sick_leave_number", "123456789012"),
    ])
}

fn unique_test_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "dokkomplekt-medical-bundle-docx-e2e-{}",
        std::process::id()
    ))
}

#[test]
fn selected_medical_bundle_goes_from_one_popup_to_real_docx_files() {
    let mut source_case = SemanticCase::default();
    set_user_value(&mut source_case, "subject.name", "Иванов Иван Иванович");
    set_user_value(&mut source_case, "medical.admission_date", "10.05.2026");
    set_user_value(
        &mut source_case,
        "medical.diagnosis",
        "F32.1 Депрессивный эпизод средней степени",
    );
    // A referral may contain narrative text, but it must never become treatment merely
    // because it was present in the source document.
    set_user_value(
        &mut source_case,
        "medical.referral_text",
        "НАПРАВЛЕНИЕ_НЕ_ИСТОЧНИК_ЛЕЧЕНИЯ",
    );

    let shared_fields = [
        "subject.name",
        "medical.case_number",
        "medical.admission_date",
        "medical.diagnosis",
        "medical.treatment",
    ];
    let primary = medical_document(
        "primary",
        "Первичный осмотр",
        "primary",
        &shared_fields,
    );
    let discharge = medical_document(
        "discharge",
        "Выписной эпикриз",
        "discharge",
        &[
            "subject.name",
            "medical.case_number",
            "medical.admission_date",
            "medical.discharge_date",
            "medical.diagnosis",
            "medical.treatment",
        ],
    );

    let flags = WorkflowFlags {
        sick_leave_enabled: false,
    };
    let plan = plan_workflow_batch(&[primary.clone(), discharge.clone()], &source_case, &flags);
    assert!(!plan.blocked, "merged popup plan is blocked: {:?}", plan.block_reasons);
    assert_eq!(
        plan.prompts
            .iter()
            .filter(|prompt| prompt.field_id == "medical.treatment")
            .count(),
        1,
        "shared treatment must be asked once for the whole selected set"
    );

    let values = answer_values();
    let answers = plan
        .prompts
        .iter()
        .map(|prompt| PopupAnswer {
            field_id: prompt.field_id.clone(),
            value: values
                .get(prompt.field_id.as_str())
                .copied()
                .or(prompt.current_value.as_deref())
                .unwrap_or_else(|| panic!("no synthetic answer for {}", prompt.field_id))
                .to_string(),
            continue_without_value: false,
        })
        .collect::<Vec<_>>();
    let applied = apply_popup_answers(&source_case, &plan, &answers);
    assert!(
        applied.accepted,
        "popup answers were rejected: {:?}; still missing: {:?}",
        applied.errors, applied.still_missing
    );
    assert_eq!(
        applied.semantic_case.get("medical.treatment"),
        Some("Сертралин 50 мг утром")
    );

    // Sick-leave number is not asked for an ordinary discharge when the toggle is off,
    // even if a learned/custom discharge template knows that field exists.
    let discharge_with_sick_leave = medical_document(
        "discharge-sick",
        "Выписной эпикриз",
        "discharge",
        &[
            "medical.case_number",
            "medical.discharge_date",
            "medical.diagnosis",
            "medical.treatment",
            "medical.sick_leave_number",
        ],
    );
    let no_sick_plan = dokkomplekt_core::plan_workflow(
        &discharge_with_sick_leave,
        &applied.semantic_case,
        &WorkflowFlags {
            sick_leave_enabled: false,
        },
    );
    assert!(
        no_sick_plan
            .prompts
            .iter()
            .all(|prompt| prompt.field_id != "medical.sick_leave_number")
    );
    let with_sick_plan = dokkomplekt_core::plan_workflow(
        &discharge_with_sick_leave,
        &applied.semantic_case,
        &WorkflowFlags {
            sick_leave_enabled: true,
        },
    );
    assert!(with_sick_plan
        .prompts
        .iter()
        .any(|prompt| prompt.field_id == "medical.sick_leave_number"));

    let dir = unique_test_dir();
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create test output directory");

    let primary_template = dir.join("primary-template.docx");
    let primary_output = dir.join("Первичный осмотр.docx");
    create_docx_from_text(
        &primary_template,
        "ПЕРВИЧНЫЙ ОСМОТР\nИстория болезни № {{medical.case_number}}\nДата поступления: {{medical.admission_date}}\nПациент: {{subject.name}}\nДиагноз: {{medical.diagnosis}}\nЛечение: {{medical.treatment}}\nЛечащий врач __________________ /____________/",
    )
    .expect("create primary template DOCX");
    let primary_render = render_docx_file(
        &primary_template,
        &primary_output,
        &applied.semantic_case,
        true,
    )
    .expect("render primary DOCX");
    assert!(primary_render.missing_fields.is_empty());
    assert!(primary_render.unknown_fields.is_empty());
    assert!(primary_render.template_errors.is_empty());

    let discharge_template = dir.join("discharge-template.docx");
    let discharge_output = dir.join("Выписной эпикриз.docx");
    create_docx_from_text(
        &discharge_template,
        "ВЫПИСНОЙ ЭПИКРИЗ\nИстория болезни № {{medical.case_number}}\nПоступил: {{medical.admission_date}}\nВыписан: {{medical.discharge_date}}\nПациент: {{subject.name}}\nДиагноз: {{medical.diagnosis}}\nЛечение: {{medical.treatment}}\nЛечащий врач __________________ /____________/",
    )
    .expect("create discharge template DOCX");
    let discharge_render = render_docx_file(
        &discharge_template,
        &discharge_output,
        &applied.semantic_case,
        true,
    )
    .expect("render discharge DOCX");
    assert!(discharge_render.missing_fields.is_empty());
    assert!(discharge_render.unknown_fields.is_empty());
    assert!(discharge_render.template_errors.is_empty());

    for output in [&primary_output, &discharge_output] {
        assert!(output.is_file(), "expected real DOCX at {}", output.display());
        let text = extract_docx_text(output).expect("read rendered DOCX back");
        assert!(text.contains("ИБ-4242"));
        assert!(text.contains("Иванов Иван Иванович"));
        assert!(text.contains("F32.1 Депрессивный эпизод средней степени"));
        assert!(text.contains("Сертралин 50 мг утром"));
        assert!(!text.contains("НАПРАВЛЕНИЕ_НЕ_ИСТОЧНИК_ЛЕЧЕНИЯ"));
        assert!(!text.contains("{{"), "unfilled placeholder leaked into output");
    }
    let discharge_text = extract_docx_text(&discharge_output).expect("read discharge DOCX");
    assert!(discharge_text.contains("Поступил: 10.05.2026"));
    assert!(discharge_text.contains("Выписан: 13.05.2026"));

    let diary_plan = build_medical_diary_series(&MedicalDiarySeriesRequest {
        admission_date: "10.05.2026".into(),
        discharge_date: "13.05.2026".into(),
        default_year: 2026,
        confirmed_cadence: None,
        profile_cadence: None,
        day_start_time: None,
        day_end_time: None,
        skip_weekdays: Vec::new(),
        excluded_dates: Vec::new(),
        force_final_discharge_entry: true,
    })
    .expect("build diary schedule");
    assert_eq!(
        diary_plan
            .iter()
            .map(|entry| entry.date.as_str())
            .collect::<Vec<_>>(),
        vec!["11.05.2026", "12.05.2026", "13.05.2026"]
    );
    assert!(diary_plan.last().is_some_and(|entry| entry.is_final_discharge_entry));

    for entry in &diary_plan {
        let body = format!(
            "{}\nДНЕВНИК НАБЛЮДЕНИЯ\nПациент: Иванов Иван Иванович\nДиагноз: F32.1 Депрессивный эпизод средней степени\nСостояние без отрицательной динамики.",
            entry.date
        );
        let signed = render_diary_text_with_signatures(&body);
        let path = dir.join(format!("Дневник {}.docx", entry.date));
        create_docx_from_text(&path, &signed).expect("write diary DOCX");
        let text = extract_docx_text(&path).expect("read diary DOCX back");
        assert!(text.starts_with(&entry.date));
        assert!(text.contains("Лечащий врач __________________"));
        assert!(text.contains("Заведующий отделением __________"));
    }

    let _ = fs::remove_dir_all(&dir);
}
