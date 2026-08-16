//! Profession-aware preparation of repeated records before template rendering.
//!
//! The collection mechanism itself is universal. Domain adapters may derive a
//! collection from already-confirmed semantic data, but they must never invent
//! professional content. This keeps medicine out of the universal renderer
//! while still allowing a medical profile to provide the proven diary rules.

use crate::{
    build_medical_diary_series, template_collection_references, DomainKind,
    MedicalDiarySeriesRequest, SemanticAtom, SemanticCase, SemanticRecord, SeriesCadence,
};
use chrono::{Datelike, Local, NaiveDate};

const MEDICAL_DIARY_COLLECTIONS: [&str; 2] = ["diaries", "medical_diaries"];
const MEDICAL_DIARY_TEXT_COLLECTIONS: [&str; 2] = ["medical_diary_texts", "diary_texts"];
const MIN_STATUS_LEN: usize = 25;
pub const DIARY_SCHEDULE_STYLE: &str = "medical.diary_schedule_style";
pub const DIARY_INTRADAY_RHYTHM: &str = "medical.diary_intraday_rhythm";
pub const DIARY_DAY_START_TIME: &str = "medical.diary_day_start_time";
pub const DIARY_DAY_END_TIME: &str = "medical.diary_day_end_time";

/// Clone `case` and derive only the professional collections that the template
/// actually references. Explicitly supplied collections always win.
///
/// This is deliberately called by the common text/DOCX rendering seam, so the
/// same behaviour is used by manual generation, batch generation and zero-touch
/// automation instead of being reimplemented by each caller.
pub fn prepare_professional_collections(template: &str, case: &SemanticCase) -> SemanticCase {
    let referenced = template_collection_references(template);
    if referenced.is_empty() {
        return case.clone();
    }

    let mut prepared = case.clone();
    if is_medical_case(case) {
        for collection_id in MEDICAL_DIARY_COLLECTIONS {
            if referenced.iter().any(|id| id == collection_id)
                && prepared.collection(collection_id).is_none()
            {
                if let Some(rows) = build_medical_diary_rows(case) {
                    prepared.set_collection(collection_id, rows);
                }
            }
        }
    }
    prepared
}

fn is_medical_case(case: &SemanticCase) -> bool {
    case.active_domains.contains(&DomainKind::Medical)
        || case.has("medical.admission_date")
        || case.has("medical.discharge_date")
        || case.has("medical.diagnosis")
}

