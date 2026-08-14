from pathlib import Path


def read(path):
    return Path(path).read_text(encoding='utf-8')


def write(path, text):
    Path(path).write_text(text, encoding='utf-8')


def replace(path, old, new, count=1):
    text = read(path)
    actual = text.count(old)
    assert actual == count, f'{path}: expected {count} matches, got {actual}: {old[:100]!r}'
    write(path, text.replace(old, new, count))


# ---------------------------------------------------------------------------
# General series engine: selected days + intraday rhythm is a profession-neutral
# capability, not a second medical scheduler.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/record_series.rs'
replace(p,
'    FixedTimes(Vec<String>),\n    MinuteInterval(u32),\n',
'''    FixedTimes(Vec<String>),
    MinuteInterval(u32),
    DayOffsetsFixedTimes {
        day_offsets: Vec<i32>,
        times: Vec<String>,
    },
    DayOffsetsMinuteInterval {
        day_offsets: Vec<i32>,
        minutes: u32,
    },
''')
marker = '''        SeriesCadence::FixedTimes(raw_times) => {
            let times = normalize_times(raw_times)?;
'''
replace(p, marker, '''        SeriesCadence::DayOffsetsFixedTimes { day_offsets, times } => {
            let times = normalize_times(times)?;
            let mut offsets = day_offsets.clone();
            offsets.sort_unstable();
            offsets.dedup();
            for offset in offsets {
                if offset < request.start_offset_days {
                    continue;
                }
                let date = start + Duration::days(i64::from(offset));
                if date < first || date > end || should_skip(date) {
                    continue;
                }
                for time in &times {
                    push_entry(&mut entries, start, date, Some(*time))?;
                }
            }
        }
        SeriesCadence::DayOffsetsMinuteInterval { day_offsets, minutes } => {
            if *minutes == 0 || *minutes > 24 * 60 {
                return Err(SeriesPlanError::InvalidCadence(
                    "интервал должен быть от 1 до 1440 минут".into(),
                ));
            }
            let start_time = parse_time(request.day_start_time.as_deref().unwrap_or("00:00"))?;
            let end_time = parse_time(request.day_end_time.as_deref().unwrap_or("23:59"))?;
            if end_time < start_time {
                return Err(SeriesPlanError::InvalidCadence(
                    "время окончания дня раньше времени начала".into(),
                ));
            }
            let mut offsets = day_offsets.clone();
            offsets.sort_unstable();
            offsets.dedup();
            for offset in offsets {
                if offset < request.start_offset_days {
                    continue;
                }
                let date = start + Duration::days(i64::from(offset));
                if date < first || date > end || should_skip(date) {
                    continue;
                }
                let mut current_time = start_time;
                while current_time <= end_time {
                    push_entry(&mut entries, start, date, Some(current_time))?;
                    let next = NaiveDateTime::new(date, current_time)
                        + Duration::minutes(i64::from(*minutes));
                    if next.date() != date {
                        break;
                    }
                    current_time = next.time();
                }
            }
        }
''' + marker)
replace(p,
'''    #[test]
    fn weekends_and_explicit_dates_can_be_omitted_for_any_profession() {
''',
'''    #[test]
    fn selected_days_can_use_fixed_times_without_expanding_to_other_days() {
        let mut req = request(SeriesCadence::DayOffsetsFixedTimes {
            day_offsets: vec![1, 3],
            times: vec!["08:00".into(), "20:00".into()],
        });
        req.end_date = "05.06.2026".into();
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.datetime.as_str()).collect::<Vec<_>>(),
            vec![
                "02.06.2026 08:00",
                "02.06.2026 20:00",
                "04.06.2026 08:00",
                "04.06.2026 20:00",
            ]
        );
    }

    #[test]
    fn selected_days_can_use_minute_rhythm_without_expanding_to_other_days() {
        let mut req = request(SeriesCadence::DayOffsetsMinuteInterval {
            day_offsets: vec![1, 3],
            minutes: 240,
        });
        req.end_date = "05.06.2026".into();
        req.day_start_time = Some("08:00".into());
        req.day_end_time = Some("12:00".into());
        let plan = build_series_plan(&req).unwrap();
        assert_eq!(
            plan.iter().map(|x| x.datetime.as_str()).collect::<Vec<_>>(),
            vec![
                "02.06.2026 08:00",
                "02.06.2026 12:00",
                "04.06.2026 08:00",
                "04.06.2026 12:00",
            ]
        );
    }

    #[test]
    fn weekends_and_explicit_dates_can_be_omitted_for_any_profession() {
''')

