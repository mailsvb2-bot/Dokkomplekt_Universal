from pathlib import Path

path = Path('crates/dokkomplekt-core/src/required_blocks.rs')
text = path.read_text(encoding='utf-8')

constants_anchor = '''const DIAGNOSIS_FIELDS: &[&str] = &[
    "medical.diagnosis",
    "medical.diagnosis_main",
    "diagnosis.main",
    "diagnosis",
];
'''
constants = constants_anchor + '''const TREATMENT_FIELDS: &[&str] = &[
    "medical.treatment",
    "medical.treatment_plan",
    "treatment.plan",
    "treatment",
];
const ADMISSION_DATE_FIELDS: &[&str] = &["medical.admission_date", "period.start_date"];
const DISCHARGE_DATE_FIELDS: &[&str] = &["medical.discharge_date", "period.end_date"];
'''
if 'const TREATMENT_FIELDS' not in text:
    if constants_anchor not in text:
        raise SystemExit('constants anchor missing')
    text = text.replace(constants_anchor, constants, 1)

start = text.index('fn role_blocks(role_id: &str) -> Vec<RequiredBlock> {')
end = text.index('\n/// Titles of every block', start)
new_role = r'''fn role_blocks(role_id: &str) -> Vec<RequiredBlock> {
    // Role identifiers may be namespaced by a profile (`medical.discharge`).
    // Completeness semantics belong to the terminal role, not to spelling style.
    let role = role_id.rsplit('.').next().unwrap_or(role_id);
    match role {
        "discharge" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("treatment", "Лечение", TREATMENT_FIELDS),
            RequiredBlock::any("admission_date", "Дата поступления", ADMISSION_DATE_FIELDS),
            RequiredBlock::any("discharge_date", "Дата выписки", DISCHARGE_DATE_FIELDS),
            RequiredBlock::signature(
                "treating_physician_signature",
                "Подпись лечащего врача",
                &["лечащий врач", "врач-психиатр", "врач психиатр"],
            ),
        ],
        "diaries" | "diary" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("admission_date", "Дата поступления", ADMISSION_DATE_FIELDS),
            RequiredBlock::any("discharge_date", "Дата выписки", DISCHARGE_DATE_FIELDS),
            RequiredBlock::signature(
                "treating_physician_signature",
                "Подпись лечащего врача",
                &["лечащий врач", "врач-психиатр", "врач психиатр"],
            ),
            RequiredBlock::signature(
                "department_head_signature",
                "Подпись заведующего отделением",
                &["заведующий отделением", "зав. отделением", "зав отделением"],
            ),
        ],
        "primary" => vec![
            RequiredBlock::any(
                "patient_identity",
                "Данные пациента (ФИО)",
                PATIENT_NAME_FIELDS,
            ),
            RequiredBlock::any("diagnosis", "Диагноз", DIAGNOSIS_FIELDS),
            RequiredBlock::any("treatment", "Лечение", TREATMENT_FIELDS),
        ],
        "rvk_act" | "vk_mse" | "commission" => vec![RequiredBlock::any(
            "patient_identity",
            "Данные освидетельствуемого (ФИО)",
            PATIENT_NAME_FIELDS,
        )],
        _ => Vec::new(),
    }
}
'''
text = text[:start] + new_role + text[end:]

if 'namespaced_discharge_requires_full_medical_contract' not in text:
    test_anchor = '''    #[test]
    fn narrative_doctor_mention_is_not_a_signature() {
'''
    test = r'''    #[test]
    fn namespaced_discharge_requires_full_medical_contract() {
        let blocks = required_blocks_for(&spec("medical.discharge", DomainKind::Medical), "");
        let partial = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "J06.9"),
        ]);
        let unmet = unmet_blocks(&blocks, &partial, "Лечащий врач ______");
        assert!(unmet.iter().any(|title| title == "Лечение"));
        assert!(unmet.iter().any(|title| title == "Дата поступления"));
        assert!(unmet.iter().any(|title| title == "Дата выписки"));
    }

    #[test]
    fn primary_requires_diagnosis_and_treatment() {
        let blocks = required_blocks_for(&spec("medical.primary", DomainKind::Medical), "");
        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let unmet = unmet_blocks(&blocks, &case, "");
        assert!(unmet.iter().any(|title| title == "Диагноз"));
        assert!(unmet.iter().any(|title| title == "Лечение"));
    }

'''
    if test_anchor not in text:
        raise SystemExit('test anchor missing')
    text = text.replace(test_anchor, test + test_anchor, 1)

# Update existing discharge test with the newly mandatory values.
text = text.replace('''            ("medical.diagnosis", "J06.9"),
        ]);
        let ok_text = "Диагноз: J06.9\\nЛечащий врач ______";
''', '''            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let ok_text = "Диагноз: J06.9\\nЛечащий врач ______";
''', 1)
# Existing test expected exactly two unmet blocks; now verify the intended two plus new safety blocks.
text = text.replace('''        // Missing diagnosis and missing signature section -> two unmet blocks.
''', '''        // Missing clinical data and signature must all be surfaced, never hidden.
''', 1)
text = text.replace('''        assert!(unmet.iter().any(|t| t.contains("Диагноз")));
        assert!(unmet.iter().any(|t| t.contains("Подпись лечащего врача")));
''', '''        assert!(unmet.iter().any(|t| t.contains("Диагноз")));
        assert!(unmet.iter().any(|t| t.contains("Лечение")));
        assert!(unmet.iter().any(|t| t.contains("Дата поступления")));
        assert!(unmet.iter().any(|t| t.contains("Дата выписки")));
        assert!(unmet.iter().any(|t| t.contains("Подпись лечащего врача")));
''', 1)
# Narrative signature test also needs other blocks satisfied so it tests only signature semantics.
text = text.replace('''            ("medical.diagnosis", "J06.9"),
        ]);
        let unmet = unmet_blocks(
            &blocks,
            &case,
            "Лечащий врач осмотрел пациента и продолжил наблюдение.",
''', '''            ("medical.diagnosis", "J06.9"),
            ("medical.treatment", "Терапия"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let unmet = unmet_blocks(
            &blocks,
            &case,
            "Лечащий врач осмотрел пациента и продолжил наблюдение.",
''', 1)
# Diary signature test must satisfy the newly required clinical/date fields.
text = text.replace('''        let case = case_with(&[("subject.name", "Иванов Иван")]);
        let one_signature = "Лечащий врач __________________";
''', '''        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.diagnosis", "F20.0"),
            ("medical.admission_date", "01.06.2026"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let one_signature = "Лечащий врач __________________";
''', 1)

path.write_text(text, encoding='utf-8')