fn build_medical_diary_rows(case: &SemanticCase) -> Option<Vec<SemanticRecord>> {
    let admission = case.get("medical.admission_date")?.trim();
    let discharge = case.get("medical.discharge_date")?.trim();
    if admission.is_empty() || discharge.is_empty() {
        return None;
    }

    let default_year = explicit_year(admission)
        .or_else(|| explicit_year(discharge))
        .unwrap_or_else(|| Local::now().year());
    let cadence = resolve_diary_cadence(case).ok()?;
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
        skip_weekdays: Vec::new(),
        excluded_dates: Vec::new(),
        force_final_discharge_entry: true,
    })
    .ok()?;

    let diagnosis = case.get("medical.diagnosis").unwrap_or_default();
    let sources = diary_text_sources(case, diagnosis);
    let final_from_block = case
        .blocks
        .get("medical.diary.final_text")
        .map(|value| clean_diary_source_text(value))
        .filter(|value| !value.is_empty() && !is_source_noise(value));
    let final_from_condition = case
        .get("medical.discharge_condition")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("Состояние при выписке: {value}"));
    let final_text = sources
        .final_text
        .clone()
        .or(final_from_block)
        .or(final_from_condition);

    let mut regular_index = 0usize;
    let rows = entries
        .into_iter()
        .map(|entry| {
            let mut row = SemanticRecord::new();
            row.insert(
                "sequence".into(),
                SemanticAtom::Integer(i64::from(entry.sequence)),
            );
            row.insert("date".into(), SemanticAtom::Date(entry.date.clone()));
            row.insert(
                "offset_days".into(),
                SemanticAtom::Integer(i64::from(entry.offset_days)),
            );
            if let Ok(date) = NaiveDate::parse_from_str(&entry.date, "%d.%m.%Y") {
                row.insert("day".into(), SemanticAtom::Integer(i64::from(date.day())));
                row.insert(
                    "day_number".into(),
                    SemanticAtom::Text(format!("{:02}", date.day())),
                );
                row.insert(
                    "month".into(),
                    SemanticAtom::Integer(i64::from(date.month())),
                );
                row.insert("year".into(), SemanticAtom::Integer(i64::from(date.year())));
            }
            if let Some(time) = entry
                .time
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                row.insert("time".into(), SemanticAtom::Text(time.to_string()));
            }
            row.insert(
                "datetime".into(),
                SemanticAtom::Text(entry.datetime.clone()),
            );
            row.insert(
                "is_final".into(),
                SemanticAtom::Boolean(entry.is_final_discharge_entry),
            );
            if let Some(signature) = entry.signatures.first() {
                row.insert(
                    "treating_physician_signature".into(),
                    SemanticAtom::Text(signature.clone()),
                );
            }
            if let Some(signature) = entry.signatures.get(1) {
                row.insert(
                    "department_head_signature".into(),
                    SemanticAtom::Text(signature.clone()),
                );
            }

            let body = if entry.is_final_discharge_entry {
                final_text.clone()
            } else if sources.regular.is_empty() {
                None
            } else {
                let value = sources.regular[regular_index % sources.regular.len()].clone();
                regular_index += 1;
                Some(value)
            };
            // Deliberately omit `text` when there is no specialist-owned source.
            // A strict template using {{diary.text}} then fails closed instead of
            // silently publishing an empty medical diary.
            if let Some(body) = body.filter(|value| !value.trim().is_empty()) {
                row.insert("text".into(), SemanticAtom::Text(body));
            }
            row
        })
        .collect::<Vec<_>>();
    (!rows.is_empty()).then_some(rows)
}

fn resolve_diary_cadence(case: &SemanticCase) -> Result<Option<SeriesCadence>, String> {
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
        None | Some("каждый день") | Some("каждый день по времени") => {
            None
        }
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
        Some(value) if value.contains("каждый час") || value.contains("1 час") => {
            Rhythm::Minutes(60)
        }
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
        (Some(day_offsets), Rhythm::Minutes(minutes)) => SeriesCadence::DayOffsetsMinuteInterval {
            day_offsets,
            minutes,
        },
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
    regular: Vec<String>,
    final_text: Option<String>,
}