# ---------------------------------------------------------------------------
# Medical adapter: read doctor-confirmed schedule/rhythm from canonical case and
# pass it to the same general series engine used by every profession.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/professional_records.rs'
replace(p,
'    MedicalDiarySeriesRequest, SemanticAtom, SemanticCase, SemanticRecord,\n',
'    MedicalDiarySeriesRequest, SemanticAtom, SemanticCase, SemanticRecord, SeriesCadence,\n')
replace(p,
'const MIN_STATUS_LEN: usize = 25;\n',
'''const MIN_STATUS_LEN: usize = 25;
pub const DIARY_SCHEDULE_STYLE: &str = "medical.diary_schedule_style";
pub const DIARY_INTRADAY_RHYTHM: &str = "medical.diary_intraday_rhythm";
pub const DIARY_DAY_START_TIME: &str = "medical.diary_day_start_time";
pub const DIARY_DAY_END_TIME: &str = "medical.diary_day_end_time";
''')
replace(p,
'''    let entries = build_medical_diary_series(&MedicalDiarySeriesRequest {
        admission_date: admission.to_string(),
        discharge_date: discharge.to_string(),
        default_year,
        confirmed_cadence: None,
        profile_cadence: None,
        day_start_time: None,
        day_end_time: None,
''',
'''    let cadence = resolve_diary_cadence(case).ok()?;
    let day_start_time = case
        .get(DIARY_DAY_START_TIME)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let day_end_time = case
        .get(DIARY_DAY_END_TIME)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if cadence.as_ref().is_some_and(cadence_requires_time_window)
        && (day_start_time.is_none() || day_end_time.is_none())
    {
        return None;
    }
    let entries = build_medical_diary_series(&MedicalDiarySeriesRequest {
        admission_date: admission.to_string(),
        discharge_date: discharge.to_string(),
        default_year,
        confirmed_cadence: cadence,
        profile_cadence: None,
        day_start_time,
        day_end_time,
''')
replace(p,
'#[derive(Default)]\nstruct DiaryTextSources {\n',
'''fn resolve_diary_cadence(case: &SemanticCase) -> Result<Option<SeriesCadence>, String> {
    let schedule = case
        .get(DIARY_SCHEDULE_STYLE)
        .map(normalize_choice)
        .filter(|value| !value.is_empty());
    let rhythm = case
        .get(DIARY_INTRADAY_RHYTHM)
        .map(normalize_choice)
        .filter(|value| !value.is_empty());
    if schedule.is_none() && rhythm.is_none() {
        return Ok(None);
    }

    let day_offsets = match schedule.as_deref() {
        None | Some("каждый день") | Some("каждый день по времени") => None,
        Some(value) if value.replace(' ', "").starts_with("1,2,3,7") => {
            Some(clinical_diary_offsets(3660))
        }
        Some(value) => Some(parse_day_offsets(value)?),
    };

    enum Rhythm {
        Once,
        Minutes(u32),
        Fixed(Vec<String>),
    }
    let rhythm = match rhythm.as_deref() {
        None | Some("один раз в день") => Rhythm::Once,
        Some(value) if value.contains("4 час") => Rhythm::Minutes(240),
        Some(value) if value.contains("каждый час") || value.contains("1 час") => Rhythm::Minutes(60),
        Some(value) if value.contains("30 минут") => Rhythm::Minutes(30),
        Some(value) if value.contains("15 минут") => Rhythm::Minutes(15),
        Some(value) if value.contains("5 минут") => Rhythm::Minutes(5),
        Some(value) if value.contains(':') => Rhythm::Fixed(parse_fixed_times(value)?),
        Some(value) => Rhythm::Minutes(parse_custom_minutes(value)?),
    };

    Ok(Some(match (day_offsets, rhythm) {
        (None, Rhythm::Once) => SeriesCadence::Daily,
        (Some(offsets), Rhythm::Once) => SeriesCadence::DayOffsets(offsets),
        (None, Rhythm::Minutes(minutes)) => SeriesCadence::MinuteInterval(minutes),
        (Some(day_offsets), Rhythm::Minutes(minutes)) => {
            SeriesCadence::DayOffsetsMinuteInterval { day_offsets, minutes }
        }
        (None, Rhythm::Fixed(times)) => SeriesCadence::FixedTimes(times),
        (Some(day_offsets), Rhythm::Fixed(times)) => {
            SeriesCadence::DayOffsetsFixedTimes { day_offsets, times }
        }
    }))
}

fn normalize_choice(value: &str) -> String {
    value.trim().to_lowercase().replace('ё', "е")
}

fn parse_day_offsets(value: &str) -> Result<Vec<i32>, String> {
    let values = value
        .split([',', ';', ' '])
        .filter(|part| !part.trim().is_empty())
        .map(|part| part.trim().parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "график дневников должен содержать номера дней через запятую".to_string())?;
    if values.is_empty() || values.iter().any(|value| *value < 1) {
        return Err("номера дней дневников должны быть положительными".into());
    }
    Ok(values)
}

fn clinical_diary_offsets(max_day: i32) -> Vec<i32> {
    let mut values = vec![1, 2, 3, 7];
    let mut current = 7;
    let mut add_three = true;
    while current < max_day {
        current += if add_three { 3 } else { 4 };
        add_three = !add_three;
        if current <= max_day {
            values.push(current);
        }
    }
    values
}

fn parse_fixed_times(value: &str) -> Result<Vec<String>, String> {
    let values = value
        .split([',', ';', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|part| !part.contains(':')) {
        return Err("время дневников задаётся как ЧЧ:ММ через запятую".into());
    }
    Ok(values)
}

fn parse_custom_minutes(value: &str) -> Result<u32, String> {
    let digits = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    let amount = digits
        .parse::<u32>()
        .map_err(|_| "не удалось определить интервал дневников".to_string())?;
    let minutes = if value.contains("час") {
        amount
            .checked_mul(60)
            .ok_or_else(|| "слишком большой интервал".to_string())?
    } else {
        amount
    };
    if !(1..=1440).contains(&minutes) {
        return Err("интервал дневников должен быть от 1 до 1440 минут".into());
    }
    Ok(minutes)
}

fn cadence_requires_time_window(cadence: &SeriesCadence) -> bool {
    matches!(
        cadence,
        SeriesCadence::MinuteInterval(_) | SeriesCadence::DayOffsetsMinuteInterval { .. }
    )
}

#[derive(Default)]
struct DiaryTextSources {
''')
replace(p,
'''    #[test]
    fn common_renderer_derives_complete_medical_diary_collection() {
''',
'''    #[test]
    fn donor_clinical_schedule_and_intraday_rhythm_reach_real_diary_rows() {
        let mut case = medical_case();
        case.values.insert(
            DIARY_SCHEDULE_STYLE.into(),
            SemanticValue::new(
                DIARY_SCHEDULE_STYLE,
                "1, 2, 3, 7, затем 2 раза в неделю",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        case.values.insert(
            DIARY_INTRADAY_RHYTHM.into(),
            SemanticValue::new(
                DIARY_INTRADAY_RHYTHM,
                "Каждые 4 часа",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        for (id, value) in [(DIARY_DAY_START_TIME, "08:00"), (DIARY_DAY_END_TIME, "12:00")] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        case.set_collection(
            "medical_diary_texts",
            vec![
                text_row("Дневник A", Some("F20.0"), false),
                text_row("Выписной дневник", Some("F20.0"), true),
            ],
        );
        let prepared = prepare_professional_collections(
            "{{#each diaries}}{{diary.datetime}}|{{diary.text}}\n{{/each}}",
            &case,
        );
        let rows = prepared.collection("diaries").expect("diary rows");
        let datetimes = rows
            .iter()
            .filter_map(|row| row.get("datetime"))
            .map(SemanticAtom::as_text)
            .collect::<Vec<_>>();
        assert_eq!(
            datetimes,
            vec![
                "11.05.2026 08:00",
                "11.05.2026 12:00",
                "12.05.2026 08:00",
                "12.05.2026 12:00",
                "13.05.2026 08:00",
                "13.05.2026 12:00",
            ]
        );
        assert_eq!(
            rows.last().and_then(|row| row.get("is_final")),
            Some(&SemanticAtom::Boolean(true))
        );
    }

    #[test]
    fn custom_doctor_selected_days_are_not_inferred_from_template_shape() {
        let mut case = medical_case();
        case.values.insert(
            DIARY_SCHEDULE_STYLE.into(),
            SemanticValue::new(
                DIARY_SCHEDULE_STYLE,
                "1, 3",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let cadence = resolve_diary_cadence(&case).unwrap().unwrap();
        assert_eq!(cadence, SeriesCadence::DayOffsets(vec![1, 3]));
    }

    #[test]
    fn intraday_interval_without_explicit_window_fails_closed() {
        let mut case = medical_case();
        case.values.insert(
            DIARY_INTRADAY_RHYTHM.into(),
            SemanticValue::new(
                DIARY_INTRADAY_RHYTHM,
                "Каждые 5 минут",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        assert!(build_medical_diary_rows(&case).is_none());
    }

    #[test]
    fn common_renderer_derives_complete_medical_diary_collection() {
''')

