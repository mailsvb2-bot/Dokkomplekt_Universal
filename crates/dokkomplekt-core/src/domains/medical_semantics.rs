//! Role-scoped semantic fields for medical documents whose labels are ambiguous.
//!
//! A single case can create both an MSE commission document and a sick-leave VK
//! document.  Their protocol number/date and workplace requisites are independent,
//! even though old templates often used the same human labels.  Persist the values
//! separately and adapt them to legacy generic placeholders only for the document
//! currently being rendered.

use crate::{SemanticCase, SemanticValue, ValueSource};
use chrono::{Duration, NaiveDate};

pub const VK_MSE_COMMISSION_DATE: &str = "medical.vk_mse.commission_date";
pub const VK_MSE_PROTOCOL_NUMBER: &str = "medical.vk_mse.protocol_number";
pub const VK_MSE_PROTOCOL_DATE: &str = "medical.vk_mse.protocol_date";
pub const VK_MSE_WORKPLACE: &str = "medical.vk_mse.workplace";
pub const VK_MSE_POSITION: &str = "medical.vk_mse.position";

pub const SICK_LEAVE_VK_COMMISSION_DATE: &str = "medical.sick_leave_vk.commission_date";
pub const SICK_LEAVE_VK_PROTOCOL_NUMBER: &str = "medical.sick_leave_vk.protocol_number";
pub const SICK_LEAVE_VK_PROTOCOL_DATE: &str = "medical.sick_leave_vk.protocol_date";
pub const SICK_LEAVE_VK_WORKPLACE: &str = "medical.sick_leave_vk.workplace";
pub const SICK_LEAVE_VK_POSITION: &str = "medical.sick_leave_vk.position";

pub const MEDICAL_EXPERT_ANAMNESIS: &str = "medical.expert_anamnesis";
pub const MEDICAL_SICK_LEAVE_NEEDED: &str = "medical.sick_leave_needed";

const VK_MSE_BINDINGS: &[(&str, &str)] = &[
    (VK_MSE_COMMISSION_DATE, "medical.commission_date"),
    (VK_MSE_PROTOCOL_NUMBER, "medical.protocol_number"),
    (VK_MSE_PROTOCOL_DATE, "medical.protocol_date"),
    (VK_MSE_WORKPLACE, "medical.workplace"),
    (VK_MSE_POSITION, "medical.position"),
];

const SICK_LEAVE_VK_BINDINGS: &[(&str, &str)] = &[
    (SICK_LEAVE_VK_COMMISSION_DATE, "medical.commission_date"),
    (SICK_LEAVE_VK_PROTOCOL_NUMBER, "medical.protocol_number"),
    (SICK_LEAVE_VK_PROTOCOL_DATE, "medical.protocol_date"),
    (SICK_LEAVE_VK_WORKPLACE, "medical.workplace"),
    (SICK_LEAVE_VK_POSITION, "medical.position"),
];

pub fn role_scoped_bindings(role_id: &str) -> &'static [(&'static str, &'static str)] {
    match crate::domains::medical::canonical_medical_role(role_id).as_str() {
        "vk_mse" => VK_MSE_BINDINGS,
        "sick_leave_vk" => SICK_LEAVE_VK_BINDINGS,
        _ => &[],
    }
}

/// Convert a legacy generic medical field into the independent storage id for a
/// document role. Fields without role-dependent meaning are returned unchanged.
pub fn scope_legacy_field_for_role(role_id: &str, field_id: &str) -> String {
    role_scoped_bindings(role_id)
        .iter()
        .find_map(|(scoped, legacy)| (*legacy == field_id).then(|| (*scoped).to_string()))
        .unwrap_or_else(|| field_id.to_string())
}

