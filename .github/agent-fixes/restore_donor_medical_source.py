from __future__ import annotations

from pathlib import Path
import json


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one anchor, found {count}")
    return text.replace(old, new, 1)


def patch_source_parser() -> None:
    path = Path("crates/dokkomplekt-core/src/source_parser.rs")
    text = path.read_text(encoding="utf-8")
    text = replace_once(
        text,
        '        "дата поступления",\n    ]',
        '        "дата поступления",\n        "жалобы",\n        "анамнез",\n        "соматический статус",\n        "профильный статус",\n        "лаборатор",\n    ]',
        "medical source markers",
    )

    start = text.index("fn medical_rules() -> Vec<LabelRule> {")
    end = text.index("\nfn mirror_medical_to_generic", start)
    rules = '''fn medical_rules() -> Vec<LabelRule> {
    vec![
        LabelRule { field: "medical.case_number", labels: &["История болезни №", "История болезни N", "Номер истории болезни", "ИБ №", "и/б №", "Nr historii choroby", "Numer historii choroby", "Historia choroby nr"], multiline: false },
        LabelRule { field: "subject.name", labels: &["Ф.И.О.", "Ф.И.О", "ФИО пациента", "Ф.И.О. пациента", "Фамилия Имя Отчество", "Пациент", "Пациентка", "Pacjent", "Pacjentka", "Imię i nazwisko", "Imie i nazwisko"], multiline: false },
        LabelRule { field: "subject.age", labels: &["Возраст", "Wiek"], multiline: false },
        LabelRule { field: "subject.birth_date", labels: &["Дата рождения", "Data urodzenia"], multiline: false },
        LabelRule { field: "subject.address", labels: &["Зарегистрирован по адресу", "Адрес регистрации", "Адрес проживания", "Место жительства", "Adres zamieszkania", "Miejsce zamieszkania"], multiline: false },
        LabelRule { field: "medical.admission_date", labels: &["Дата поступления", "Дата госпитализации", "Data przyjęcia", "Data przyjecia", "Data hospitalizacji"], multiline: false },
        LabelRule { field: "medical.discharge_date", labels: &["Дата выписки", "Data wypisu"], multiline: false },
        LabelRule { field: "medical.complaints", labels: &["Жалобы на момент осмотра", "Жалобы при поступлении", "Жалобы", "Skargi przy przyjęciu", "Skargi przy przyjeciu", "Dolegliwości", "Dolegliwosci", "Skargi"], multiline: true },
        LabelRule { field: "medical.anamnesis_life", labels: &["Анамнез жизни", "Wywiad życiowy", "Wywiad zyciowy", "Wywiad osobniczy"], multiline: true },
        LabelRule { field: "medical.anamnesis_disease", labels: &["Анамнез заболевания", "Wywiad chorobowy", "Wywiad obecnej choroby", "Historia choroby"], multiline: true },
        LabelRule { field: "medical.profile_status", labels: &["Профильный статус при поступлении", "Профильный статус", "Психический статус при поступлении", "Психический статус", "Stan psychiczny", "Badanie psychiatryczne"], multiline: true },
        LabelRule { field: "medical.somatic_status", labels: &["Сомато-неврологический статус", "Соматический статус", "Объективный статус", "Объективно", "Status praesens", "Stan przedmiotowy", "Badanie przedmiotowe", "Stan somatyczny"], multiline: true },
        LabelRule { field: "medical.examination_plan", labels: &["План обследования", "Plan badań", "Plan badan"], multiline: true },
        LabelRule { field: "medical.diagnosis", labels: &["Клинический диагноз", "Предварительный диагноз", "Основной диагноз", "Заключительный диагноз", "Диагноз", "Rozpoznanie kliniczne", "Rozpoznanie główne", "Rozpoznanie glowne", "Rozpoznanie", "Diagnoza"], multiline: true },
        LabelRule { field: "medical.icd10", labels: &["Код МКБ-10", "Код МКБ", "МКБ-10", "ICD-10"], multiline: false },
        LabelRule { field: "medical.treatment", labels: &["План лечения", "Назначенное лечение", "Лечение", "Plan leczenia", "Zalecone leczenie", "Zastosowane leczenie", "Leczenie", "Terapia"], multiline: true },
        LabelRule { field: "medical.treatment_result", labels: &["Результат лечения", "Исход лечения", "Эффект лечения"], multiline: true },
        LabelRule { field: "medical.discharge_condition", labels: &["Состояние при выписке", "Состояние на момент выписки"], multiline: true },
        LabelRule { field: "medical.recommendations", labels: &["Рекомендации", "Рекомендовано", "Zalecenia"], multiline: true },
        LabelRule { field: "medical.labs", labels: &["Лабораторные исследования", "Лабораторные данные", "Результаты анализов", "Результаты обследований", "Результаты исследований", "Анализы", "Wyniki badań", "Wyniki badan"], multiline: true },
        LabelRule { field: "medical.labs_date", labels: &["Дата анализов", "Дата лабораторных исследований"], multiline: false },
        LabelRule { field: "medical.workplace", labels: &["Работает в организации", "Место работы", "Работа", "Miejsce pracy", "Zakład pracy", "Zaklad pracy"], multiline: false },
        LabelRule { field: "medical.position", labels: &["Должность", "Stanowisko", "Zawód", "Zawod"], multiline: false },
        LabelRule { field: "medical.sick_leave_number", labels: &["Номер больничного", "Больничный лист №", "Лист нетрудоспособности №"], multiline: false },
        LabelRule { field: "medical.attending_doctor", labels: &["Лечащий врач", "Lekarz prowadzący", "Lekarz prowadzacy"], multiline: false },
        LabelRule { field: "medical.department_head", labels: &["Заведующий отделением", "Зав. отделением", "Зав. отд.", "Ordynator", "Kierownik oddziału", "Kierownik oddzialu"], multiline: false },
    ]
}
'''
    text = text[:start] + rules + text[end:]

    start = text.index("fn clean_inline_value(value: &str) -> String {")
    end = text.index("\nfn first_date_candidate", start)
    helper = '''fn clean_inline_value(value: &str) -> String {
    let mut end = value.len();
    for (index, ch) in value.char_indices() {
        if !matches!(ch, ',' | ';' | '.') {
            continue;
        }
        let tail = value[index + ch.len_utf8()..].trim_start();
        if !tail.is_empty() && looks_like_known_label(tail) {
            end = index;
            break;
        }
    }
    if let Some(next_label) = next_explicit_inline_label_start(value) {
        end = end.min(next_label);
    }
    clean_value(&value[..end])
}

fn next_explicit_inline_label_start(value: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    for rule in generic_rules().into_iter().chain(medical_rules()) {
        for label in rule.labels {
            let Some(label_end) = find_label_end(value, label) else { continue };
            let Some(label_start) = label_start_from_end(value, label, label_end) else { continue };
            if label_start == 0 { continue; }
            let tail = value[label_end..].trim_start();
            let explicit_separator = tail.chars().next().is_some_and(|ch| matches!(ch, ':' | '№' | '#' | '-' | '—' | '–'));
            if !explicit_separator { continue; }
            best = Some(best.map_or(label_start, |current| current.min(label_start)));
        }
    }
    best
}

fn label_start_from_end(value: &str, label: &str, mut end: usize) -> Option<usize> {
    if !value.is_char_boundary(end) { return None; }
    for _ in label.chars() {
        end = value[..end].char_indices().next_back()?.0;
    }
    Some(end)
}
'''
    text = text[:start] + helper + text[end:]
    path.write_text(text, encoding="utf-8")


