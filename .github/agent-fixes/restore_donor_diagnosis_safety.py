from pathlib import Path

path = Path("crates/dokkomplekt-core/src/source_parser.rs")
text = path.read_text(encoding="utf-8")

old = '''fn normalize_field_value(field: &str, value: &str, default_year: i32) -> Option<String> {
    if field.ends_with(".date") || field.ends_with("_date") {
'''
new = '''fn normalize_field_value(field: &str, value: &str, default_year: i32) -> Option<String> {
    if field == "medical.diagnosis" {
        return sanitize_medical_diagnosis(value);
    }
    if field.ends_with(".date") || field.ends_with("_date") {
'''
if text.count(old) != 1:
    raise SystemExit("normalize_field_value anchor mismatch")
text = text.replace(old, new, 1)

anchor = '''fn apply_role_aware_source_facts(
'''
helper = r'''fn sanitize_medical_diagnosis(value: &str) -> Option<String> {
    let mut cleaned = clean_value(value);
    if cleaned.is_empty() {
        return None;
    }

    let lower = cleaned.to_lowercase();
    if ["лечение", "назначенное лечение", "план лечения"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return None;
    }

    let has_icd_code = contains_icd_like_code(&cleaned);
    if !has_icd_code {
        let admin_words = [
            "подпись",
            "подпис",
            "кнопк",
            "шаблон",
            "документ",
            "попап",
            "вк на мсэ",
            "мсэ",
            "рвк",
            "комисс",
            "эпикриз",
            "галочк",
            "выбира",
            "созда",
            "встав",
            "подстав",
            "поле",
            "файл",
            "блок 03",
        ];
        let hits = admin_words
            .iter()
            .filter(|word| lower.contains(**word))
            .count();
        let instruction_words = ["где", "куда", "котор", "нужно", "надо", "для", "чтобы", "или"];
        let looks_like_instruction = instruction_words
            .iter()
            .any(|word| lower.split(|ch: char| !ch.is_alphanumeric()).any(|token| token == *word));
        if hits >= 2 || (hits >= 1 && looks_like_instruction) || (hits >= 1 && cleaned.chars().count() > 90) {
            return None;
        }
    }

    cleaned = cleaned
        .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';'))
        .trim()
        .to_string();
    if cleaned.is_empty() || looks_like_known_label(&cleaned) {
        return None;
    }
    Some(cleaned)
}

fn contains_icd_like_code(value: &str) -> bool {
    value
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.'))
        .any(|token| {
            let bytes = token.as_bytes();
            bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1].is_ascii_digit()
                && bytes[2].is_ascii_digit()
                && (bytes.len() == 3 || bytes[3] == b'.' || bytes[3].is_ascii_alphanumeric())
        })
}

'''
if text.count(anchor) != 1:
    raise SystemExit("apply_role_aware_source_facts anchor mismatch")
text = text.replace(anchor, helper + anchor, 1)

test_anchor = '''    #[test]
    fn medical_values_are_mirrored_into_generic_core_fields() {
'''
tests = r'''    #[test]
    fn donor_diagnosis_safety_rejects_template_and_admin_noise() {
        assert_eq!(
            sanitize_medical_diagnosis("лечение и подпись документа, выбрать шаблон"),
            None
        );
        assert_eq!(
            sanitize_medical_diagnosis("Шаблон документа для МСЭ: выбрать поле"),
            None
        );
        assert_eq!(sanitize_medical_diagnosis("Лечение: режим"), None);
    }

    #[test]
    fn donor_diagnosis_safety_preserves_real_formulation_and_icd_code() {
        assert_eq!(
            sanitize_medical_diagnosis("F20.0 Параноидная шизофрения."),
            Some("F20.0 Параноидная шизофрения".into())
        );
        assert_eq!(
            sanitize_medical_diagnosis("J20 Острый бронхит"),
            Some("J20 Острый бронхит".into())
        );
        assert_eq!(
            sanitize_medical_diagnosis("Острый бронхит"),
            Some("Острый бронхит".into())
        );
    }

'''
if text.count(test_anchor) != 1:
    raise SystemExit("source parser test anchor mismatch")
text = text.replace(test_anchor, tests + test_anchor, 1)

path.write_text(text, encoding="utf-8")
