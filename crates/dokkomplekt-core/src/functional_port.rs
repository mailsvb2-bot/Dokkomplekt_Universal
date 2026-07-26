//! Functional rewrite layer ported from the Python v1.5.x behavior contracts.
//!
//! This module is not a facade over the old Python code. It is a Rust implementation of the
//! behavior that repeatedly caused regressions in the legacy project: source parsing, dynamic
//! button creation, popup planning, diary scheduling, folder naming, RVK formatting, and strict
//! template rendering.

use crate::core::{SourceDocument, TargetTemplate};
use crate::label_search::find_label_end;
use crate::universal_pipeline::canonical_role_for_domain;
use crate::{
    detect_title, is_valid_field_id, merge_value, parse_flexible_date, render_text_template,
    DocumentTemplateSpec, DomainKind, RenderResult, SemanticCase, SemanticValue, ValueSource,
    WorkflowFlags, WorkflowPlan,
};
use crate::{
    run_universal_constructor_pipeline, UniversalDomain, UniversalPipelineFlags,
    UniversalPipelineInput,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortedParseReport {
    pub recognized_title: Option<String>,
    pub filled_fields: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiaryEntryPlan {
    pub index: usize,
    pub date: String,
    pub day_number: String,
    pub month: String,
    pub year: String,
    pub template_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortedFieldConflict {
    pub field_id: String,
    pub current: String,
    pub incoming: String,
    pub requires_confirmation: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputNamingOptions {
    pub full_name: bool,
    pub surname_initials: bool,
    pub surname_name: bool,
    pub admission_date: bool,
    pub discharge_date: bool,
    pub admission_and_discharge_dates: bool,
    pub admission_month: bool,
    pub discharge_month: bool,
}

pub fn title_for_ported_field(field_id: &str) -> String {
    match field_id {
        "subject.name" => "ФИО / субъект документа",
        "subject.birth_date" => "Дата рождения",
        "subject.address" => "Адрес",
        "document.title" => "Название документа",
        "document.date" => "Дата документа",
        "medical.case_number" => "Номер истории болезни",
        "medical.diagnosis" => "Диагноз",
        "medical.icd10" => "МКБ-10",
        "medical.treatment" => "Лечение",
        "medical.admission_date" => "Дата поступления",
        "medical.discharge_date" => "Дата выписки",
        "medical.commission_date" => "Дата комиссии",
        "medical.sick_leave_number" => "Номер больничного листа",
        "medical.sick_leave_from" => "Больничный лист с",
        "medical.protocol_number" => "Номер протокола",
        "medical.commission_number" => "Номер комиссии",
        "medical.rvk_act_number" => "Номер акта / заключения РВК",
        "medical.discharge_condition" => "Состояние при выписке",
        "medical.rvk_commissariat" => "Военный комиссариат",
        "medical.workplace" => "Место работы",
        "medical.position" => "Должность",
        "medical.labs" => "Анализы",
        _ => {
            return format!(
                "Пользовательское поле: {}",
                field_id
                    .rsplit('.')
                    .next()
                    .unwrap_or(field_id)
                    .replace(['_', '-'], " ")
            )
        }
    }
    .to_string()
}

pub fn create_button_from_template_text(
    text: &str,
    document_id: &str,
    template_path: &str,
    label: Option<&str>,
) -> DocumentTemplateSpec {
    let pipeline = run_universal_constructor_pipeline(UniversalPipelineInput {
        source_document: SourceDocument {
            id: "empty_source".into(),
            text: String::new(),
            metadata: Default::default(),
        },
        target_template: TargetTemplate {
            id: document_id.into(),
            path: template_path.into(),
            text: text.into(),
        },
        domain_hint: None,
        flags: UniversalPipelineFlags::default(),
    });
    let category = domain_kind_from_universal(
        &pipeline.domain,
        pipeline.template_structure.fields.is_empty(),
    );
    let role_id =
        canonical_role_for_domain(&pipeline.domain, &pipeline.template_structure.document_type);
    let mut required_fields = pipeline
        .workflow
        .requires
        .iter()
        .chain(pipeline.workflow.optional.iter())
        .filter(|field| is_valid_field_id(field))
        .cloned()
        .collect::<Vec<_>>();
    required_fields.sort_by(|a, b| prompt_order(a).cmp(&prompt_order(b)).then(a.cmp(b)));
    required_fields.dedup();

    let mut document = DocumentTemplateSpec {
        id: document_id.to_string(),
        button_label: label
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(&pipeline.button.label)
            .to_string(),
        template_path: template_path.to_string(),
        category,
        role_id,
        required_fields,
        placeholders: pipeline.template_structure.fields.clone(),
        is_static_copy: pipeline.workflow.produces.iter().any(|p| p == "copy"),
        popup_fields: Vec::new(),
        popup_configured: false,
    };
    document.popup_fields = crate::default_popup_fields_for_document(&document);
    document
}

fn domain_kind_from_universal(domain: &UniversalDomain, is_static: bool) -> DomainKind {
    if is_static && matches!(domain, UniversalDomain::Custom) {
        return DomainKind::Generic;
    }
    match domain {
        UniversalDomain::Medical => DomainKind::Medical,
        UniversalDomain::Legal => DomainKind::Legal,
        UniversalDomain::Hr => DomainKind::Hr,
        UniversalDomain::Education => DomainKind::Education,
        UniversalDomain::Accounting => DomainKind::Accounting,
        UniversalDomain::Custom => DomainKind::Generic,
    }
}

pub fn parse_legacy_source_text(
    text: &str,
    default_year: i32,
) -> (SemanticCase, PortedParseReport) {
    let mut case = SemanticCase::default();
    let mut report = PortedParseReport {
        recognized_title: detect_title(text),
        filled_fields: Vec::new(),
        warnings: Vec::new(),
    };
    if let Some(title) = report.recognized_title.clone() {
        put(&mut case, &mut report, "document.title", &title, 0.86);
    }
    if let Some(date) = detect_date_near_title(text, default_year) {
        put(
            &mut case,
            &mut report,
            "medical.admission_date",
            &date,
            0.84,
        );
        put(&mut case, &mut report, "document.date", &date, 0.70);
    }

    for (field, aliases) in field_aliases() {
        if let Some(value) = find_labeled_value(text, &aliases, field) {
            let normalized = normalize_field_value(field, &value, default_year).unwrap_or(value);
            if field == "medical.case_number" && looks_like_person_name(&normalized) {
                report.warnings.push(
                    "Номер истории болезни был похож на ФИО и не принят автоматически".to_string(),
                );
                continue;
            }
            put(&mut case, &mut report, field, &normalized, 0.78);
        }
    }

    if let Some(block) = extract_treatment_block(text) {
        put(&mut case, &mut report, "medical.treatment", &block, 0.82);
    }
    if let Some((from, to)) = extract_treatment_period(text, default_year) {
        put(
            &mut case,
            &mut report,
            "medical.admission_date",
            &from,
            0.76,
        );
        put(&mut case, &mut report, "medical.discharge_date", &to, 0.76);
    }

    report.filled_fields.sort();
    report.filled_fields.dedup();
    (case, report)
}

pub fn ported_workflow_plan(
    document: &DocumentTemplateSpec,
    case: &SemanticCase,
    sick_leave_enabled: bool,
) -> WorkflowPlan {
    crate::workflow_engine::plan_workflow(document, case, &WorkflowFlags { sick_leave_enabled })
}

pub fn validate_prompt_answers(
    plan: &WorkflowPlan,
    answers: &BTreeMap<String, String>,
    allow_empty: &[String],
) -> Result<(), Vec<String>> {
    let missing = plan
        .prompts
        .iter()
        .filter(|p| p.required && !allow_empty.iter().any(|x| x == &p.field_id))
        .filter(|p| {
            answers
                .get(&p.field_id)
                .map(|v| v.trim().is_empty())
                .unwrap_or(true)
        })
        .map(|p| format!("{} ({})", p.title, p.field_id))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

pub fn render_ported_template(template: &str, case: &SemanticCase, strict: bool) -> RenderResult {
    render_text_template(template, case, strict)
}

pub fn build_diary_schedule(
    admission_date: &str,
    discharge_date: &str,
    default_year: i32,
) -> Vec<DiaryEntryPlan> {
    let Some(mut current) =
        date_tuple(parse_flexible_date(admission_date, default_year).as_deref())
    else {
        return Vec::new();
    };
    let Some(discharge) = date_tuple(parse_flexible_date(discharge_date, default_year).as_deref())
    else {
        return Vec::new();
    };
    current = add_one_day(current);
    let mut out = Vec::new();
    let mut index = 1;
    while current <= discharge {
        out.push(DiaryEntryPlan {
            index,
            date: format_tuple_date(current),
            day_number: format!("{:02}", current.2),
            month: format!("{:02}", current.1),
            year: format!("{}", current.0),
            template_number: format!("{:02}", index),
        });
        current = add_one_day(current);
        index += 1;
    }
    out
}

pub fn append_diary_signatures(body: &str) -> String {
    let mut out = body.trim().to_string();
    let lower = out.to_lowercase();
    if !lower.contains("лечащий врач") {
        out.push_str("\n\nЛечащий врач __________________");
    }
    if !lower.contains("зав. отдел") && !lower.contains("зав отдел") {
        out.push_str("\nЗав. отделением _______________");
    }
    out
}

pub fn select_diary_text_by_diagnosis<'a>(files: &'a [String], diagnosis: &str) -> Option<&'a str> {
    let target = normalize_compare(diagnosis);
    files
        .iter()
        .find(|name| normalize_compare(strip_extension(name)) == target)
        .or_else(|| {
            files.iter().find(|name| {
                let n = normalize_compare(strip_extension(name));
                target.contains(&n) || n.contains(&target)
            })
        })
        .map(|s| s.as_str())
}

pub fn ported_detect_field_conflict(
    case: &SemanticCase,
    field_id: &str,
    incoming: &str,
    default_year: i32,
) -> Option<PortedFieldConflict> {
    let current = case.get(field_id)?.trim().to_string();
    let incoming_norm =
        parse_flexible_date(incoming, default_year).unwrap_or_else(|| incoming.trim().to_string());
    let current_norm = parse_flexible_date(&current, default_year).unwrap_or(current);
    if incoming_norm.is_empty() || incoming_norm == current_norm {
        return None;
    }
    Some(PortedFieldConflict {
        field_id: field_id.to_string(),
        current: current_norm.clone(),
        incoming: incoming_norm.clone(),
        requires_confirmation: true,
        message: format!(
            "{}: уже есть «{}», новое значение «{}». Требуется подтверждение.",
            title_for_ported_field(field_id),
            current_norm,
            incoming_norm
        ),
    })
}

pub fn build_ported_output_folder_name(
    case: &SemanticCase,
    options: &OutputNamingOptions,
) -> String {
    let fio = case.get("subject.name").unwrap_or("Пациент");
    let admission = case.get("medical.admission_date");
    let discharge = case.get("medical.discharge_date");
    let mut parts = Vec::new();
    if options.full_name {
        parts.push(fio.to_string());
    }
    if options.surname_initials {
        parts.push(surname_initials(fio));
    }
    if options.surname_name {
        parts.push(surname_name(fio));
    }
    if options.admission_date {
        if let Some(v) = admission {
            parts.push(v.to_string());
        }
    }
    if options.discharge_date {
        if let Some(v) = discharge {
            parts.push(v.to_string());
        }
    }
    if options.admission_and_discharge_dates {
        if let (Some(a), Some(d)) = (admission, discharge) {
            parts.push(format!("{} - {}", a, d));
        }
    }
    if options.admission_month {
        if let Some(v) = admission {
            parts.push(month_label(v));
        }
    }
    if options.discharge_month {
        if let Some(v) = discharge {
            parts.push(month_label(v));
        }
    }
    sanitize_filename_like(&if parts.is_empty() {
        fio.to_string()
    } else {
        parts.join(" ")
    })
}

pub fn format_rvk_district(value: &str) -> String {
    match value.trim() {
        "Автозаводский" => "Автозаводского района".to_string(),
        "Ленинский" => "Ленинского района".to_string(),
        "Сормовский" => "Сормовского района".to_string(),
        "Канавинский" => "Канавинского района".to_string(),
        "Московский" => "Московского района".to_string(),
        other => other.to_string(),
    }
}

fn put(
    case: &mut SemanticCase,
    report: &mut PortedParseReport,
    field_id: &str,
    value: &str,
    confidence: f32,
) {
    if value.trim().is_empty() {
        return;
    }
    if merge_value(
        case,
        SemanticValue::new(field_id, value, ValueSource::Scanner, confidence),
    ) {
        report.filled_fields.push(field_id.to_string());
    }
}

fn field_aliases() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "subject.name",
            vec!["ФИО", "Пациент", "Больной", "Фамилия Имя Отчество"],
        ),
        (
            "subject.birth_date",
            vec!["Дата рождения", "Родился", "Родилась"],
        ),
        (
            "subject.address",
            vec!["Адрес", "Место жительства", "Проживает"],
        ),
        (
            "medical.case_number",
            vec![
                "История болезни №",
                "Номер истории болезни",
                "ИБ №",
                "и/б №",
                "№ истории болезни",
            ],
        ),
        (
            "medical.diagnosis",
            vec!["Диагноз", "Основной диагноз", "Клинический диагноз"],
        ),
        ("medical.icd10", vec!["МКБ", "МКБ-10", "Код МКБ"]),
        (
            "medical.treatment",
            vec!["Назначенное лечение", "Лечение", "Проводимое лечение"],
        ),
        (
            "medical.admission_date",
            vec![
                "Дата поступления",
                "Поступил",
                "Поступила",
                "Дата госпитализации",
            ],
        ),
        (
            "medical.discharge_date",
            vec!["Дата выписки", "Выписан", "Выписана"],
        ),
        (
            "medical.workplace",
            vec!["Место работы", "Работает", "Организация"],
        ),
        ("medical.position", vec!["Должность", "в должности"]),
        (
            "medical.labs",
            vec!["Анализы", "Лабораторные данные", "Исследования"],
        ),
    ]
}