# ---------------------------------------------------------------------------
# Popup profile: schedule questions are runtime controls for the diary role. They
# are not physical template placeholders and do not leak into other professions.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/popup_profiles.rs'
replace(p,
'use crate::{\n    canonical_storage_field_id, is_valid_field_id, title_for_field, DocumentTemplateSpec,\n',
'use crate::{\n    canonical_storage_field_id, is_valid_field_id, title_for_field, DocumentTemplateSpec,\n')
# Bring the IDs from the professional adapter: one shared semantic namespace.
replace(p,
'use chrono::Local;\n',
'''use chrono::Local;
use crate::professional_records::{
    DIARY_DAY_END_TIME, DIARY_DAY_START_TIME, DIARY_INTRADAY_RHYTHM, DIARY_SCHEDULE_STYLE,
};
''')
replace(p,
'''            if id == "medical.icd10" || id == "medical.diagnosis_code" {
                config.input_kind = PromptInputKind::Icd10;
            }
''',
'''            if id == "medical.icd10" || id == "medical.diagnosis_code" {
                config.input_kind = PromptInputKind::Icd10;
            }
            if id == DIARY_SCHEDULE_STYLE {
                config.input_kind = PromptInputKind::Select;
                config.options = vec![
                    "Каждый день".into(),
                    "1, 2, 3, 7, затем 2 раза в неделю".into(),
                    "Каждый день по времени".into(),
                ];
                config.allow_custom_option = true;
                config.default_value = Some("Каждый день".into());
                config.help_text = Some(
                    "Можно ввести свои дни, например: 1, 4, 9. График задаёт специалист, а не количество строк в шаблоне".into(),
                );
            }
            if id == DIARY_INTRADAY_RHYTHM {
                config.input_kind = PromptInputKind::Select;
                config.options = vec![
                    "Один раз в день".into(),
                    "Каждые 4 часа".into(),
                    "Каждый час".into(),
                    "Каждые 30 минут".into(),
                    "Каждые 15 минут".into(),
                    "Каждые 5 минут".into(),
                ];
                config.allow_custom_option = true;
                config.default_value = Some("Один раз в день".into());
                config.help_text = Some(
                    "Можно ввести свой интервал (например, 90 минут) или список времени 08:00, 20:00".into(),
                );
            }
            if matches!(id, DIARY_DAY_START_TIME | DIARY_DAY_END_TIME) {
                config.input_kind = PromptInputKind::Text;
                config.help_text = Some(
                    "ЧЧ:ММ. Нужен для ритма в минутах/часах; без явных границ внутридневная серия не создаётся".into(),
                );
            }
''')
replace(p,
'''            if matches!(plan.role, MedicalDocumentRole::DischargeEpicrisis) {
                add("medical.discharge_condition", false);
                add("medical.recommendations", false);
            }
''',
'''            if matches!(plan.role, MedicalDocumentRole::DischargeEpicrisis) {
                add("medical.discharge_condition", false);
                add("medical.recommendations", false);
            }
            if matches!(plan.role, MedicalDocumentRole::Diary) {
                add(DIARY_SCHEDULE_STYLE, false);
                add(DIARY_INTRADAY_RHYTHM, false);
                add(DIARY_DAY_START_TIME, false);
                add(DIARY_DAY_END_TIME, false);
            }
''')
replace(p,
'        "medical.treatment" => 120,\n',
'''        "medical.treatment" => 120,
        DIARY_SCHEDULE_STYLE => 121,
        DIARY_INTRADAY_RHYTHM => 122,
        DIARY_DAY_START_TIME => 123,
        DIARY_DAY_END_TIME => 124,
''')
# Export the profile-owned runtime controls for the generic workflow planner.
insert = 'fn validation_hint_for(field_id: &str, kind: PromptInputKind) -> Option<String> {\n'
replace(p, insert, '''pub fn profession_runtime_control_fields(
    category: &DomainKind,
    role_id: &str,
) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    if matches!(category, DomainKind::Medical)
        && matches!(
            MedicalDocumentRole::from_role_id(role_id),
            MedicalDocumentRole::Diary
        )
    {
        fields.extend([
            DIARY_SCHEDULE_STYLE.to_string(),
            DIARY_INTRADAY_RHYTHM.to_string(),
            DIARY_DAY_START_TIME.to_string(),
            DIARY_DAY_END_TIME.to_string(),
        ]);
    }
    fields
}

fn validation_hint_for(field_id: &str, kind: PromptInputKind) -> Option<String> {
''')
replace(p,
'''    #[test]
    fn role_scoped_protocol_dates_link_only_to_their_own_commission() {
''',
'''    #[test]
    fn diary_runtime_controls_are_profile_scoped_and_profession_safe() {
        let medical = profession_runtime_control_fields(&DomainKind::Medical, "diaries");
        assert!(medical.contains(DIARY_SCHEDULE_STYLE));
        assert!(medical.contains(DIARY_INTRADAY_RHYTHM));
        assert!(medical.contains(DIARY_DAY_START_TIME));
        assert!(medical.contains(DIARY_DAY_END_TIME));
        assert!(profession_runtime_control_fields(&DomainKind::Hr, "diaries").is_empty());
        assert!(profession_runtime_control_fields(&DomainKind::Legal, "diaries").is_empty());
        assert!(profession_runtime_control_fields(&DomainKind::Generic, "diaries").is_empty());
    }

    #[test]
    fn role_scoped_protocol_dates_link_only_to_their_own_commission() {
''')