def patch_field_aliases() -> None:
    path = Path("crates/dokkomplekt-core/src/field_aliases.rs")
    text = path.read_text(encoding="utf-8")
    anchor = '        "person.address" | "patient.address" => "subject.address".into(),\n'
    insert = anchor + '''        "person.age" | "patient.age" => "subject.age".into(),
        "complaints" | "medical.complaints_text" => "medical.complaints".into(),
        "anamnesis.disease" | "disease_anamnesis" => "medical.anamnesis_disease".into(),
        "anamnesis.life" | "life_anamnesis" => "medical.anamnesis_life".into(),
        "status.objective" | "status.somatic" | "somatic_status" => "medical.somatic_status".into(),
        "status.profile" | "status.mental" | "mental_status" => "medical.profile_status".into(),
        "examination.plan" | "examination_plan" => "medical.examination_plan".into(),
        "treatment.result" => "medical.treatment_result".into(),
        "condition.discharge" => "medical.discharge_condition".into(),
        "labs.results" | "labs.block" | "analysis.results" | "analyses.results" => "medical.labs".into(),
        "labs.date" => "medical.labs_date".into(),
        "labs.source" => "medical.labs_source".into(),
        "labs.date_policy" => "medical.labs_date_policy".into(),
'''
    text = replace_once(text, anchor, insert, "canonical donor aliases")
    anchor = '        "subject.address" => &["subject.address", "person.address", "patient.address"],\n'
    insert = anchor + '''        "subject.age" => &["subject.age", "person.age", "patient.age"],
        "medical.complaints" => &["medical.complaints", "complaints", "medical.complaints_text"],
        "medical.anamnesis_disease" => &["medical.anamnesis_disease", "anamnesis.disease", "disease_anamnesis"],
        "medical.anamnesis_life" => &["medical.anamnesis_life", "anamnesis.life", "life_anamnesis"],
        "medical.somatic_status" => &["medical.somatic_status", "status.objective", "status.somatic", "somatic_status"],
        "medical.profile_status" => &["medical.profile_status", "status.profile", "status.mental", "mental_status"],
        "medical.examination_plan" => &["medical.examination_plan", "examination.plan", "examination_plan"],
        "medical.treatment_result" => &["medical.treatment_result", "treatment.result"],
        "medical.discharge_condition" => &["medical.discharge_condition", "condition.discharge"],
        "medical.labs" => &["medical.labs", "labs.results", "labs.block", "analysis.results", "analyses.results"],
        "medical.labs_date" => &["medical.labs_date", "labs.date"],
        "medical.labs_source" => &["medical.labs_source", "labs.source"],
        "medical.labs_date_policy" => &["medical.labs_date_policy", "labs.date_policy"],
'''
    text = replace_once(text, anchor, insert, "storage donor aliases")
    path.write_text(text, encoding="utf-8")