/// Clone a case for one render and project role-specific values onto the legacy
/// generic ids used by older user templates. The persistent case remains unchanged.
/// Exact scoped values win; when only a legacy value exists no projection is made,
/// so backward-compatible reading continues to work.
pub fn case_for_medical_document_render(case: &SemanticCase, role_id: &str) -> SemanticCase {
    let mut scoped_case = case.clone();
    for (scoped_id, legacy_id) in role_scoped_bindings(role_id) {
        if let Some(mut value) = case.values.get(*scoped_id).cloned() {
            value.field_id = (*legacy_id).to_string();
            scoped_case.values.insert((*legacy_id).to_string(), value);
        }
        if case.skipped_fields.contains(*scoped_id) {
            scoped_case.skipped_fields.insert((*legacy_id).to_string());
        }
    }

    // Historical templates used one visual placeholder `Место работы / должность`.
    // Build it only in this render clone from the current role-scoped facts; never
    // persist it as another source of truth.
    if let Some(combined) = combined_work_position(&scoped_case) {
        scoped_case.unskip(crate::MEDICAL_WORK_POSITION);
        scoped_case.values.insert(
            crate::MEDICAL_WORK_POSITION.to_string(),
            SemanticValue::new(
                crate::MEDICAL_WORK_POSITION,
                combined,
                ValueSource::SafeDefault,
                1.0,
            ),
        );
    } else {
        scoped_case.values.remove(crate::MEDICAL_WORK_POSITION);
        if ["medical.workplace", "medical.position"]
            .iter()
            .all(|field_id| scoped_case.is_skipped(field_id))
        {
            scoped_case.skip(crate::MEDICAL_WORK_POSITION);
        }
    }

    let canonical_role = crate::domains::medical::canonical_medical_role(role_id);
    if matches!(canonical_role.as_str(), "primary" | "discharge") {
        // Never reuse a stale expert paragraph from the source document. Build an
        // ephemeral render-only value from the current case and the current role.
        scoped_case.values.remove(MEDICAL_EXPERT_ANAMNESIS);
        let all_sources_skipped = expert_source_fields(&scoped_case, &canonical_role)
            .iter()
            .all(|field_id| scoped_case.is_skipped(field_id));
        if all_sources_skipped {
            scoped_case.skip(MEDICAL_EXPERT_ANAMNESIS);
        } else if let Some(expert) = build_expert_anamnesis(&scoped_case, &canonical_role) {
            scoped_case.unskip(MEDICAL_EXPERT_ANAMNESIS);
            scoped_case.values.insert(
                MEDICAL_EXPERT_ANAMNESIS.to_string(),
                SemanticValue::new(
                    MEDICAL_EXPERT_ANAMNESIS,
                    expert,
                    ValueSource::SafeDefault,
                    1.0,
                ),
            );
        }
    }
    scoped_case
}

fn expert_source_fields(case: &SemanticCase, role_id: &str) -> Vec<&'static str> {
    let mut fields = vec!["medical.workplace", "medical.position"];
    let sick_leave_enabled = role_id == "discharge"
        && case
            .get(MEDICAL_SICK_LEAVE_NEEDED)
            .and_then(normalize_yes_no)
            .or_else(|| case.get("medical.sick_leave_number").map(|_| true))
            .unwrap_or(false);
    if sick_leave_enabled {
        fields.extend([
            "medical.admission_date",
            "medical.discharge_date",
            "medical.sick_leave_number",
        ]);
    }
    fields
}

fn combined_work_position(case: &SemanticCase) -> Option<String> {
    let workplace = work_component(case, "medical.workplace", "subject.organization");
    let position = work_component(case, "medical.position", "subject.position");
    match (workplace, position) {
        (Some(workplace), Some(position)) => Some(format!("{workplace} / {position}")),
        (Some(workplace), None) => Some(workplace),
        (None, Some(position)) => Some(position),
        (None, None) => None,
    }
}

pub fn set_medical_sick_leave_choice(case: &mut SemanticCase, enabled: bool) {
    let value = if enabled { "Да" } else { "Нет" };
    case.values.insert(
        MEDICAL_SICK_LEAVE_NEEDED.to_string(),
        SemanticValue::new(
            MEDICAL_SICK_LEAVE_NEEDED,
            value,
            ValueSource::UserConfirmed,
            1.0,
        ),
    );
    case.skipped_fields.remove(MEDICAL_SICK_LEAVE_NEEDED);
}

fn build_expert_anamnesis(case: &SemanticCase, role_id: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(work) = expert_work_sentence(case) {
        parts.push(work);
    }

    if role_id == "discharge" {
        let sick_needed = case
            .get(MEDICAL_SICK_LEAVE_NEEDED)
            .and_then(normalize_yes_no)
            .or_else(|| case.get("medical.sick_leave_number").map(|_| true));
        match sick_needed {
            Some(true) => parts.push(discharge_sick_leave_sentence(case)),
            Some(false) => parts.push("В выдаче ЛН не нуждается.".to_string()),
            None => {}
        }
    }

    (!parts.is_empty()).then(|| parts.join(" "))
}