fn find_labeled_value(text: &str, aliases: &[&str], field: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        for alias in aliases {
            let Some(value_start) = find_label_end(line, alias) else {
                continue;
            };
            let after = line[value_start..]
                .trim_start_matches([' ', ':', '-', '—', '№'])
                .trim();
            if !after.is_empty() {
                return Some(clean(after));
            }
            if field == "medical.treatment" {
                let block = collect_block(&lines, idx + 1);
                if !block.is_empty() {
                    return Some(block);
                }
            }
            if let Some(next) = lines
                .get(idx + 1)
                .map(|x| clean(x))
                .filter(|v| !v.is_empty() && !looks_like_label(v))
            {
                return Some(next);
            }
        }
    }
    None
}

fn extract_treatment_block(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let lower = line.trim().to_lowercase();
        if lower == "лечение"
            || lower == "назначенное лечение"
            || lower.starts_with("лечение:")
            || lower.starts_with("назначенное лечение:")
        {
            let inline = line
                .split_once(':')
                .map(|(_, v)| clean(v))
                .unwrap_or_default();
            if !inline.is_empty() {
                return Some(inline);
            }
            let block = collect_block(&lines, idx + 1);
            if !block.is_empty() {
                return Some(block);
            }
        }
    }
    None
}

fn extract_treatment_period(text: &str, default_year: i32) -> Option<(String, String)> {
    let lower = text.to_lowercase();
    let idx = lower.find("с ")?;
    let tail = &text[idx..];
    let dates = first_n_dates(tail, default_year, 2);
    if dates.len() == 2 {
        Some((dates[0].clone(), dates[1].clone()))
    } else {
        None
    }
}

