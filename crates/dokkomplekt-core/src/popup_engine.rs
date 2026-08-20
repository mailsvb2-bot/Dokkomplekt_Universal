use crate::{
    merge_value, parse_flexible_date, plan_workflow, set_user_value, validate_case_relations,
    validate_inn, validate_kpp, validate_ogrn, validate_snils, validate_vin, DocumentTemplateSpec,
    PromptInputKind, PromptSpec, SemanticCase, SemanticValue, ValueSource, WorkflowFlags,
    WorkflowPlan,
};
use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupAnswer {
    pub field_id: String,
    pub value: String,
    #[serde(default)]
    pub continue_without_value: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupApplyResult {
    pub accepted: bool,
    pub semantic_case: SemanticCase,
    pub still_missing: Vec<PromptSpec>,
    pub message: String,
    #[serde(default)]
    pub errors: Vec<String>,
}

/// Builds one merged popup instead of chains like "discharge popup" then "date popup".
pub fn build_merged_popup_plan(
    document: &DocumentTemplateSpec,
    case: &SemanticCase,
    flags: &WorkflowFlags,
) -> WorkflowPlan {
    plan_workflow(document, case, flags)
}

/// Final fail-closed check used immediately before publication.
///
/// This intentionally consumes the already-built WorkflowPlan instead of
/// rebuilding profession rules. Required active prompts must either have a
/// validated value in the SemanticCase or be explicitly skipped when the plan
/// allows skipping. Linked Yes/No children reuse the same activity predicate as
/// popup application, so a hidden child can never reappear as a second rule set.
pub fn workflow_publication_blockers(case: &SemanticCase, plan: &WorkflowPlan) -> Vec<String> {
    let answers = BTreeMap::<&str, &PopupAnswer>::new();
    let prompt_ids = plan
        .prompts
        .iter()
        .map(|prompt| prompt.field_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut blockers = plan.block_reasons.clone();

    for prompt in &plan.prompts {
        if !prompt.required || !prompt_is_active(prompt, plan, &answers, case) {
            continue;
        }
        if prompt.skippable && case.is_skipped(&prompt.field_id) {
            continue;
        }
        let Some(value) = case
            .get(&prompt.field_id)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            blockers.push(format!("Не заполнено обязательное поле: {}", prompt.title));
            continue;
        };
        if let Err(error) = validate_prompt_value(prompt, value) {
            blockers.push(error);
        }
    }

    for (field_id, error) in validate_case_relations(case) {
        if prompt_ids.contains(field_id.as_str()) {
            blockers.push(error);
        }
    }

    let mut seen = BTreeSet::new();
    blockers
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// UI contract: if a required field is empty and not explicitly skipped, popup stays open.
pub fn apply_popup_answers(
    case: &SemanticCase,
    plan: &WorkflowPlan,
    answers: &[PopupAnswer],
) -> PopupApplyResult {
    let known_fields = plan
        .prompts
        .iter()
        .map(|prompt| prompt.field_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut answer_ids = BTreeSet::<&str>::new();
    let mut validation_errors = Vec::new();
    for answer in answers {
        let field_id = answer.field_id.trim();
        if !known_fields.contains(field_id) {
            validation_errors.push(format!("Неизвестный ответ popup: {field_id}"));
        } else if !answer_ids.insert(field_id) {
            validation_errors.push(format!("Ответ для поля «{field_id}» передан повторно"));
        }
    }
    let by_id = answers
        .iter()
        .map(|answer| (answer.field_id.trim(), answer))
        .collect::<BTreeMap<_, _>>();
    let explicit_without_labs = by_id.get("medical.labs_without").is_some_and(|answer| {
        matches!(
            answer.value.trim().to_lowercase().as_str(),
            "да" | "yes" | "true"
        )
    });
    let mut next = case.clone();
    let mut still_missing = Vec::new();

    for prompt in &plan.prompts {
        if !prompt_is_active(prompt, plan, &by_id, case) {
            next.skip(&prompt.field_id);
            continue;
        }
        if prompt.field_id == "medical.labs" && explicit_without_labs {
            next.unskip("medical.labs");
            set_user_value(&mut next, "medical.labs", "Нет анализов");
            continue;
        }
        let Some(answer) = by_id.get(prompt.field_id.as_str()) else {
            if prompt.required {
                still_missing.push(prompt.clone());
            }
            continue;
        };
        let value = answer.value.trim();
        if value.is_empty() {
            if answer.continue_without_value {
                if prompt.required && !prompt.skippable {
                    still_missing.push(prompt.clone());
                    validation_errors.push(format!(
                        "{}: поле обязательно для выбранного шаблона и не может быть пропущено",
                        prompt.title
                    ));
                } else {
                    next.skip(&prompt.field_id);
                }
            } else if prompt.required {
                still_missing.push(prompt.clone());
            }
            continue;
        }
        next.unskip(&prompt.field_id);
        let normalized_value = match normalize_prompt_value(prompt, value) {
            Ok(value) => value,
            Err(error) => {
                let mut invalid = prompt.clone();
                invalid.validation_hint = Some(error.clone());
                still_missing.push(invalid);
                validation_errors.push(error);
                continue;
            }
        };
        merge_value(
            &mut next,
            SemanticValue::new(
                prompt.field_id.as_str(),
                normalized_value,
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
    }

    for (field_id, error) in validate_case_relations(&next) {
        if let Some(prompt) = plan
            .prompts
            .iter()
            .find(|prompt| prompt.field_id == field_id)
        {
            let mut invalid = prompt.clone();
            invalid.validation_hint = Some(error.clone());
            if !still_missing.iter().any(|item| item.field_id == field_id) {
                still_missing.push(invalid);
            }
        }
        validation_errors.push(error);
    }

    let date_order_error = validate_date_order(&next, plan, &mut still_missing);
    if let Some(error) = &date_order_error {
        validation_errors.push(error.clone());
    }
    let accepted = still_missing.is_empty() && validation_errors.is_empty();
    PopupApplyResult {
        accepted,
        semantic_case: next,
        message: if accepted {
            "Поля приняты; значения будут автоматически подтянуты в остальные документы комплекта"
                .to_string()
        } else if let Some(error) = validation_errors.first() {
            error.clone()
        } else if let Some(prompt) = still_missing.first() {
            format!("Не заполнено обязательное поле: {}", prompt.title)
        } else {
            "Проверьте введённые значения".to_string()
        },
        still_missing,
        errors: validation_errors,
    }
}

fn yes_no_value_is_affirmative(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().replace('ё', "е").as_str(),
        "да" | "yes" | "true"
    )
}

fn prompt_is_active(
    prompt: &PromptSpec,
    plan: &WorkflowPlan,
    answers: &BTreeMap<&str, &PopupAnswer>,
    case: &SemanticCase,
) -> bool {
    let Some(linked_to) = prompt.linked_to.as_deref() else {
        return true;
    };
    let Some(source) = plan
        .prompts
        .iter()
        .find(|candidate| candidate.field_id == linked_to)
    else {
        return true;
    };
    if !matches!(source.input_kind, PromptInputKind::YesNo) {
        return true;
    }
    let source_value = answers
        .get(source.field_id.as_str())
        .map(|answer| answer.value.as_str())
        .or_else(|| case.get(&source.field_id))
        .or(source.current_value.as_deref())
        .unwrap_or_default();
    yes_no_value_is_affirmative(source_value)
}

fn normalize_prompt_value(prompt: &PromptSpec, value: &str) -> Result<String, String> {
    if matches!(prompt.input_kind, PromptInputKind::Date) {
        let reference_year = Local::now().year();
        return parse_flexible_date(value, reference_year)
            .ok_or_else(|| format!("{}: ожидается корректная дата", prompt.title));
    }
    validate_prompt_value(prompt, value)?;
    Ok(value.trim().to_string())
}

fn validate_prompt_value(prompt: &PromptSpec, value: &str) -> Result<(), String> {
    match prompt.input_kind {
        PromptInputKind::Date => parse_flexible_date(value, Local::now().year())
            .map(|_| ())
            .ok_or_else(|| format!("{}: ожидается корректная дата", prompt.title)),
        PromptInputKind::Number => parse_finite_number(value)
            .map(|_| ())
            .map_err(|_| format!("{}: ожидается конечное число", prompt.title)),
        PromptInputKind::Money => parse_money(value)
            .map(|_| ())
            .map_err(|_| format!("{}: ожидается корректная денежная сумма", prompt.title)),
        PromptInputKind::Inn => validate_inn(value),
        PromptInputKind::Kpp => validate_kpp(value),
        PromptInputKind::Ogrn => validate_ogrn(value),
        PromptInputKind::Snils => validate_snils(value),
        PromptInputKind::Vin => validate_vin(value),
        PromptInputKind::Select if !prompt.allow_custom_option && !prompt.options.is_empty() => {
            if prompt
                .options
                .iter()
                .any(|option| option.eq_ignore_ascii_case(value))
            {
                Ok(())
            } else {
                Err(format!("{}: выберите значение из списка", prompt.title))
            }
        }
        PromptInputKind::YesNo => {
            let normalized = value.trim().to_lowercase();
            if matches!(
                normalized.as_str(),
                "да" | "нет" | "yes" | "no" | "true" | "false"
            ) {
                Ok(())
            } else {
                Err(format!("{}: выберите Да или Нет", prompt.title))
            }
        }
        PromptInputKind::Text
        | PromptInputKind::LongText
        | PromptInputKind::Passport
        | PromptInputKind::Icd10
        | PromptInputKind::Select => Ok(()),
    }
}

fn parse_finite_number(value: &str) -> Result<f64, ()> {
    let normalized = value.trim().replace(' ', "").replace(',', ".");
    if normalized.is_empty() {
        return Err(());
    }
    let number = normalized.parse::<f64>().map_err(|_| ())?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(())
    }
}

fn parse_money(value: &str) -> Result<f64, ()> {
    let mut normalized = value.trim().to_lowercase();
    for suffix in ["рублей", "рубля", "руб.", "руб", "₽"] {
        if normalized.ends_with(suffix) {
            let new_len = normalized.len().saturating_sub(suffix.len());
            normalized.truncate(new_len);
            normalized = normalized.trim().to_string();
            break;
        }
    }
    if normalized.is_empty()
        || normalized.chars().any(|character| {
            !(character.is_ascii_digit()
                || matches!(character, '-' | '+' | ',' | '.' | ' ' | '\u{00a0}'))
        })
    {
        return Err(());
    }
    let compact = normalized.replace([' ', '\u{00a0}'], "").replace(',', ".");
    if compact.matches('.').count() > 1
        || compact.matches('-').count() > 1
        || compact.matches('+').count() > 1
        || (compact.contains('-') && !compact.starts_with('-'))
        || (compact.contains('+') && !compact.starts_with('+'))
    {
        return Err(());
    }
    if let Some((_, fraction)) = compact.split_once('.') {
        if fraction.len() > 2 || fraction.is_empty() {
            return Err(());
        }
    }
    let number = compact.parse::<f64>().map_err(|_| ())?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(())
    }
}

fn validate_date_order(
    case: &SemanticCase,
    plan: &WorkflowPlan,
    still_missing: &mut Vec<PromptSpec>,
) -> Option<String> {
    const DATE_PAIRS: [(&str, &str); 3] = [
        ("period.start_date", "period.end_date"),
        ("contract.start_date", "contract.end_date"),
        ("medical.admission_date", "medical.discharge_date"),
    ];
    for (start_id, end_id) in DATE_PAIRS {
        let (Some(start), Some(end)) = (case.get(start_id), case.get(end_id)) else {
            continue;
        };
        let (Some(start), Some(end)) = (parse_full_date(start), parse_full_date(end)) else {
            continue;
        };
        if end < start {
            if let Some(prompt) = plan.prompts.iter().find(|prompt| prompt.field_id == end_id) {
                if !still_missing
                    .iter()
                    .any(|item| item.field_id == prompt.field_id)
                {
                    still_missing.push(prompt.clone());
                }
            } else {
                still_missing.push(PromptSpec {
                    field_id: end_id.to_string(),
                    title: "Дата окончания периода".to_string(),
                    required: true,
                    skippable: false,
                    current_value: case.get(end_id).map(str::to_string),
                    validation_hint: Some(
                        "Дата окончания не может быть раньше даты начала".to_string(),
                    ),
                    input_kind: PromptInputKind::Date,
                    ask_mode: crate::PromptAskMode::IfMissing,
                    options: Vec::new(),
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: None,
                    order: 500,
                });
            }
            return Some("Дата окончания не может быть раньше даты начала. Исправьте значение — введённые данные пока не сохранены.".to_string());
        }
    }
    None
}

fn parse_full_date(value: &str) -> Option<NaiveDate> {
    ["%d.%m.%Y", "%Y-%m-%d", "%d/%m/%Y"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(value.trim(), format).ok())
}

pub fn remember_shared_answers(case: &mut SemanticCase, answers: &[(&str, &str)]) {
    for (field_id, value) in answers {
        set_user_value(case, *field_id, *value);
    }
}

pub fn required_fields_from_user_marks(
    existing: &[String],
    user_required: &[String],
) -> Vec<String> {
    let mut out = existing.iter().cloned().collect::<BTreeSet<_>>();
    for field in user_required {
        if !field.trim().is_empty() {
            out.insert(field.trim().to_string());
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_empty_field_keeps_popup_open() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.required".into(),
                title: "Обязательное поле".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.required".into(),
                value: "   ".into(),
                continue_without_value: false,
            }],
        );
        assert!(!result.accepted);
        assert_eq!(result.still_missing.len(), 1);
        assert_eq!(result.still_missing[0].field_id, "custom.required");
    }

    #[test]
    fn hard_required_field_rejects_continue_without_and_preserves_original_case() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.required".into(),
                title: "Критическое поле".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.required".into(),
                value: String::new(),
                continue_without_value: true,
            }],
        );
        assert!(!result.accepted);
        assert!(!result.semantic_case.is_skipped("custom.required"));
        assert!(case.skipped_fields.is_empty());
    }

    #[test]
    fn explicitly_skippable_required_allows_explicit_skip() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.note".into(),
                title: "Note".into(),
                required: true,
                skippable: true,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.note".into(),
                value: "".into(),
                continue_without_value: true,
            }],
        );
        assert!(result.accepted);
        assert!(result.semantic_case.is_skipped("custom.note"));
        let rendered =
            crate::render_text_template("До {{custom.note}} после", &result.semantic_case, true);
        assert_eq!(rendered.output_text, "До  после");
        assert!(rendered.missing_fields.is_empty());
    }

    #[test]
    fn explicit_skip_hides_existing_value_until_user_supplies_a_replacement() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "custom.note", "Старое значение");
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.note".into(),
                title: "Note".into(),
                required: true,
                skippable: true,
                current_value: Some("Старое значение".into()),
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::Always,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let skipped = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.note".into(),
                value: String::new(),
                continue_without_value: true,
            }],
        );
        assert!(skipped.accepted);
        assert_eq!(skipped.semantic_case.get("custom.note"), None);
        assert!(skipped.semantic_case.values.contains_key("custom.note"));

        let replaced = apply_popup_answers(
            &skipped.semantic_case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.note".into(),
                value: "Новое значение".into(),
                continue_without_value: false,
            }],
        );
        assert!(replaced.accepted);
        assert!(!replaced.semantic_case.is_skipped("custom.note"));
        assert_eq!(
            replaced.semantic_case.get("custom.note"),
            Some("Новое значение")
        );
    }

    #[test]
    fn rejects_period_end_before_start_without_mutating_original_case() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "period.start_date", "10.07.2026");
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "period.end_date".into(),
                title: "Дата окончания".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "period.end_date".into(),
                value: "09.07.2026".into(),
                continue_without_value: false,
            }],
        );
        assert!(!result.accepted);
        assert!(result.message.contains("не может быть раньше"));
        assert_eq!(case.get("period.end_date"), None);
    }

    #[test]
    fn accepts_iso_period_in_chronological_order() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "period.start_date", "2026-07-10");
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "period.end_date".into(),
                title: "Дата окончания".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 500,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "period.end_date".into(),
                value: "2026-07-11".into(),
                continue_without_value: false,
            }],
        );
        assert!(result.accepted);
    }

    #[test]
    fn negative_yes_no_answer_deactivates_required_linked_prompt() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![
                PromptSpec {
                    field_id: "custom.need_details".into(),
                    title: "Нужны дополнительные сведения?".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::YesNo,
                    ask_mode: crate::PromptAskMode::Always,
                    options: vec!["Нет".into(), "Да".into()],
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: None,
                    order: 10,
                },
                PromptSpec {
                    field_id: "custom.details".into(),
                    title: "Дополнительные сведения".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::LongText,
                    ask_mode: crate::PromptAskMode::Always,
                    options: Vec::new(),
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: Some("custom.need_details".into()),
                    order: 20,
                },
            ],
            blocked: false,
            block_reasons: vec![],
        };

        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.need_details".into(),
                value: "Нет".into(),
                continue_without_value: false,
            }],
        );

        assert!(result.accepted);
        assert!(result.still_missing.is_empty());
        assert_eq!(result.semantic_case.get("custom.details"), None);
        assert!(result.semantic_case.is_skipped("custom.details"));
    }

    #[test]
    fn negative_yes_no_answer_hides_a_stale_linked_value_from_rendering() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "custom.details", "Старое значение");
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![
                PromptSpec {
                    field_id: "custom.need_details".into(),
                    title: "Нужны дополнительные сведения?".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::YesNo,
                    ask_mode: crate::PromptAskMode::Always,
                    options: vec!["Нет".into(), "Да".into()],
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: None,
                    order: 10,
                },
                PromptSpec {
                    field_id: "custom.details".into(),
                    title: "Дополнительные сведения".into(),
                    required: true,
                    skippable: false,
                    current_value: Some("Старое значение".into()),
                    validation_hint: None,
                    input_kind: PromptInputKind::LongText,
                    ask_mode: crate::PromptAskMode::Always,
                    options: Vec::new(),
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: Some("custom.need_details".into()),
                    order: 20,
                },
            ],
            blocked: false,
            block_reasons: vec![],
        };

        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.need_details".into(),
                value: "Нет".into(),
                continue_without_value: false,
            }],
        );

        assert!(result.accepted);
        assert!(result.semantic_case.is_skipped("custom.details"));
        let rendered =
            crate::render_text_template("До {{custom.details}} после", &result.semantic_case, true);
        assert_eq!(rendered.output_text, "До  после");
        assert!(rendered.missing_fields.is_empty());
    }

    #[test]
    fn affirmative_yes_no_answer_keeps_required_linked_prompt_active() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![
                PromptSpec {
                    field_id: "custom.need_details".into(),
                    title: "Нужны дополнительные сведения?".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::YesNo,
                    ask_mode: crate::PromptAskMode::Always,
                    options: vec!["Нет".into(), "Да".into()],
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: None,
                    order: 10,
                },
                PromptSpec {
                    field_id: "custom.details".into(),
                    title: "Дополнительные сведения".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::LongText,
                    ask_mode: crate::PromptAskMode::Always,
                    options: Vec::new(),
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: Some("custom.need_details".into()),
                    order: 20,
                },
            ],
            blocked: false,
            block_reasons: vec![],
        };

        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "custom.need_details".into(),
                value: "Да".into(),
                continue_without_value: false,
            }],
        );

        assert!(!result.accepted);
        assert_eq!(result.still_missing.len(), 1);
        assert_eq!(result.still_missing[0].field_id, "custom.details");
    }

    #[test]
    fn publication_readiness_reuses_conditional_prompt_activity() {
        let mut case = SemanticCase::default();
        set_user_value(&mut case, "custom.need_details", "Нет");
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![
                PromptSpec {
                    field_id: "custom.need_details".into(),
                    title: "Нужны дополнительные сведения?".into(),
                    required: true,
                    skippable: false,
                    current_value: Some("Нет".into()),
                    validation_hint: None,
                    input_kind: PromptInputKind::YesNo,
                    ask_mode: crate::PromptAskMode::Always,
                    options: vec!["Нет".into(), "Да".into()],
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: None,
                    order: 10,
                },
                PromptSpec {
                    field_id: "custom.details".into(),
                    title: "Дополнительные сведения".into(),
                    required: true,
                    skippable: false,
                    current_value: None,
                    validation_hint: None,
                    input_kind: PromptInputKind::LongText,
                    ask_mode: crate::PromptAskMode::Always,
                    options: Vec::new(),
                    allow_custom_option: false,
                    help_text: None,
                    section: None,
                    linked_to: Some("custom.need_details".into()),
                    order: 20,
                },
            ],
            blocked: false,
            block_reasons: vec![],
        };

        assert!(workflow_publication_blockers(&case, &plan).is_empty());

        set_user_value(&mut case, "custom.need_details", "Да");
        let blockers = workflow_publication_blockers(&case, &plan);
        assert_eq!(blockers.len(), 1);
        assert!(blockers[0].contains("Дополнительные сведения"));
    }

    #[test]
    fn publication_readiness_accepts_only_explicitly_skippable_omissions() {
        let mut case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "custom.optional_required".into(),
                title: "Поле с разрешённым пропуском".into(),
                required: true,
                skippable: true,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Text,
                ask_mode: crate::PromptAskMode::IfMissing,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 10,
            }],
            blocked: false,
            block_reasons: vec![],
        };

        assert_eq!(workflow_publication_blockers(&case, &plan).len(), 1);
        case.skip("custom.optional_required");
        assert!(workflow_publication_blockers(&case, &plan).is_empty());
    }

    #[test]
    fn popup_normalizes_compact_date_before_reuse_in_the_set() {
        let case = SemanticCase::default();
        let plan = WorkflowPlan {
            document_id: "x".into(),
            prompts: vec![PromptSpec {
                field_id: "document.date".into(),
                title: "Дата документа".into(),
                required: true,
                skippable: false,
                current_value: None,
                validation_hint: None,
                input_kind: PromptInputKind::Date,
                ask_mode: crate::PromptAskMode::Always,
                options: Vec::new(),
                allow_custom_option: false,
                help_text: None,
                section: None,
                linked_to: None,
                order: 20,
            }],
            blocked: false,
            block_reasons: vec![],
        };
        let result = apply_popup_answers(
            &case,
            &plan,
            &[PopupAnswer {
                field_id: "document.date".into(),
                value: "100526".into(),
                continue_without_value: false,
            }],
        );
        assert!(result.accepted);
        assert_eq!(
            result.semantic_case.get("document.date"),
            Some("10.05.2026")
        );
    }
}
