from pathlib import Path

source_path = Path("crates/dokkomplekt-core/src/source_parser.rs")
text = source_path.read_text(encoding="utf-8")

old = '''    if field == "medical.diagnosis" {
        return sanitize_medical_diagnosis(value);
    }
    if field.ends_with(".date") || field.ends_with("_date") {
'''
new = '''    if field == "medical.diagnosis" {
        return sanitize_medical_diagnosis(value);
    }
    if field == "subject.name" {
        return sanitize_subject_name(value);
    }
    if field.ends_with(".date") || field.ends_with("_date") {
'''
if text.count(old) != 1:
    raise SystemExit("normalize field anchor mismatch")
text = text.replace(old, new, 1)

anchor = '''fn sanitize_medical_diagnosis(value: &str) -> Option<String> {
'''
helper = '''fn sanitize_subject_name(value: &str) -> Option<String> {
    let cleaned = clean_value(value);
    if cleaned.is_empty() {
        return None;
    }
    let mut end = cleaned.len();
    if let Some(index) = cleaned.find(',') {
        end = end.min(index);
    }
    if let Some((index, _)) = cleaned.char_indices().find(|(_, ch)| ch.is_ascii_digit()) {
        end = end.min(index);
    }
    let name = cleaned[..end]
        .trim()
        .trim_end_matches(|ch: char| matches!(ch, ',' | ';' | ':'))
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_explicit_icd10_from_diagnosis(value: &str) -> Option<String> {
    let token = value
        .split_whitespace()
        .next()?
        .trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | ',' | ';' | ':' | '-'))
        .to_ascii_uppercase();
    let bytes = token.as_bytes();
    let shape_ok = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b'.');
    if !shape_ok {
        return None;
    }
    crate::search_icd10(&token, 1)
        .into_iter()
        .find(|row| row.code.eq_ignore_ascii_case(&token))
        .map(|row| row.code)
}

'''
if text.count(anchor) != 1:
    raise SystemExit("diagnosis sanitizer anchor mismatch")
text = text.replace(anchor, helper + anchor, 1)

old = '''        if case.get("medical.admission_date").is_none() {
            if let Some(date) = case.get("document.date").map(str::to_owned) {
                put(
                    &mut case,
                    &mut report,
                    "medical.admission_date",
                    &date,
                    0.70,
                );
            }
        }
        mirror_medical_to_generic(&mut case, &mut report);
'''
new = '''        if case.get("medical.admission_date").is_none() {
            if let Some(date) = case.get("document.date").map(str::to_owned) {
                put(
                    &mut case,
                    &mut report,
                    "medical.admission_date",
                    &date,
                    0.70,
                );
            }
        }
        if case.get("medical.icd10").is_none() {
            if let Some(diagnosis) = case.get("medical.diagnosis").map(str::to_owned) {
                if let Some(code) = extract_explicit_icd10_from_diagnosis(&diagnosis) {
                    put(&mut case, &mut report, "medical.icd10", &code, 0.90);
                }
            }
        }
        mirror_medical_to_generic(&mut case, &mut report);
'''
if text.count(old) != 1:
    raise SystemExit("medical post-processing anchor mismatch")
text = text.replace(old, new, 1)
source_path.write_text(text, encoding="utf-8")

aliases_path = Path("crates/dokkomplekt-core/src/field_aliases.rs")
aliases = aliases_path.read_text(encoding="utf-8")
old = '''    match field {
        "medical.diagnosis_code" => "medical.icd10".into(),
'''
new = '''    match field {
        "diagnosis.main" => "medical.diagnosis".into(),
        "diagnosis.icd10" | "icd10" | "medical.diagnosis_code" => "medical.icd10".into(),
'''
if aliases.count(old) != 1:
    raise SystemExit("canonical diagnosis alias anchor mismatch")
aliases = aliases.replace(old, new, 1)
old = '''    match canonical_storage_field_id(raw).as_str() {
        "medical.icd10" => &["medical.icd10", "medical.diagnosis_code"],
'''
new = '''    match canonical_storage_field_id(raw).as_str() {
        "medical.diagnosis" => &["medical.diagnosis", "diagnosis.main"],
        "medical.icd10" => &["medical.icd10", "medical.diagnosis_code", "diagnosis.icd10", "icd10"],
'''
if aliases.count(old) != 1:
    raise SystemExit("equivalent diagnosis alias anchor mismatch")
