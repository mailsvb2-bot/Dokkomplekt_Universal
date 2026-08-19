use dokkomplekt_core::parse_source_text;

#[test]
fn donor_narrative_treatment_mention_is_not_an_assignment() {
    let text = concat!(
        "Первичный осмотр\n",
        "Пациент: Иванов Иван Иванович\n",
        "Дата поступления: 10.02.2026\n",
        "Диагноз: F32.1 Депрессивный эпизод\n",
        "Анамнез: ранее проходил лечение амбулаторно, эффект частичный."
    );

    let (case, _) = parse_source_text(text, 2026);
    assert_eq!(case.get("medical.treatment"), None);
}
