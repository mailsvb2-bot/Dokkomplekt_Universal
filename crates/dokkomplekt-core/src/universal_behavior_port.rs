//! Universal behavior port from the legacy Python v1.5.x project.
//!
//! This is executable Rust domain code, not a report.  It covers the old universal constructor
//! semantics that were outside the first medical-only port: generic accounting/HR scanning,
//! field aliases, dynamic button management, semantic date conflict handling, diary schedule
//! popup choices, and intake-agent single-instance decisions.

use crate::label_search::find_label_end;
use crate::{
    analyze_template_text, canonical_storage_field_id, is_valid_field_id, merge_value,
    parse_flexible_date, render_text_template, DocumentTemplateSpec, RenderResult, SemanticCase,
    SemanticValue, ValueSource,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimaryDirection {
    TransactionRecord,
    RoleRecord,
    ClinicalExtension,
    GenericRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniversalScan {
    pub primary_direction: PrimaryDirection,
    pub case_data: SemanticCase,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstructionContract {
    pub primary_direction: PrimaryDirection,
    pub ready_document_ids: Vec<String>,
    pub blocked_document_ids: Vec<String>,
    pub missing_by_document: BTreeMap<String, Vec<String>>,
    pub human_report: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticDateStore {
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateConflict {
    pub key: String,
    pub label: String,
    pub existing: String,
    pub candidate: String,
    pub source_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaryScheduleSpec {
    pub mode: String,
    pub day_offsets: Vec<i32>,
    pub hour_offsets: Vec<i32>,
    pub minute_offsets: Vec<i32>,
    pub confidence: i32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSignatureInput {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
    pub ctime: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchDecision {
    pub should_launch: bool,
    pub reason: String,
}

pub const UNIVERSAL_BEHAVIOR_PORT_VERSION: &str = "v6.0-rust-port";
pub const LAUNCH_COOLDOWN_SECONDS: f32 = 3.0;
pub const PENDING_RETRY_SECONDS: f32 = 12.0;

pub fn normalize_field_id(field: &str) -> String {
    let migrated = match field.trim() {
        "patient.name" | "fio" => "subject.name",
        "case.number" => "document.number",
        "case_number" => "medical.case_number",
        "commission_date" => "commission.date",
        "vk_protocol_date" => "vk_mse.protocol_date",
        "sick_leave_vk_commission_date" => "sick_leave_vk.commission_date",
        "expert_sick_leave_from" => "expert.sick_leave_from",
        "rvk_act_number" => "rvk.act_number",
        "discharge_date" => "medical.discharge_date",
        "admission_date" => "medical.admission_date",
        other => other,
    };
    canonical_storage_field_id(migrated)
}

pub fn case_get<'a>(case: &'a SemanticCase, field: &str) -> Option<&'a str> {
    let normalized = normalize_field_id(field);
    if let Some(v) = case.get(&normalized) {
        return Some(v);
    }
    match normalized.as_str() {
        "subject.name" => case.get("patient.fio"),
        "document.number" => case
            .get("case.number")
            .or_else(|| case.get("medical.case_number")),
        _ => None,
    }
}

pub fn case_set(
    case: &mut SemanticCase,
    field: &str,
    value: &str,
    source: ValueSource,
    confidence: f32,
) {
    let normalized = normalize_field_id(field);
    put(case, &normalized, value, source, confidence);
    if field == "case_number" {
        put(case, "medical.case_number", value, source, confidence);
    }
}

pub fn scan_universal_text(text: &str, default_year: i32) -> UniversalScan {
    let mut case = SemanticCase::default();
    let mut warnings = Vec::new();
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = joined.to_lowercase();
    if let Some((number, date)) =
        find_number_after(&joined, "Счёт").or_else(|| find_number_after(&joined, "Счет"))
    {
        case_set(
            &mut case,
            "document.number",
            &number,
            ValueSource::Scanner,
            0.9,
        );
        case_set(
            &mut case,
            "invoice.number",
            &number,
            ValueSource::Scanner,
            0.9,
        );
        if let Some(d) = date.and_then(|d| parse_flexible_date(&d, default_year)) {
            case_set(&mut case, "invoice.date", &d, ValueSource::Scanner, 0.9);
        }
    }
    if let Some((number, date)) = find_number_after(&joined, "Приказ") {
        case_set(
            &mut case,
            "document.number",
            &number,
            ValueSource::Scanner,
            0.9,
        );
        case_set(
            &mut case,
            "order.number",
            &number,
            ValueSource::Scanner,
            0.9,
        );
        if let Some(d) = date.and_then(|d| parse_flexible_date(&d, default_year)) {
            case_set(&mut case, "order.date", &d, ValueSource::Scanner, 0.9);
        }
    }
    if let Some(case_no) = labeled_value(&joined, &["История болезни №", "ИБ №", "и/б №"])
    {
        if looks_like_person_name(&case_no) {
            warnings.push("Номер документа похож на ФИО и не принят автоматически.".into());
        } else {
            case_set(
                &mut case,
                "medical.case_number",
                &case_no,
                ValueSource::Scanner,
                0.88,
            );
            case_set(
                &mut case,
                "document.number",
                &case_no,
                ValueSource::Scanner,
                0.75,
            );
        }
    }
    for (field, aliases) in scanner_rules() {
        if let Some(mut value) = labeled_value(&joined, aliases) {
            if field.ends_with(".date") || field.ends_with("_date") {
                value = parse_flexible_date(&value, default_year).unwrap_or(value);
            }
            case_set(&mut case, field, &value, ValueSource::Scanner, 0.82);
        }
    }
    if let Some(diag) = labeled_value(&joined, &["Диагноз"]) {
        if let Some(code) = normalize_icd10(&diag) {
            case_set(
                &mut case,
                "diagnosis.icd10",
                &code,
                ValueSource::Scanner,
                0.92,
            );
        }
        let main = remove_initial_icd10(&diag);
        if !main.is_empty() {
            case_set(
                &mut case,
                "diagnosis.main",
                &main,
                ValueSource::Scanner,
                0.86,
            );
        }
        case_set(
            &mut case,
            "medical.diagnosis",
            &diag,
            ValueSource::Scanner,
            0.86,
        );
    }
    let direction = if case.get("invoice.number").is_some()
        || lower.contains("инн")
        || lower.contains("кпп")
        || lower.contains("к оплате")
        || lower.contains("сумма")
    {
        PrimaryDirection::TransactionRecord
    } else if case.get("order.number").is_some()
        || lower.contains("сотрудник")
        || lower.contains("подразделение")
    {
        PrimaryDirection::RoleRecord
    } else if case.get("medical.case_number").is_some()
        || lower.contains("диагноз")
        || lower.contains("дата поступления")
    {
        PrimaryDirection::ClinicalExtension
    } else {
        PrimaryDirection::GenericRecord
    };
    UniversalScan {
        primary_direction: direction,
        case_data: case,
        warnings,
    }
}

pub fn build_construction_contract(
    docs: &[DocumentTemplateSpec],
    case: &SemanticCase,
    direction: PrimaryDirection,
) -> ConstructionContract {
    let mut ready = Vec::new();
    let mut blocked = Vec::new();
    let mut missing_by_document = BTreeMap::new();
    for doc in docs {
        let mut missing = Vec::new();
        for field in doc.required_fields.iter().filter(|f| is_valid_field_id(f)) {
            if case_get(case, field).is_none() {
                missing.push(field.clone());
            }
        }
        for field in doc.placeholders.iter().filter(|f| !is_valid_field_id(f)) {
            missing.push(format!("unsafe:{field}"));
        }
        if missing.is_empty() || doc.is_static_copy {
            ready.push(doc.id.clone());
        } else {
            blocked.push(doc.id.clone());
            missing_by_document.insert(doc.id.clone(), missing);
        }
    }
    let human_report = if blocked.is_empty() {
        format!(
            "Готово документов: {}; направление: {:?}",
            ready.len(),
            direction
        )
    } else {
        format!("Заблокированы документы: {}", blocked.join(", "))
    };
    ConstructionContract {
        primary_direction: direction,
        ready_document_ids: ready,
        blocked_document_ids: blocked,
        missing_by_document,
        human_report,
    }
}

pub fn rename_document_button(
    docs: &[DocumentTemplateSpec],
    document_id: &str,
    new_label: &str,
) -> Option<DocumentTemplateSpec> {
    let doc = docs.iter().find(|d| d.id == document_id)?;
    let taken = docs
        .iter()
        .filter(|d| d.id != document_id)
        .map(|d| d.button_label.as_str())
        .collect::<BTreeSet<_>>();
    let base = clean_label(new_label).unwrap_or_else(|| doc.button_label.clone());
    let mut label = base.clone();
    let mut idx = 2;
    while taken.contains(label.as_str()) {
        label = format!("{} ({idx})", base);
        idx += 1;
    }
    let mut out = doc.clone();
    out.button_label = label;
    Some(out)
}

pub fn semantic_date_key_from_prompt(title: &str, prompt: &str) -> String {
    let full = format!("{} {}", title, prompt).to_lowercase();
    if full.contains("больничн") && full.contains("комисс") {
        return "sick_leave_vk.commission_date".into();
    }
    if full.contains("больничн") && full.contains("протокол") {
        return "sick_leave_vk.protocol_date".into();
    }
    if full.contains("больничн") && (full.contains("какого") || full.contains("с числа"))
    {
        return "expert.sick_leave_from".into();
    }
    if full.contains("совмест") || full.contains("комисс") {
        return "commission.date".into();
    }
    if full.contains("мсэ") && full.contains("протокол") {
        return "vk_mse.protocol_date".into();
    }
    if full.contains("мсэ") {
        return "vk_mse.date".into();
    }
    if full.contains("анализ") {
        return "labs.explicit_date".into();
    }
    if full.contains("выписк") {
        return "medical.discharge_date".into();
    }
    normalize_field_id(prompt)
}

pub fn apply_semantic_date(
    store: &mut SemanticDateStore,
    key: &str,
    raw_value: &str,
    default_year: i32,
) -> Result<(), DateConflict> {
    let normalized = normalize_date_key(key);
    let candidate = parse_flexible_date(raw_value, default_year)
        .unwrap_or_else(|| raw_value.trim().to_string());
    if let Some(existing) = store.values.get(&normalized) {
        if existing != &candidate {
            return Err(DateConflict {
                key: normalized.clone(),
                label: date_label(&normalized).into(),
                existing: existing.clone(),
                candidate,
                source_label: key.into(),
            });
        }
    }
    store.values.insert(normalized, candidate);
    Ok(())
}

pub fn confirm_semantic_date(
    store: &mut SemanticDateStore,
    conflict: &DateConflict,
    accept: bool,
) -> bool {
    if accept {
        store
            .values
            .insert(conflict.key.clone(), conflict.candidate.clone());
        true
    } else {
        false
    }
}

pub fn default_calendar_diary_schedule(limit: i32) -> DiaryScheduleSpec {
    DiaryScheduleSpec {
        mode: "daily".into(),
        day_offsets: (1..=limit).collect(),
        hour_offsets: vec![],
        minute_offsets: vec![],
        confidence: 1,
        source: "popup_every_day".into(),
    }
}

pub fn clinical_calendar_diary_schedule(limit: usize) -> DiaryScheduleSpec {
    let mut out = vec![1, 2, 3, 7];
    let steps = [3, 4];
    let mut i = 0;
    while out.len() < limit {
        out.push(out[out.len() - 1] + steps[i % 2]);
        i += 1;
    }
    out.truncate(limit);
    DiaryScheduleSpec {
        mode: "daily".into(),
        day_offsets: out,
        hour_offsets: vec![],
        minute_offsets: vec![],
        confidence: 1,
        source: "popup_1_2_3_day".into(),
    }
}

pub fn diary_minute_schedule_from_choice(choice: &str) -> DiaryScheduleSpec {
    let text = choice.trim().to_lowercase().replace('ё', "е");
    let compact = text.replace(' ', "");
    let minutes = match text.as_str() {
        "2" => 240,
        "3" => 60,
        "4" => 30,
        "5" => 15,
        "6" => 5,
        _ if compact == "4часа" || compact == "каждые4часа" => 240,
        _ if compact == "1час" || compact == "каждыйчас" => 60,
        _ if compact.contains("30") => 30,
        _ if compact.contains("15") => 15,
        _ if compact.contains("5мин") => 5,
        _ => 0,
    };
    if minutes == 0 {
        DiaryScheduleSpec {
            mode: "daily".into(),
            day_offsets: vec![],
            hour_offsets: vec![],
            minute_offsets: vec![],
            confidence: 1,
            source: "popup_one_per_day".into(),
        }
    } else {
        DiaryScheduleSpec {
            mode: "hourly".into(),
            day_offsets: vec![],
            hour_offsets: vec![],
            minute_offsets: vec![minutes],
            confidence: 1,
            source: "popup_intraday_minute_rhythm".into(),
        }
    }
}

pub fn candidate_signature(input: CandidateSignatureInput) -> String {
    format!(
        "{}|{}|{}|{}",
        input.path.replace('\\', "/").to_lowercase(),
        input.size,
        input.mtime,
        input.ctime
    )
}

pub fn decide_agent_launch(
    foreground_gui_present: bool,
    runtime_active: bool,
    pending_signature: Option<&str>,
    candidate_signature: &str,
    cooldown_seconds: f32,
    seconds_since_last_launch: f32,
) -> LaunchDecision {
    if foreground_gui_present || runtime_active {
        return LaunchDecision {
            should_launch: false,
            reason: "foreground_gui_or_runtime_lock_present".into(),
        };
    }
    if pending_signature == Some(candidate_signature) {
        return LaunchDecision {
            should_launch: false,
            reason: "same_pending_signature".into(),
        };
    }
    if seconds_since_last_launch < cooldown_seconds {
        return LaunchDecision {
            should_launch: false,
            reason: "cooldown".into(),
        };
    }
    LaunchDecision {
        should_launch: true,
        reason: "new_actionable_folder_interaction".into(),
    }
}

pub fn render_universal_template(
    template: &str,
    case: &SemanticCase,
    strict: bool,
) -> RenderResult {
    render_text_template(template, case, strict)
}

pub fn make_document_from_text(id: &str, path: &str, text: &str) -> DocumentTemplateSpec {
    let analysis = analyze_template_text(text);
    crate::create_document_spec(id, path, &analysis, None)
}

fn put(case: &mut SemanticCase, field: &str, value: &str, source: ValueSource, confidence: f32) {
    if !value.trim().is_empty() {
        let canonical = canonical_storage_field_id(field);
        merge_value(
            case,
            SemanticValue::new(&canonical, value.trim(), source, confidence),
        );
    }
}
fn scanner_rules() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("subject.name", &["Клиент", "Сотрудник", "Пациент", "ФИО"]),
        ("org.inn", &["ИНН"]),
        ("org.kpp", &["КПП"]),
        ("amount.total", &["К оплате", "Сумма", "Итого"]),
        ("employee.position", &["Должность"]),
        ("employee.department", &["Подразделение"]),
        (
            "medical.admission_date",
            &["Дата поступления", "Дата госпитализации"],
        ),
        ("medical.discharge_date", &["Дата выписки"]),
    ]
}
fn labeled_value(text: &str, aliases: &[&str]) -> Option<String> {
    for line in text.lines() {
        for alias in aliases {
            if let Some(value_start) = find_label_end(line, alias) {
                let value = line[value_start..]
                    .trim_start_matches([':', ' ', '№', '-', '—'])
                    .trim();
                if !value.is_empty() {
                    return Some(clean_scanner_value(value));
                }
            }
        }
    }
    None
}
fn find_number_after(text: &str, marker: &str) -> Option<(String, Option<String>)> {
    for line in text.lines() {
        if !line.to_lowercase().contains(&marker.to_lowercase()) {
            continue;
        }
        let after = line
            .split('№')
            .nth(1)
            .or_else(|| line.split(marker).nth(1))?
            .trim();
        let mut parts = after.split_whitespace();
        let number = parts
            .next()?
            .trim_matches(|c: char| c == ':' || c == '-' || c == '—' || c == '№')
            .to_string();
        let date = line
            .split(" от ")
            .nth(1)
            .and_then(|tail| tail.split_whitespace().next())
            .map(|s| s.to_string());
        return Some((number, date));
    }
    None
}
fn clean_scanner_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| c == '.' || c == ',' || c == ';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn clean_label(value: &str) -> Option<String> {
    let label = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}
fn looks_like_person_name(value: &str) -> bool {
    let words = value.split_whitespace().collect::<Vec<_>>();
    (2..=4).contains(&words.len())
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
        && !value.chars().any(|c| c.is_ascii_digit())
}
fn normalize_icd10(value: &str) -> Option<String> {
    let chars: Vec<char> = value.chars().collect();
    for i in 0..chars.len().saturating_sub(2) {
        if chars[i].is_alphabetic()
            && chars.get(i + 1)?.is_ascii_digit()
            && chars.get(i + 2)?.is_ascii_digit()
        {
            let mut code = format!(
                "{}{}{}",
                chars[i].to_ascii_uppercase(),
                chars[i + 1],
                chars[i + 2]
            );
            if chars.get(i + 3) == Some(&'.') || chars.get(i + 3) == Some(&',') {
                if let Some(d) = chars.get(i + 4).filter(|c| c.is_ascii_digit()) {
                    code.push('.');
                    code.push(*d);
                }
            }
            return Some(code);
        }
    }
    None
}
fn remove_initial_icd10(value: &str) -> String {
    if let Some(code) = normalize_icd10(value) {
        value.replacen(&code, "", 1).trim().to_string()
    } else {
        value.trim().to_string()
    }
}
fn normalize_date_key(key: &str) -> String {
    match key.trim() {
        "discharge.date" | "discharge_date" => "medical.discharge_date".into(),
        "commission_date" => "commission.date".into(),
        "vk_protocol_date" => "vk_mse.protocol_date".into(),
        "sick_leave_vk_commission_date" => "sick_leave_vk.commission_date".into(),
        "expert_sick_leave_from" => "expert.sick_leave_from".into(),
        "labs_explicit_date" => "labs.explicit_date".into(),
        other => other.to_string(),
    }
}
fn date_label(key: &str) -> &'static str {
    match key {
        "commission.date" => "Дата совместного осмотра",
        "vk_mse.protocol_date" => "Дата протокола ВК на МСЭ",
        "sick_leave_vk.commission_date" => "Дата проведения комиссии",
        "expert.sick_leave_from" => "С какого числа больничный",
        "labs.explicit_date" => "Дата анализов",
        "medical.discharge_date" => "Дата выписки",
        _ => "Дата",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounting_scan_contract() {
        let scan = scan_universal_text("Счёт № INV-151 от 06.07.2026\nКлиент: ООО Север\nИНН: 1234567890 КПП: 123456789\nК оплате: 151 000 руб", 2026);
        assert_eq!(scan.primary_direction, PrimaryDirection::TransactionRecord);
        assert_eq!(case_get(&scan.case_data, "invoice.number"), Some("INV-151"));
        assert_eq!(case_get(&scan.case_data, "subject.name"), Some("ООО Север"));
    }
    #[test]
    fn hr_scan_contract() {
        let scan = scan_universal_text(
            "Приказ № HR-77 от 01.07.2026\nСотрудник: Петров Пётр Петрович\nДолжность: бухгалтер",
            2026,
        );
        assert_eq!(scan.primary_direction, PrimaryDirection::RoleRecord);
        assert_eq!(case_get(&scan.case_data, "order.number"), Some("HR-77"));
    }
    #[test]
    fn semantic_date_conflict_contract() {
        let mut s = SemanticDateStore::default();
        assert!(apply_semantic_date(&mut s, "commission_date", "11.06.2026", 2026).is_ok());
        assert!(apply_semantic_date(&mut s, "commission_date", "12.06.2026", 2026).is_err());
    }
    #[test]
    fn diary_menu_four_is_thirty_minutes() {
        assert_eq!(
            diary_minute_schedule_from_choice("4").minute_offsets,
            vec![30]
        );
    }
    #[test]
    fn signature_includes_ctime() {
        let a = candidate_signature(CandidateSignatureInput {
            path: "C:/x/a.docx".into(),
            size: 1,
            mtime: 2,
            ctime: 3,
        });
        let b = candidate_signature(CandidateSignatureInput {
            path: "C:/x/a.docx".into(),
            size: 1,
            mtime: 2,
            ctime: 4,
        });
        assert_ne!(a, b);
    }
}