aliases = aliases.replace(old, new, 1)
aliases_path.write_text(aliases, encoding="utf-8")

registry_path = Path("crates/dokkomplekt-core/src/field_registry.rs")
registry = registry_path.read_text(encoding="utf-8")
old = '''                "diagnosis",
                "mainDiagnosis",
'''
new = '''                "diagnosis",
                "diagnosis.main",
                "mainDiagnosis",
'''
if registry.count(old) != 1:
    raise SystemExit("diagnosis registry anchor mismatch")
registry = registry.replace(old, new, 1)
old = '''                "icd_code",
                "diagnosisCode",
'''
new = '''                "icd_code",
                "diagnosis.icd10",
                "diagnosisCode",
'''
if registry.count(old) != 1:
    raise SystemExit("ICD registry anchor mismatch")
registry = registry.replace(old, new, 1)
registry_path.write_text(registry, encoding="utf-8")

test_path = Path("crates/dokkomplekt-core/tests/donor_medical_source_parity.rs")
tests = test_path.read_text(encoding="utf-8")
anchor = '''#[test]\nfn historical_medical_placeholders_resolve_to_current_schema() {\n'''
extra = '''#[test]\nfn donor_patient_name_drops_demographic_tail_but_preserves_initials() {\n    for (source, expected) in [\n        ("Пациентка: Петрова Анна Сергеевна, 1975 г.р.", "Петрова Анна Сергеевна"),\n        ("Пациент: Иванов Иван Иванович, 1980 года рождения, пол мужской.", "Иванов Иван Иванович"),\n        ("Пациент: Кузнецова-Смирнова Ольга Викторовна 1990 г.р.", "Кузнецова-Смирнова Ольга Викторовна"),\n        ("ФИО: Сидоров П.К.", "Сидоров П.К."),\n    ] {\n        let (case, _) = parse_source_text(source, 2026);\n        assert_eq!(case.get("subject.name"), Some(expected), "source {source}");\n    }\n}\n\n#[test]\nfn donor_diagnosis_exposes_only_explicit_catalogued_icd_code() {\n    let (case, _) = parse_source_text("История болезни № 42\\nДиагноз: K35 Острый аппендицит", 2026);\n    assert_eq!(case.get("medical.diagnosis"), Some("K35 Острый аппендицит"));\n    assert_eq!(case.get("medical.icd10"), Some("K35"));\n\n    let (case, _) = parse_source_text("История болезни № 43\\nДиагноз: unmapped local wording", 2026);\n    assert_eq!(case.get("medical.diagnosis"), Some("unmapped local wording"));\n    assert_eq!(case.get("medical.icd10"), None);\n}\n\n#[test]\nfn donor_single_line_dates_bind_to_their_own_markers() {\n    let (case, _) = parse_source_text(\n        "Дата рождения: 05.05.1980. Дата поступления: 10.02.2026. Выписан: 20.02.2026.",\n        2026,\n    );\n    assert_eq!(case.get("medical.admission_date"), Some("10.02.2026"));\n    assert_eq!(case.get("medical.discharge_date"), Some("20.02.2026"));\n\n    let (case, _) = parse_source_text("История болезни № 44\\nПоступил 10.02.2026, выписан 20.02.2026.", 2026);\n    assert_eq!(case.get("medical.admission_date"), Some("10.02.2026"));\n    assert_eq!(case.get("medical.discharge_date"), Some("20.02.2026"));\n}\n\n'''
if tests.count(anchor) != 1:
    raise SystemExit("donor parity insertion anchor mismatch")
tests = tests.replace(anchor, extra + anchor, 1)
tests = tests.replace(
    '''        ("patient.age", "subject.age"),\n''',
    '''        ("patient.age", "subject.age"),\n        ("diagnosis.main", "medical.diagnosis"),\n        ("diagnosis.icd10", "medical.icd10"),\n''',
    1,
)
test_path.write_text(tests, encoding="utf-8")