def patch_field_registry() -> None:
    path = Path("crates/dokkomplekt-core/src/field_registry.rs")
    text = path.read_text(encoding="utf-8")
    marker = '        field(\n            "subject.address",\n'
    age = '''        field(
            "subject.age",
            "Возраст",
            DomainKind::Generic,
            false,
            &["age", "Возраст", "patient.age", "person.age"],
        ),
'''
    if marker not in text:
        raise SystemExit("subject.address marker missing")
    text = text.replace(marker, age + marker, 1)

    marker = '        field(\n            "medical.diary_schedule_style",\n'
    clinical = '''        field(
            "medical.complaints", "Жалобы", DomainKind::Medical, false,
            &["complaints", "Жалобы", "Жалобы при поступлении"],
        ),
        field(
            "medical.anamnesis_disease", "Анамнез заболевания", DomainKind::Medical, false,
            &["anamnesis.disease", "disease_anamnesis", "Анамнез заболевания"],
        ),
        field(
            "medical.anamnesis_life", "Анамнез жизни", DomainKind::Medical, false,
            &["anamnesis.life", "life_anamnesis", "Анамнез жизни"],
        ),
        field(
            "medical.profile_status", "Профильный статус", DomainKind::Medical, false,
            &["status.profile", "status.mental", "mental_status", "Профильный статус", "Психический статус"],
        ),
        field(
            "medical.somatic_status", "Соматический / объективный статус", DomainKind::Medical, false,
            &["status.objective", "status.somatic", "somatic_status", "Соматический статус", "Объективный статус"],
        ),
        field(
            "medical.examination_plan", "План обследования", DomainKind::Medical, false,
            &["examination.plan", "examination_plan", "План обследования"],
        ),
        field(
            "medical.treatment_result", "Результат лечения", DomainKind::Medical, false,
            &["treatment.result", "Результат лечения", "Исход лечения"],
        ),
        field(
            "medical.labs_source", "Источник результатов исследований", DomainKind::Medical, false,
            &["labs.source", "Источник анализов"],
        ),
        field(
            "medical.labs_date_policy", "Политика даты исследований", DomainKind::Medical, false,
            &["labs.date_policy", "Политика даты анализов"],
        ),
        field(
            "medical.labs_without", "Без лабораторных исследований", DomainKind::Medical, false,
            &["labs.without", "Без анализов"],
        ),
'''
    if text.count(marker) != 1:
        raise SystemExit("medical diary marker mismatch")
    text = text.replace(marker, clinical + marker, 1)
    text = text.replace(
        '                "discharge.condition",\n                "dischargeCondition",',
        '                "discharge.condition",\n                "condition.discharge",\n                "dischargeCondition",',
        1,
    )
    text = text.replace(
        '                "labs.results",\n                "labResults",',
        '                "labs.results",\n                "labs.block",\n                "analysis.results",\n                "analyses.results",\n                "labResults",',
        1,
    )
    path.write_text(text, encoding="utf-8")