# Generic workflow planner asks profile-declared runtime controls even though they
# are not rendered placeholders. Other profile defaults remain filtered out.
p = 'crates/dokkomplekt-core/src/workflow_engine.rs'
replace(p,
'    canonical_storage_field_id, effective_popup_fields, is_valid_field_id, popup_config_for_field,\n',
'    canonical_storage_field_id, effective_popup_fields, is_valid_field_id, popup_config_for_field,\n    profession_runtime_control_fields,\n')
replace(p,
'''fn selected_document_fields(document: &DocumentTemplateSpec) -> BTreeSet<String> {
    let explicit_popup_fields = document
''',
'''fn selected_document_fields(document: &DocumentTemplateSpec) -> BTreeSet<String> {
    let runtime_controls = profession_runtime_control_fields(&document.category, &document.role_id);
    let explicit_popup_fields = document
''')
replace(p,
'''        .chain(document.required_fields.iter())
        .chain(explicit_popup_fields)
        .filter(|field_id| is_valid_field_id(field_id))
''',
'''        .chain(document.required_fields.iter())
        .chain(explicit_popup_fields)
        .chain(runtime_controls.iter())
        .filter(|field_id| is_valid_field_id(field_id))
''')

# ---------------------------------------------------------------------------
# Human-readable semantic field titles.
# ---------------------------------------------------------------------------
p = 'crates/dokkomplekt-core/src/field_registry.rs'
replace(p,
'''        field(
            "medical.discharge_date",
            "Дата выписки",
            DomainKind::Medical,
            false,
            &["discharge.date", "dischargeDate", "Дата выписки"],
        ),
''',
'''        field(
            "medical.discharge_date",
            "Дата выписки",
            DomainKind::Medical,
            false,
            &["discharge.date", "dischargeDate", "Дата выписки"],
        ),
        field(
            "medical.diary_schedule_style",
            "График дневников",
            DomainKind::Medical,
            false,
            &["График дневников", "Режим дневников"],
        ),
        field(
            "medical.diary_intraday_rhythm",
            "Ритм записей в течение дня",
            DomainKind::Medical,
            false,
            &["Ритм дневников", "Интервал дневников"],
        ),
        field(
            "medical.diary_day_start_time",
            "Начало времени дневников",
            DomainKind::Medical,
            false,
            &["Начало записей", "Время начала дневников"],
        ),
        field(
            "medical.diary_day_end_time",
            "Окончание времени дневников",
            DomainKind::Medical,
            false,
            &["Окончание записей", "Время окончания дневников"],
        ),
''')

# Thin TS mirror of the generic series enum.
p = 'src/lib/types.ts'
replace(p,
"  | { kind: 'minute_interval'; value: number };\n",
"""  | { kind: 'minute_interval'; value: number }
  | { kind: 'day_offsets_fixed_times'; value: { day_offsets: number[]; times: string[] } }
  | { kind: 'day_offsets_minute_interval'; value: { day_offsets: number[]; minutes: number } };
""")

print('diary donor parity patch applied')
