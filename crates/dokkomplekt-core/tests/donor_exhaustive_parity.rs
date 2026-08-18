use chrono::NaiveDate;
use dokkomplekt_core::universal_behavior_port::{
    diary_hourly_schedule_from_choice, diary_minute_schedule_from_choice,
};
use dokkomplekt_core::{
    clinical_calendar_diary_schedule, dynamic_epicrisis_base_date, dynamic_epicrisis_dates,
    parse_flexible_date, parse_source_text, search_icd10,
};

fn d(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%d.%m.%Y").unwrap()
}

#[test]
fn donor_calendar_and_intraday_rhythm_contracts_remain_exact() {
    assert_eq!(
        clinical_calendar_diary_schedule(8).day_offsets,
        vec![1, 2, 3, 7, 10, 14, 17, 21]
    );

    let expected = [("2", 240), ("3", 60), ("4", 30), ("5", 15), ("6", 5)];
    for (choice, minutes) in expected {
        assert_eq!(
            diary_minute_schedule_from_choice(choice).minute_offsets,
            vec![minutes],
            "menu choice {choice}"
        );
    }
    assert_eq!(
        diary_minute_schedule_from_choice("45 минут").minute_offsets,
        vec![45]
    );
    assert_eq!(
        diary_minute_schedule_from_choice("2 часа").minute_offsets,
        vec![120]
    );
    assert_eq!(
        diary_hourly_schedule_from_choice("3").unwrap().hour_offsets,
        vec![24]
    );
    assert_eq!(
        diary_hourly_schedule_from_choice("1,2,4,8")
            .unwrap()
            .hour_offsets,
        vec![1, 2, 4, 8]
    );
    assert!(diary_hourly_schedule_from_choice("-1,2").is_err());
}

#[test]
fn donor_date_formats_include_compact_russian_and_polish_word_dates() {
    assert_eq!(
        parse_flexible_date("1126", 2026).as_deref(),
        Some("01.01.2026")
    );
    assert_eq!(
        parse_flexible_date("110626", 2026).as_deref(),
        Some("11.06.2026")
    );
    assert_eq!(
        parse_flexible_date("2 июня 2026", 2026).as_deref(),
        Some("02.06.2026")
    );
    assert_eq!(
        parse_flexible_date("2 czerwca 2026", 2026).as_deref(),
        Some("02.06.2026")
    );
}

#[test]
fn donor_source_parser_keeps_same_line_dates_and_sanitizes_fio_tail() {
    let text = concat!(
        "Первичный осмотр\n",
        "Пациент: Петрова Анна Сергеевна, 1975 г.р.\n",
        "Дата поступления: 10.02.2026. Дата выписки: 20.02.2026.\n",
        "Диагноз: F32.1 Депрессивный эпизод\n",
        "Лечение: наблюдение"
    );
    let (case, _) = parse_source_text(text, 2026);
    assert_eq!(case.get("subject.name"), Some("Петрова Анна Сергеевна"));
    assert_eq!(case.get("medical.admission_date"), Some("10.02.2026"));
    assert_eq!(case.get("medical.discharge_date"), Some("20.02.2026"));
}

#[test]
fn polish_only_medical_source_activates_the_same_canonical_medical_parser() {
    let text = concat!(
        "Karta informacyjna leczenia szpitalnego\n",
        "Pacjent: Anna Kowalska\n",
        "Nr historii choroby: 123/PL\n",
        "Data urodzenia: 04.01.1980\n",
        "Data przyjęcia: 2 czerwca 2026\n",
        "Data wypisu: 12.06.2026\n",
        "Rozpoznanie: K35.8 Ostre zapalenie wyrostka robaczkowego\n",
        "Leczenie: appendektomia, antybiotykoterapia\n",
        "Zalecenia: kontrola w poradni"
    );
    let (case, _) = parse_source_text(text, 2026);
    assert_eq!(case.get("subject.name"), Some("Anna Kowalska"));
    assert_eq!(case.get("medical.case_number"), Some("123/PL"));
    assert_eq!(case.get("subject.birth_date"), Some("04.01.1980"));
    assert_eq!(case.get("medical.admission_date"), Some("02.06.2026"));
    assert_eq!(case.get("medical.discharge_date"), Some("12.06.2026"));
    assert_eq!(
        case.get("medical.diagnosis"),
        Some("K35.8 Ostre zapalenie wyrostka robaczkowego")
    );
    assert_eq!(
        case.get("medical.treatment"),
        Some("appendektomia, antybiotykoterapia")
    );
    assert_eq!(
        case.get("medical.recommendations"),
        Some("kontrola w poradni")
    );
}

#[test]
fn donor_dynamic_epicrisis_schedule_stays_ten_day_anchored_and_before_discharge() {
    let base = dynamic_epicrisis_base_date(d("10.05.2026"), None);
    assert_eq!(base, d("10.05.2026"));
    assert_eq!(
        dynamic_epicrisis_dates(base, Some(d("10.06.2026")), 12),
        vec![d("20.05.2026"), d("01.06.2026"), d("09.06.2026")]
    );
}

#[test]
fn donor_full_icd_catalog_keeps_somatic_and_curated_psychiatric_rows() {
    for code in [
        "A00.0", "C50.9", "E11.9", "I21.0", "J18.9", "K35.8", "N23", "S72.0",
    ] {
        let hits = search_icd10(code, 5);
        assert!(
            hits.iter().any(|row| row.code == code),
            "ICD-10 row {code} must remain available"
        );
    }
    let f = search_icd10("F32.1", 5);
    assert!(f.iter().any(|row| row.code == "F32.1"));
}