fn diary_text_sources(case: &SemanticCase, diagnosis: &str) -> DiaryTextSources {
    let mut all = Vec::<&SemanticRecord>::new();
    for collection_id in MEDICAL_DIARY_TEXT_COLLECTIONS {
        if let Some(rows) = case.collection(collection_id) {
            all.extend(rows);
        }
    }

    let target = normalize_match(diagnosis);
    let exact = all
        .iter()
        .copied()
        .filter(|row| {
            atom_text(row, "diagnosis")
                .map(|value| normalize_match(&value) == target)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let compatible = if exact.is_empty() {
        unambiguous_compatible_rows(&all, &target)
    } else {
        Vec::new()
    };
    let selected = if !exact.is_empty() {
        exact
    } else if !compatible.is_empty() {
        compatible
    } else {
        // Unscoped rows are reusable within the active medical profile. Rows
        // explicitly assigned to a different diagnosis must never leak across.
        all.into_iter()
            .filter(|row| atom_text(row, "diagnosis").is_none_or(|value| value.trim().is_empty()))
            .collect::<Vec<_>>()
    };

    let mut result = DiaryTextSources::default();
    let mut seen_regular = Vec::<String>::new();
    for row in selected {
        let Some(raw_text) = atom_text(row, "text").or_else(|| atom_text(row, "body")) else {
            continue;
        };
        let text = clean_diary_source_text(&raw_text);
        if text.is_empty() || is_source_noise(&text) {
            continue;
        }
        if record_is_final(row) {
            if result.final_text.is_none() {
                result.final_text = Some(text);
            }
        } else {
            let key = normalize_match(&text);
            if !seen_regular.iter().any(|seen| seen == &key) {
                seen_regular.push(key);
                result.regular.push(text);
            }
        }
    }

    // Persistent profile sources reuse the existing local clause-block store.
    // This keeps storage universal: other professions may introduce their own
    // namespaced sources without a medical database or a second semantic brain.
    let key = source_key(diagnosis);
    if result.regular.is_empty() {
        if let Some(content) = persistent_source(case, "professional.medical.diary.regular.", &key)
        {
            result.regular = split_status_source(content);
        }
    }
    if result.final_text.is_none() {
        result.final_text = persistent_source(case, "professional.medical.diary.final.", &key)
            .map(clean_diary_source_text)
            .filter(|value| !value.is_empty() && !is_source_noise(value));
    }
    result
}

fn unambiguous_compatible_rows<'a>(
    rows: &[&'a SemanticRecord],
    target: &str,
) -> Vec<&'a SemanticRecord> {
    let mut candidates = rows
        .iter()
        .copied()
        .filter_map(|row| {
            let diagnosis = atom_text(row, "diagnosis")?;
            let normalized = normalize_match(&diagnosis);
            diagnosis_compatible(&normalized, target).then_some((normalized, row))
        })
        .collect::<Vec<_>>();
    let mut keys = candidates
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() != 1 {
        return Vec::new();
    }
    candidates.drain(..).map(|(_, row)| row).collect()
}

fn diagnosis_compatible(candidate: &str, target: &str) -> bool {
    let candidate = source_key(candidate);
    let target = source_key(target);
    if candidate.len() < 3 || target.len() < 3 {
        return false;
    }
    candidate.contains(&target)
        || target.contains(&candidate)
        || medical_diary_semantic_compatible(&candidate, &target)
}

/// Medical-profile-only compatibility bridge for donor diary source names.
///
/// Legacy doctors' folders often contain files such as
/// `дневники ВЭ легкая депрессия с датами.docx`, while the primary document
/// contains a formal diagnosis such as `F32.0 Депрессивный эпизод легкой
/// степени`. The UI intentionally stores a deterministic, punctuation-free
/// source key. At this seam we recover only the small, proven semantic bridges
/// from the donor project. This does not change the universal matcher and does
/// not invent diary text. Ambiguity remains fail-closed in `persistent_source`.
#[derive(Debug, serde::Deserialize)]
struct ProfileMatchPack {
    groups: Vec<ProfileMatchGroup>,
}

#[derive(Debug, serde::Deserialize)]
struct ProfileMatchGroup {
    id: String,
    kind: String,
    #[serde(default)]
    exclusive: bool,
    terms: Vec<String>,
}

fn medical_diary_match_pack() -> &'static ProfileMatchPack {
    static PACK: std::sync::OnceLock<ProfileMatchPack> = std::sync::OnceLock::new();
    PACK.get_or_init(|| {
        match serde_json::from_str(include_str!("../data/medical_diary_match_aliases.ru.json")) {
            Ok(pack) => pack,
            Err(_) => ProfileMatchPack { groups: Vec::new() },
        }
    })
}

fn profile_group_matches(value: &str, group: &ProfileMatchGroup) -> bool {
    let normalized = source_key(value);
    group.terms.iter().any(|term| {
        let term = source_key(term);
        !term.is_empty() && normalized.contains(&term)
    })
}

