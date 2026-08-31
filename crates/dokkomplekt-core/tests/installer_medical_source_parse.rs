use dokkomplekt_core::parse_source_text;

#[test]
fn installed_medical_source_fixture_keeps_required_values_clean() {
    let text = concat!(
        "Первичный осмотр\n",
        "Ф.И.О.: Петров Пётр Петрович\n",
        "Номер истории болезни: 2222\n",
        "Дата поступления: 26.08.2026\n",
        "Диагноз: F20.0 Параноидная шизофрения\n",
        "Лечение: рисперидон 4 мг/сут\n",
        "Место работы: Новый завод\n",
        "Должность: инженер\n",
        "Лечащий врач __________\n",
        "Заведующий отделением __________\n",
        "ГБУЗ НО «НКЦПЗ» диспансер №2"
    );
    let (case, _) = parse_source_text(text, 2026);
    for (field, expected) in [
        ("subject.name", "Петров Пётр Петрович"),
        ("medical.case_number", "2222"),
        ("medical.admission_date", "26.08.2026"),
        ("medical.treatment", "рисперидон 4 мг/сут"),
        ("medical.workplace", "Новый завод"),
        ("medical.position", "инженер"),
    ] {
        let actual = case
            .value(field)
            .unwrap_or_else(|| panic!("missing {field}"));
        assert_eq!(actual.value, expected, "dirty parsed value for {field}");
    }
    let diagnosis = case.value("medical.diagnosis").expect("diagnosis");
    assert!(diagnosis.value.contains("F20.0"));
    assert!(!diagnosis.value.contains("НКЦПЗ"));
}