def patch_medical_profile_contract() -> None:
    path = Path("crates/dokkomplekt-core/src/domains/medical.rs")
    text = path.read_text(encoding="utf-8")
    start = text.index("        field_rules: vec![")
    end = text.index("        ],\n    }", start)
    rules = '''        field_rules: vec![
            FieldExtractionRule { field_id: "medical.case_number".into(), aliases: vec!["Номер истории болезни".into(), "История болезни №".into()], required: true },
            FieldExtractionRule { field_id: "subject.name".into(), aliases: vec!["ФИО пациента".into(), "Ф.И.О.".into()], required: false },
            FieldExtractionRule { field_id: "subject.age".into(), aliases: vec!["Возраст".into()], required: false },
            FieldExtractionRule { field_id: "medical.diagnosis".into(), aliases: vec!["Диагноз".into(), "Клинический диагноз".into()], required: true },
            FieldExtractionRule { field_id: "medical.treatment".into(), aliases: vec!["Лечение".into(), "Назначенное лечение".into()], required: false },
            FieldExtractionRule { field_id: "medical.complaints".into(), aliases: vec!["Жалобы".into()], required: false },
            FieldExtractionRule { field_id: "medical.anamnesis_disease".into(), aliases: vec!["Анамнез заболевания".into()], required: false },
            FieldExtractionRule { field_id: "medical.anamnesis_life".into(), aliases: vec!["Анамнез жизни".into()], required: false },
            FieldExtractionRule { field_id: "medical.somatic_status".into(), aliases: vec!["Соматический статус".into(), "Объективный статус".into()], required: false },
            FieldExtractionRule { field_id: "medical.profile_status".into(), aliases: vec!["Профильный статус".into(), "Психический статус".into()], required: false },
            FieldExtractionRule { field_id: "medical.examination_plan".into(), aliases: vec!["План обследования".into()], required: false },
            FieldExtractionRule { field_id: "medical.labs".into(), aliases: vec!["Лабораторные исследования".into(), "Анализы".into()], required: false },
            FieldExtractionRule { field_id: "medical.recommendations".into(), aliases: vec!["Рекомендации".into()], required: false },
            FieldExtractionRule { field_id: "medical.attending_doctor".into(), aliases: vec!["Лечащий врач".into()], required: false },
            FieldExtractionRule { field_id: "medical.department_head".into(), aliases: vec!["Заведующий отделением".into(), "Зав. отделением".into()], required: false },
'''
    text = text[:start] + rules + text[end:]
    path.write_text(text, encoding="utf-8")