fn matching_profile_groups(value: &str, kind: &str) -> std::collections::BTreeSet<String> {
    medical_diary_match_pack()
        .groups
        .iter()
        .filter(|group| group.kind == kind && profile_group_matches(value, group))
        .map(|group| group.id.clone())
        .collect()
}

/// Apply profile-owned semantic bridges for old diary-source filenames.
///
/// The algorithm is deliberately vocabulary-free: diagnosis families, aliases
/// and severities live in an embedded Medical profile data pack. Other domains
/// therefore never inherit psychiatric terminology or matching rules.
fn medical_diary_semantic_compatible(candidate: &str, target: &str) -> bool {
    let candidate_semantic = matching_profile_groups(candidate, "semantic");
    let target_semantic = matching_profile_groups(target, "semantic");
    if candidate_semantic.is_empty()
        || target_semantic.is_empty()
        || candidate_semantic.is_disjoint(&target_semantic)
    {
        return false;
    }

    for group in medical_diary_match_pack()
        .groups
        .iter()
        .filter(|group| group.kind == "semantic" && group.exclusive)
    {
        if candidate_semantic.contains(&group.id) && !target_semantic.contains(&group.id) {
            return false;
        }
    }

    let candidate_severity = matching_profile_groups(candidate, "severity");
    let target_severity = matching_profile_groups(target, "severity");
    candidate_severity.is_empty()
        || target_severity.is_empty()
        || !candidate_severity.is_disjoint(&target_severity)
}

fn persistent_source<'a>(case: &'a SemanticCase, prefix: &str, key: &str) -> Option<&'a str> {
    let exact = format!("{prefix}{key}");
    if let Some(value) = case.blocks.get(&exact) {
        return Some(value.as_str());
    }
    let mut candidates = case
        .blocks
        .iter()
        .filter_map(|(id, value)| {
            let suffix = id.strip_prefix(prefix)?;
            diagnosis_compatible(suffix, key).then_some((suffix, value.as_str()))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(right.0));
    candidates.dedup_by(|left, right| left.0 == right.0);
    (candidates.len() == 1).then(|| candidates[0].1)
}