fn detect_date_near_title(text: &str, default_year: i32) -> Option<String> {
    for line in text.lines().take(8) {
        if line.to_lowercase().contains("рожд") {
            continue;
        }
        if let Some(date) = first_n_dates(line, default_year, 1).into_iter().next() {
            return Some(date);
        }
    }
    None
}

fn first_n_dates(text: &str, default_year: i32, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    for start in 0..chars.len() {
        if !chars[start].is_ascii_digit() {
            continue;
        }
        for end in (start + 6)..=usize::min(chars.len(), start + 10) {
            let candidate: String = chars[start..end].iter().collect();
            if let Some(parsed) = parse_flexible_date(&candidate, default_year) {
                out.push(parsed);
                if out.len() >= n {
                    return out;
                }
                break;
            }
        }
    }
    out
}

fn normalize_field_value(field: &str, value: &str, default_year: i32) -> Option<String> {
    if field.ends_with("_date") || field == "subject.birth_date" || field == "document.date" {
        return parse_flexible_date(value, default_year);
    }
    None
}

fn looks_like_person_name(value: &str) -> bool {
    let words: Vec<&str> = value.split_whitespace().collect();
    words.len() >= 2
        && words.len() <= 4
        && words
            .iter()
            .all(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
        && !value.chars().any(|c| c.is_ascii_digit())
}

fn clean(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| matches!(c, ':' | ';' | '.' | ' '))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_block(lines: &[&str], start: usize) -> String {
    let mut out = Vec::new();
    for line in lines.iter().skip(start).take(10) {
        let clean_line = line.trim();
        if clean_line.is_empty() {
            continue;
        }
        if !out.is_empty() && looks_like_label(clean_line) {
            break;
        }
        out.push(clean_line);
    }
    out.join("\n")
}

fn looks_like_label(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    [
        "фио",
        "пациент",
        "дата",
        "адрес",
        "диагноз",
        "лечение",
        "анамнез",
        "жалобы",
        "статус",
        "место работы",
        "должность",
        "история болезни",
        "номер истории",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
}

fn prompt_order(field_id: &str) -> usize {
    match field_id {
        "medical.case_number" => 10,
        "medical.diagnosis" => 20,
        "medical.icd10" => 30,
        "medical.treatment" => 40,
        "medical.admission_date" => 50,
        "medical.discharge_date" => 60,
        "medical.commission_date" => 70,
        "medical.sick_leave_number" => 80,
        "medical.rvk_commissariat" => 90,
        "medical.workplace" => 100,
        "medical.position" => 110,
        _ => 1000,
    }
}

fn date_tuple(value: Option<&str>) -> Option<(i32, u32, u32)> {
    let value = value?;
    let mut parts = value.split('.');
    let d = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let y = parts.next()?.parse().ok()?;
    Some((y, m, d))
}

fn format_tuple_date(date: (i32, u32, u32)) -> String {
    format!("{:02}.{:02}.{}", date.2, date.1, date.0)
}

fn add_one_day(date: (i32, u32, u32)) -> (i32, u32, u32) {
    let (mut y, mut m, mut d) = date;
    d += 1;
    let max = days_in_month(y, m);
    if d > max {
        d = 1;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    (y, m, d)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

fn normalize_compare(value: &str) -> String {
    value
        .to_lowercase()
        .replace(".docx", "")
        .replace(".doc", "")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_extension(name: &str) -> &str {
    name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

fn surname_initials(fio: &str) -> String {
    let parts: Vec<&str> = fio.split_whitespace().collect();
    let surname = parts.first().copied().unwrap_or("");
    let n = parts
        .get(1)
        .and_then(|s| s.chars().next())
        .map(|c| format!("{}.", c))
        .unwrap_or_default();
    let p = parts
        .get(2)
        .and_then(|s| s.chars().next())
        .map(|c| format!("{}.", c))
        .unwrap_or_default();
    [surname.to_string(), n, p]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn surname_name(fio: &str) -> String {
    fio.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

fn month_label(value: &str) -> String {
    let Some((y, m, _)) = date_tuple(Some(value)) else {
        return value.to_string();
    };
    format!("{:02}.{}", m, y)
}

fn sanitize_filename_like(value: &str) -> String {
    let forbidden = ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '_'];
    let out = value
        .chars()
        .map(|c| {
            if forbidden.contains(&c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect::<String>();
    let clean = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        "Пациент".to_string()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_number_does_not_become_fio() {
        let (case, report) = parse_legacy_source_text("01.06.2026 Первичный осмотр\nФИО: Иванов Иван Иванович\nИстория болезни № Иванов Иван Иванович\nДиагноз: тест\nЛечение: терапия", 2026);
        assert_eq!(case.get("subject.name"), Some("Иванов Иван Иванович"));
        assert_eq!(case.get("medical.case_number"), None);
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn diaries_have_signatures() {
        let text = append_diary_signatures("Состояние стабильное.");
        assert!(text.contains("Лечащий врач"));
        assert!(text.contains("Зав. отделением"));
    }

    #[test]
    fn folder_names_have_spaces() {
        let mut case = SemanticCase::default();
        merge_value(
            &mut case,
            SemanticValue::new(
                "subject.name",
                "Иванов Иван Иванович",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        merge_value(
            &mut case,
            SemanticValue::new(
                "medical.admission_date",
                "01.06.2026",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        merge_value(
            &mut case,
            SemanticValue::new(
                "medical.discharge_date",
                "03.06.2026",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let name = build_ported_output_folder_name(
            &case,
            &OutputNamingOptions {
                full_name: true,
                admission_and_discharge_dates: true,
                ..Default::default()
            },
        );
        assert_eq!(name, "Иванов Иван Иванович 01.06.2026 - 03.06.2026");
    }
}