def write_tests() -> None:
    path = Path("crates/dokkomplekt-core/tests/donor_medical_source_parity.rs")
    path.write_text('''use dokkomplekt_core::{canonical_field_id_for_domain, parse_source_text, DomainKind};

#[test]
fn compact_primary_populates_donor_clinical_fields_without_bleed() {
    let text = "Первичный осмотр 05.06.2026 История болезни № 12345 Ф.И.О.: Иванов Иван Иванович Возраст: 45 лет Место жительства: г. Нижний Новгород, ул. Пушкина, 1 Место работы: ООО Ромашка Должность: инженер Жалобы: головная боль Анамнез заболевания: заболел вчера Анамнез жизни: рос и развивался нормально Соматический статус: стабилен Профильный статус: ориентирован План обследования: ОАК Лечение: режим, терапия Диагноз: J20 Острый бронхит";
    let (case, _) = parse_source_text(text, 2026);
    for (field, expected) in [
        ("medical.admission_date", "05.06.2026"),
        ("medical.case_number", "12345"),
        ("subject.name", "Иванов Иван Иванович"),
        ("subject.age", "45 лет"),
        ("medical.workplace", "ООО Ромашка"),
        ("medical.position", "инженер"),
        ("medical.complaints", "головная боль"),
        ("medical.anamnesis_disease", "заболел вчера"),
        ("medical.anamnesis_life", "рос и развивался нормально"),
        ("medical.somatic_status", "стабилен"),
        ("medical.profile_status", "ориентирован"),
        ("medical.examination_plan", "ОАК"),
        ("medical.treatment", "режим, терапия"),
        ("medical.diagnosis", "J20 Острый бронхит"),
    ] {
        assert_eq!(case.get(field), Some(expected), "field {field}");
    }
}

#[test]
fn historical_medical_placeholders_resolve_to_current_schema() {
    let medical = Some(&DomainKind::Medical);
    for (legacy, canonical) in [
        ("patient.age", "subject.age"),
        ("complaints", "medical.complaints"),
        ("anamnesis.disease", "medical.anamnesis_disease"),
        ("anamnesis.life", "medical.anamnesis_life"),
        ("status.objective", "medical.somatic_status"),
        ("status.profile", "medical.profile_status"),
        ("examination.plan", "medical.examination_plan"),
        ("treatment.result", "medical.treatment_result"),
        ("condition.discharge", "medical.discharge_condition"),
        ("labs.results", "medical.labs"),
        ("labs.date", "medical.labs_date"),
    ] {
        assert_eq!(canonical_field_id_for_domain(legacy, medical).as_deref(), Some(canonical), "alias {legacy}");
    }
}

#[test]
fn labs_are_explicit_and_never_synthesized() {
    let with_labs = "ПЕРВИЧНЫЙ ОСМОТР\nИстория болезни № 77\nЛабораторные исследования:\nОАК от 01.06.2026: Hb 140 г/л\nДиагноз: J20 Острый бронхит";
    let (case, _) = parse_source_text(with_labs, 2026);
    assert_eq!(case.get("medical.labs"), Some("ОАК от 01.06.2026: Hb 140 г/л"));

    let without_labs = "ПЕРВИЧНЫЙ ОСМОТР\nИстория болезни № 78\nДиагноз: J20 Острый бронхит\nЛечение: режим";
    let (case, _) = parse_source_text(without_labs, 2026);
    assert_eq!(case.get("medical.labs"), None);
    assert_eq!(case.get("medical.labs_date"), None);
}
''', encoding="utf-8")


def patch_inventory() -> None:
    path = Path("docs/LEGACY_MIGRATION_INVENTORY.json")
    data = json.loads(path.read_text(encoding="utf-8"))
    donor = next(item for item in data["donors"] if item["repository"] == "mailsvb2-bot/Dokkomplekt")
    if not any(entry["path"] == "medical_renderer_labs.py" for entry in donor["entries"]):
        donor["entries"].append({
            "path": "medical_renderer_labs.py",
            "status": "migrated-domain-profile",
            "targets": [
                "crates/dokkomplekt-core/src/source_parser.rs",
                "crates/dokkomplekt-core/src/field_registry.rs",
                "crates/dokkomplekt-core/src/field_aliases.rs",
            ],
            "note": "Explicit laboratory results/date/source semantics are migrated without synthetic medical-result generation.",
        })
        donor["entries"].sort(key=lambda entry: entry["path"])
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    patch_source_parser()
    patch_field_aliases()
    patch_field_registry()
    patch_medical_profile_contract()
    write_tests()
    patch_inventory()