fn source_key(value: &str) -> String {
    normalize_match(value)
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

/// Clean one specialist-owned diary text without rewriting its clinical meaning.
///
/// This deliberately ports only the safe donor behaviour: invisible characters,
/// leading date/number/label metadata, template headings and signature lines are
/// removed. Clinical wording itself is left untouched and no ready-made medical
/// text is invented.
fn clean_diary_source_text(content: &str) -> String {
    let mut value = normalize_source_whitespace(content);
    for _ in 0..12 {
        let next = strip_one_metadata_prefix(&value);
        if next == value {
            break;
        }
        value = next;
    }
    trim_leading_separators(&value)
}

fn normalize_source_whitespace(content: &str) -> String {
    content
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '\u{00ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
            )
        })
        .map(|character| match character {
            '\u{00a0}' | '\n' | '\r' | '\t' => ' ',
            other => other,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_one_metadata_prefix(value: &str) -> String {
    let trimmed = value.trim_start();
    if let Some(rest) = strip_label_prefix(trimmed) {
        return trim_leading_separators(rest);
    }
    if let Some(rest) = strip_date_prefix(trimmed) {
        return trim_leading_separators(rest);
    }
    if let Some(rest) = strip_number_prefix(trimmed) {
        return trim_leading_separators(rest);
    }
    trimmed.to_string()
}

fn strip_label_prefix(value: &str) -> Option<&str> {
    let lower = value.to_lowercase().replace('ё', "е");
    for label in [
        "дата",
        "число",
        "номер",
        "запись",
        "дневник",
        "no.",
        "no",
        "n",
        "№",
    ] {
        if !lower.starts_with(label) {
            continue;
        }
        let mut rest = &value[label.len()..];
        rest = rest.trim_start();
        if let Some(after_number_sign) = rest.strip_prefix('№') {
            rest = after_number_sign.trim_start();
        }
        let digit_count = rest
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count();
        if digit_count > 0 {
            rest = &rest[digit_count..];
            rest = rest.trim_start();
        }
        if rest.starts_with([':', '.', '-', '–', '—']) || digit_count > 0 {
            return Some(rest);
        }
    }
    None
}

fn strip_number_prefix(value: &str) -> Option<&str> {
    let digit_count = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    let rest = &value[digit_count..];
    let trimmed = rest.trim_start();
    if trimmed.starts_with(['.', ')', ']', ':', '-', '–', '—']) {
        return Some(trimmed);
    }
    if first_token_is_date(trimmed) {
        return Some(trimmed);
    }
    None
}

fn strip_date_prefix(value: &str) -> Option<&str> {
    let mut end = 0usize;
    for (index, character) in value.char_indices() {
        if character.is_whitespace() {
            break;
        }
        end = index + character.len_utf8();
    }
    if end == 0 || !looks_like_date_token(&value[..end]) {
        return None;
    }
    let mut rest = value[end..].trim_start();
    if let Some(after_year_marker) = rest.strip_prefix("г.") {
        rest = after_year_marker.trim_start();
    } else if let Some(after_year_marker) = rest.strip_prefix('г') {
        rest = after_year_marker.trim_start();
    }
    if let Some(token) = rest.split_whitespace().next() {
        if looks_like_time_token(token) {
            rest = rest[token.len()..].trim_start();
        }
    }
    Some(rest)
}

fn first_token_is_date(value: &str) -> bool {
    value
        .split_whitespace()
        .next()
        .is_some_and(looks_like_date_token)
}

fn looks_like_date_token(token: &str) -> bool {
    if token.is_empty()
        || !token
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | '/' | '-'))
    {
        return false;
    }
    let parts = token
        .split(['.', '/', '-'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|part| !part.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    let first = parts[0].parse::<u32>().ok();
    let second = parts[1].parse::<u32>().ok();
    match (parts.len(), first, second) {
        (3, Some(year), Some(month)) if parts[0].len() == 4 => {
            (1900..=2200).contains(&year) && (1..=12).contains(&month)
        }
        (_, Some(day), Some(month)) => (1..=31).contains(&day) && (1..=12).contains(&month),
        _ => false,
    }
}

fn looks_like_time_token(token: &str) -> bool {
    let Some((hour, minute)) = token
        .trim_matches(|c: char| !c.is_ascii_digit() && c != ':')
        .split_once(':')
    else {
        return false;
    };
    hour.parse::<u32>().is_ok_and(|value| value <= 23)
        && minute.parse::<u32>().is_ok_and(|value| value <= 59)
}

fn trim_leading_separators(value: &str) -> String {
    value
        .trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    ':' | '.' | ';' | ',' | ')' | ']' | '-' | '–' | '—'
                )
        })
        .trim()
        .to_string()
}

fn is_signature_line(value: &str) -> bool {
    let normalized = normalize_match(value);
    normalized.starts_with("лечащий врач")
        || normalized.starts_with("зав отделением")
        || normalized.starts_with("заведующий отделением")
        || normalized.starts_with("заведующая отделением")
}

fn is_source_noise(value: &str) -> bool {
    let normalized = normalize_match(value);
    if normalized.is_empty() || is_signature_line(value) {
        return true;
    }
    if normalized.starts_with("совместный осмотр") {
        return true;
    }
    if matches!(
        normalized.as_str(),
        "дневник наблюдения"
            | "день госпитализации"
            | "число"
            | "дата"
            | "месяц год"
            | "месяц"
            | "год"
    ) {
        return true;
    }
    value.chars().all(|character| {
        character.is_ascii_digit()
            || character.is_whitespace()
            || matches!(character, '.' | '/' | '-')
    })
}

