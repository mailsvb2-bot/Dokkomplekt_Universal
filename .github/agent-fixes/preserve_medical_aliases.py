from pathlib import Path

path = Path("crates/dokkomplekt-core/src/source_parser.rs")
text = path.read_text(encoding="utf-8")
replacements = {
    '                "Дата госпитализации",\n': '                "Дата госпитализации",\n                "Поступил",\n                "Поступила",\n',
    '            labels: &["Дата выписки", "Data wypisu"],\n': '            labels: &["Дата выписки", "Выписан", "Выписана", "Data wypisu"],\n',
    '                "Работа",\n': '                "Работа",\n                "Работает",\n',
    '            labels: &["Должность", "Stanowisko", "Zawód", "Zawod"],\n': '            labels: &["Должность", "в должности", "Stanowisko", "Zawód", "Zawod"],\n',
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"medical alias anchor mismatch: {old!r} count={text.count(old)}")
    text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")

test_path = Path("crates/dokkomplekt-core/tests/donor_medical_source_parity.rs")
test = test_path.read_text(encoding="utf-8")
anchor = '''#[test]\nfn historical_medical_placeholders_resolve_to_current_schema() {\n'''
case = '''#[test]\nfn donor_expansion_preserves_preexisting_russian_medical_aliases() {\n    let text = "История болезни № 41\\nПоступил: 01.06.2026\\nВыписан: 10.06.2026\\nРаботает: ООО Ромашка\\nв должности: инженер\\nДиагноз: J20 Острый бронхит";\n    let (case, _) = parse_source_text(text, 2026);\n    assert_eq!(case.get("medical.admission_date"), Some("01.06.2026"));\n    assert_eq!(case.get("medical.discharge_date"), Some("10.06.2026"));\n    assert_eq!(case.get("medical.workplace"), Some("ООО Ромашка"));\n    assert_eq!(case.get("medical.position"), Some("инженер"));\n}\n\n'''
if test.count(anchor) != 1:
    raise SystemExit("donor parity test anchor mismatch")
test_path.write_text(test.replace(anchor, case + anchor, 1), encoding="utf-8")