fn expert_work_sentence(case: &SemanticCase) -> Option<String> {
    let workplace = work_component(case, "medical.workplace", "subject.organization");
    let position = work_component(case, "medical.position", "subject.position");
    match (workplace, position) {
        (Some(workplace), Some(position)) => {
            Some(format!("Работает в {workplace}, в должности {position}."))
        }
        (Some(workplace), None) => Some(format!("Работает в {workplace}.")),
        (None, Some(position)) => Some(format!("Работает, должность: {position}.")),
        (None, None) => None,
    }
}

fn work_component(case: &SemanticCase, field_id: &str, fallback_id: &str) -> Option<String> {
    if case.is_skipped(field_id) {
        return None;
    }
    case.get(field_id)
        .or_else(|| case.get(fallback_id))
        .map(clean_expert_component)
        .filter(|value| !value.is_empty())
}

fn discharge_sick_leave_sentence(case: &SemanticCase) -> String {
    let number = case
        .get("medical.sick_leave_number")
        .map(clean_expert_component)
        .filter(|value| !value.is_empty());
    let mut line = match number {
        Some(number) => format!("Больничный лист № {number}."),
        None => "Больничный лист.".to_string(),
    };

    let start = if case.is_skipped("medical.admission_date") {
        None
    } else {
        case.get("medical.admission_date")
            .or_else(|| case.get("medical.sick_leave_from"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let finish = case
        .get("medical.discharge_date")
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let (Some(start), Some(finish)) = (start, finish) {
        if let (Some(start_date), Some(finish_date)) =
            (parse_medical_date(start), parse_medical_date(finish))
        {
            if finish_date >= start_date {
                let days = (finish_date - start_date).num_days() + 1;
                line.push_str(&format!(
                    " Срок лечения с {start} по {finish}, {days} {}.",
                    russian_day_word(days)
                ));
            } else {
                line.push_str(&format!(" Срок лечения с {start} по {finish}."));
            }
        } else {
            line.push_str(&format!(" Срок лечения с {start} по {finish}."));
        }
    } else if let Some(start) = start {
        line.push_str(&format!(" Больничный лист открыт с {start}."));
    }

    if let Some(finish_date) = finish.and_then(parse_medical_date) {
        let return_to_work = finish_date + Duration::days(1);
        line.push_str(&format!(
            " К труду с {}.",
            return_to_work.format("%d.%m.%Y")
        ));
    }
    line
}

fn clean_expert_component(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(['.', ',', ';', ':'])
        .trim()
        .to_string()
}

fn parse_medical_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%d.%m.%Y").ok()
}

fn normalize_yes_no(value: &str) -> Option<bool> {
    let normalized = value.trim().to_lowercase().replace('ё', "е");
    match normalized.as_str() {
        "да" | "д" | "yes" | "y" | "1" | "+" | "нужен" | "нужна" | "нужно" => {
            Some(true)
        }
        "нет" | "н" | "no" | "n" | "0" | "-" | "не нужен" | "не нужна" | "не нужно" => {
            Some(false)
        }
        _ => None,
    }
}

fn russian_day_word(days: i64) -> &'static str {
    let last_two = days.rem_euclid(100);
    if (11..=14).contains(&last_two) {
        return "дней";
    }
    match days.rem_euclid(10) {
        1 => "день",
        2..=4 => "дня",
        _ => "дней",
    }
}

pub fn title_for_role_scoped_field(field_id: &str) -> Option<&'static str> {
    match field_id {
        VK_MSE_COMMISSION_DATE => Some("Дата ВК на МСЭ"),
        VK_MSE_PROTOCOL_NUMBER => Some("Номер протокола ВК на МСЭ"),
        VK_MSE_PROTOCOL_DATE => Some("Дата протокола ВК на МСЭ"),
        VK_MSE_WORKPLACE => Some("Место работы для ВК на МСЭ"),
        VK_MSE_POSITION => Some("Должность для ВК на МСЭ"),
        SICK_LEAVE_VK_COMMISSION_DATE => Some("Дата ВК по больничному"),
        SICK_LEAVE_VK_PROTOCOL_NUMBER => Some("Номер протокола ВК по больничному"),
        SICK_LEAVE_VK_PROTOCOL_DATE => Some("Дата протокола ВК по больничному"),
        SICK_LEAVE_VK_WORKPLACE => Some("Место работы для ВК по больничному"),
        SICK_LEAVE_VK_POSITION => Some("Должность для ВК по больничному"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    fn put(case: &mut SemanticCase, field_id: &str, value: &str) {
        case.values.insert(
            field_id.into(),
            SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
        );
    }

    #[test]
    fn mse_and_sick_leave_protocols_are_independent() {
        let mut case = SemanticCase::default();
        put(&mut case, VK_MSE_PROTOCOL_NUMBER, "MSE-10");
        put(&mut case, SICK_LEAVE_VK_PROTOCOL_NUMBER, "SL-20");

        let mse = case_for_medical_document_render(&case, "vk_mse");
        let sick = case_for_medical_document_render(&case, "sick_leave_vk");
        assert_eq!(mse.get("medical.protocol_number"), Some("MSE-10"));
        assert_eq!(sick.get("medical.protocol_number"), Some("SL-20"));
        assert_eq!(case.get("medical.protocol_number"), None);
    }

    #[test]
    fn legacy_generic_value_is_not_destroyed_when_scoped_value_is_missing() {
        let mut case = SemanticCase::default();
        put(&mut case, "medical.protocol_number", "OLD-77");
        let mse = case_for_medical_document_render(&case, "vk_mse");
        assert_eq!(mse.get("medical.protocol_number"), Some("OLD-77"));
    }

    #[test]
    fn only_ambiguous_fields_are_scoped() {
        assert_eq!(
            scope_legacy_field_for_role("vk_mse", "medical.protocol_number"),
            VK_MSE_PROTOCOL_NUMBER
        );
        assert_eq!(
            scope_legacy_field_for_role("reception", "medical.protocol_number"),
            "medical.protocol_number"
        );
        assert_eq!(
            scope_legacy_field_for_role("vk_mse", "medical.diagnosis"),
            "medical.diagnosis"
        );
    }

    #[test]
    fn explicitly_skipped_optional_expert_sources_omit_the_derived_placeholder() {
        let mut case = SemanticCase::default();
        case.skip("medical.workplace");
        case.skip("medical.position");

        let prepared = case_for_medical_document_render(&case, "primary");
        assert!(prepared.is_skipped(MEDICAL_EXPERT_ANAMNESIS));
        let rendered = crate::render_text_template(
            "Первичный осмотр\n{{medical.expert_anamnesis}}",
            &case,
            true,
        );
        assert!(rendered.missing_fields.is_empty());
        assert!(!rendered
            .output_text
            .contains("{{medical.expert_anamnesis}}"));
    }

    #[test]
    fn individually_skipped_work_source_never_reappears_through_profile_fallback() {
        let mut case = SemanticCase::default();
        put(&mut case, "subject.organization", "Скрытая организация");
        put(&mut case, "medical.position", "инженер");
        case.skip("medical.workplace");

        let prepared = case_for_medical_document_render(&case, "primary");
        assert_eq!(
            prepared.get(MEDICAL_EXPERT_ANAMNESIS),
            Some("Работает, должность: инженер.")
        );
        assert_eq!(prepared.get(crate::MEDICAL_WORK_POSITION), Some("инженер"));
        assert!(!prepared
            .get(MEDICAL_EXPERT_ANAMNESIS)
            .unwrap_or_default()
            .contains("Скрытая организация"));
    }

    #[test]
    fn skipped_admission_never_reappears_as_sick_leave_start_fallback() {
        let mut case = SemanticCase::default();
        put(&mut case, "medical.workplace", "ООО Пример");
        put(&mut case, "medical.position", "инженер");
        put(&mut case, "medical.sick_leave_number", "ЛН-7");
        put(&mut case, "medical.sick_leave_from", "20.05.2026");
        put(&mut case, "medical.discharge_date", "12.06.2026");
        set_medical_sick_leave_choice(&mut case, true);
        case.skip("medical.admission_date");

        let prepared = case_for_medical_document_render(&case, "discharge");
        let expert = prepared
            .get(MEDICAL_EXPERT_ANAMNESIS)
            .expect("remaining expert facts should still render");
        assert!(expert.contains("Больничный лист № ЛН-7."));
        assert!(!expert.contains("20.05.2026"));
    }
}