fn looks_like_status(value: &str) -> bool {
    let cleaned = clean_diary_source_text(value);
    cleaned.chars().count() >= MIN_STATUS_LEN && !is_source_noise(&cleaned)
}

fn split_status_source(content: &str) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let candidates = if paragraphs.len() > 1 {
        paragraphs
    } else {
        normalized
            .lines()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
    };

    let mut result = Vec::new();
    let mut seen = Vec::<String>::new();
    for candidate in candidates {
        let cleaned = clean_diary_source_text(candidate);
        if !looks_like_status(&cleaned) {
            continue;
        }
        let key = normalize_match(&cleaned);
        if seen.iter().any(|value| value == &key) {
            continue;
        }
        seen.push(key);
        result.push(cleaned);
    }
    if result.is_empty() {
        let cleaned = clean_diary_source_text(&normalized);
        if looks_like_status(&cleaned) {
            result.push(cleaned);
        }
    }
    result
}

fn record_is_final(row: &SemanticRecord) -> bool {
    match row.get("is_final") {
        Some(SemanticAtom::Boolean(value)) => *value,
        Some(value) => matches!(
            value.as_text().trim().to_lowercase().as_str(),
            "1" | "true" | "да" | "final" | "итоговый"
        ),
        None => atom_text(row, "kind").is_some_and(|value| {
            matches!(
                value.trim().to_lowercase().as_str(),
                "final" | "discharge" | "итоговый" | "выписной"
            )
        }),
    }
}

fn atom_text(row: &SemanticRecord, key: &str) -> Option<String> {
    row.get(key).map(SemanticAtom::as_text)
}

fn normalize_match(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ё', "е")
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn explicit_year(value: &str) -> Option<i32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .filter_map(|part| part.parse::<i32>().ok())
        .find(|year| (1900..=2200).contains(year))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_text_template, SemanticValue, ValueSource};

    fn medical_case() -> SemanticCase {
        let mut case = SemanticCase::default();
        case.active_domains.push(DomainKind::Medical);
        for (field, value) in [
            ("medical.admission_date", "10.05.2026"),
            ("medical.discharge_date", "13.05.2026"),
            ("medical.diagnosis", "F20.0"),
            ("medical.discharge_condition", "улучшение"),
        ] {
            case.values.insert(
                field.into(),
                SemanticValue::new(field, value, ValueSource::UserConfirmed, 1.0),
            );
        }
        case
    }

    fn text_row(text: &str, diagnosis: Option<&str>, final_row: bool) -> SemanticRecord {
        let mut row = SemanticRecord::new();
        row.insert("text".into(), SemanticAtom::Text(text.into()));
        if let Some(diagnosis) = diagnosis {
            row.insert("diagnosis".into(), SemanticAtom::Text(diagnosis.into()));
        }
        if final_row {
            row.insert("is_final".into(), SemanticAtom::Boolean(true));
        }
        row
    }

    #[test]
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
        for (id, value) in [
            (DIARY_DAY_START_TIME, "08:00"),
            (DIARY_DAY_END_TIME, "12:00"),
        ] {
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
        let mut case = medical_case();
        case.set_collection(
            "medical_diary_texts",
            vec![
                text_row("Дневник A", Some("F20.0"), false),
                text_row("Дневник B", Some("F20.0"), false),
                text_row("Выписной дневник", Some("F20.0"), true),
                text_row("Чужой диагноз", Some("F32.0"), false),
            ],
        );
        let template = "{{#each diaries}}{{diary.date}}|{{diary.text}}|{{diary.treating_physician_signature}}|{{diary.department_head_signature}}\n{{/each}}";
        let rendered = render_text_template(template, &case, true);
        assert!(
            rendered.missing_fields.is_empty(),
            "{:?}",
            rendered.missing_fields
        );
        assert!(
            rendered.unknown_fields.is_empty(),
            "{:?}",
            rendered.unknown_fields
        );
        assert!(rendered.output_text.contains("11.05.2026|Дневник A"));
        assert!(rendered.output_text.contains("12.05.2026|Дневник B"));
        assert!(rendered.output_text.contains("13.05.2026|Выписной дневник"));
        assert!(!rendered.output_text.contains("Чужой диагноз"));
        assert_eq!(rendered.output_text.matches("Лечащий врач").count(), 3);
        assert_eq!(
            rendered
                .output_text
                .matches("Заведующий отделением")
                .count(),
            3
        );
    }

    #[test]
    fn missing_specialist_diary_text_fails_closed_in_strict_template() {
        let case = medical_case();
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}} {{diary.text}}{{/each}}",
            &case,
            true,
        );
        assert!(
            !rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty(),
            "strict diary unexpectedly rendered without specialist text: {rendered:?}"
        );
    }

    #[test]
    fn explicit_user_diary_collection_is_never_replaced() {
        let mut case = medical_case();
        let mut row = SemanticRecord::new();
        row.insert("text".into(), SemanticAtom::Text("Ручной дневник".into()));
        case.set_collection("diaries", vec![row]);
        let prepared =
            prepare_professional_collections("{{#each diaries}}{{diary.text}}{{/each}}", &case);
        assert_eq!(
            prepared.collection("diaries").unwrap()[0]["text"].as_text(),
            "Ручной дневник"
        );
    }

    #[test]
    fn unambiguous_parent_diagnosis_source_matches_more_specific_code() {
        let mut case = medical_case();
        case.blocks.insert(
            "professional.medical.diary.regular.f20".into(),
            "Профессиональный статус для родительского кода диагноза.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.final.f20".into(),
            "Итоговый статус родительского кода.".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}\n{{/each}}", &case, true);
        assert!(rendered.output_text.contains("родительского кода"));
        assert!(rendered.missing_fields.is_empty());
    }

    #[test]
    fn ambiguous_partial_diagnosis_sources_are_not_guessed() {
        let mut case = medical_case();
        case.values.get_mut("medical.diagnosis").unwrap().value = "F20".into();
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            "Статус F20.0".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.regular.f201".into(),
            "Статус F20.1".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}{{/each}}", &case, true);
        assert!(!rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty());
    }

    #[test]
    fn persistent_clause_block_sources_feed_medical_diaries() {
        let mut case = medical_case();
        case.blocks.insert(
            "professional.medical.diary.regular.f200".into(),
            "Первый профессиональный статус достаточно длинный для источника.\n\nВторой профессиональный статус также хранится локально.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.final.f200".into(),
            "Подтверждённый специалистом итоговый дневник.".into(),
        );
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}}|{{diary.text}}\n{{/each}}",
            &case,
            true,
        );
        assert!(
            rendered.missing_fields.is_empty(),
            "{:?}",
            rendered.missing_fields
        );
        assert!(rendered
            .output_text
            .contains("Первый профессиональный статус"));
        assert!(rendered
            .output_text
            .contains("Второй профессиональный статус"));
        assert!(rendered
            .output_text
            .contains("Подтверждённый специалистом итоговый дневник"));
    }

    #[test]
    fn donor_wrapped_free_form_diary_filename_matches_formal_diagnosis() {
        let mut case = medical_case();
        case.values.get_mut("medical.diagnosis").unwrap().value =
            "F32.0 Депрессивный эпизод легкой степени".into();
        case.blocks.insert(
            "professional.medical.diary.regular.дневникивелегкаядепрессиясдатами".into(),
            "Профессиональный текст дневника для легкой депрессии, подтвержденный лечащим врачом."
                .into(),
        );
        let rendered = render_text_template(
            "{{#each diaries}}{{diary.date}}|{{diary.text}}\n{{/each}}",
            &case,
            true,
        );
        assert!(rendered.missing_fields.is_empty(), "{rendered:?}");
        assert!(rendered
            .output_text
            .contains("Профессиональный текст дневника для легкой депрессии"));
    }

    #[test]
    fn semantic_diary_source_matching_does_not_guess_between_two_compatible_files() {
        let mut case = medical_case();
        case.values.get_mut("medical.diagnosis").unwrap().value =
            "F32.0 Депрессивный эпизод легкой степени".into();
        case.blocks.insert(
            "professional.medical.diary.regular.дневникилегкаядепрессия".into(),
            "Первый конкурирующий профессиональный текст дневника достаточной длины.".into(),
        );
        case.blocks.insert(
            "professional.medical.diary.regular.депрессиялегкойстепени".into(),
            "Второй конкурирующий профессиональный текст дневника достаточной длины.".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}{{/each}}", &case, true);
        assert!(!rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty());
        assert!(!rendered.output_text.contains("Первый конкурирующий"));
        assert!(!rendered.output_text.contains("Второй конкурирующий"));
    }

    #[test]
    fn semantic_diary_source_matching_rejects_conflicting_severity() {
        let mut case = medical_case();
        case.values.get_mut("medical.diagnosis").unwrap().value =
            "F32.0 Депрессивный эпизод легкой степени".into();
        case.blocks.insert(
            "professional.medical.diary.regular.дневникиумереннаядепрессия".into(),
            "Профессиональный текст для иной степени тяжести, который нельзя подставлять.".into(),
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}{{/each}}", &case, true);
        assert!(!rendered.missing_fields.is_empty() || !rendered.unknown_fields.is_empty());
        assert!(!rendered.output_text.contains("иной степени тяжести"));
    }

    #[test]
    fn donor_style_metadata_signatures_and_duplicates_are_removed_from_status_sources() {
        let source = concat!(
            "Дневник № 12: 11.05.2026 08:30 Состояние стабильное, контакт продуктивный, назначения выполняет.\n\n",
            "11.05.2026 Состояние стабильное, контакт продуктивный, назначения выполняет.\n\n",
            "Лечащий врач __________________ /____________/\n\n",
            "Заведующий отделением __________ /____________/\n\n",
            "Число\n"
        );
        let statuses = split_status_source(source);
        assert_eq!(statuses.len(), 1, "{statuses:?}");
        assert_eq!(
            statuses[0],
            "Состояние стабильное, контакт продуктивный, назначения выполняет."
        );
        assert!(!statuses[0].contains("11.05.2026"));
        assert!(!statuses[0].contains("Лечащий врач"));
    }

    #[test]
    fn explicit_collection_text_is_cleaned_without_changing_clinical_wording() {
        let mut case = medical_case();
        case.set_collection(
            "medical_diary_texts",
            vec![
                text_row(
                    "№1) 11.05.2026 Пациент спокоен, доступен контакту, жалоб не предъявляет.",
                    Some("F20.0"),
                    false,
                ),
                text_row(
                    "13.05.2026 Итоговое состояние устойчивое, рекомендации разъяснены.",
                    Some("F20.0"),
                    true,
                ),
            ],
        );
        let rendered =
            render_text_template("{{#each diaries}}{{diary.text}}\n{{/each}}", &case, true);
        assert!(rendered.missing_fields.is_empty(), "{rendered:?}");
        assert!(rendered
            .output_text
            .contains("Пациент спокоен, доступен контакту, жалоб не предъявляет."));
        assert!(rendered
            .output_text
            .contains("Итоговое состояние устойчивое, рекомендации разъяснены."));
        assert!(!rendered.output_text.contains("№1)"));
        assert!(!rendered.output_text.contains("11.05.2026 Пациент"));
    }

    #[test]
    fn nonmedical_case_does_not_receive_medical_diaries() {
        let case = SemanticCase::default();
        let prepared =
            prepare_professional_collections("{{#each diaries}}{{diary.date}}{{/each}}", &case);
        assert!(prepared.collection("diaries").is_none());
    }
}
