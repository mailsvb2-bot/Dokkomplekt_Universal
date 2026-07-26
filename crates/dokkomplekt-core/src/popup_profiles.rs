use crate::{
    is_valid_field_id, title_for_field, DocumentTemplateSpec, DomainKind, PopupFieldConfig,
    PromptAskMode, PromptInputKind,
};
use chrono::Local;
use std::collections::{BTreeMap, BTreeSet};

pub fn default_popup_fields_for_document(document: &DocumentTemplateSpec) -> Vec<PopupFieldConfig> {
    let mut required = document
        .required_fields
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut ordered = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .filter(|field| !field.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    ordered.sort_by(|a, b| popup_order(a).cmp(&popup_order(b)).then(a.cmp(b)));
    ordered.dedup();

    let mut configs = ordered
        .into_iter()
        .map(|field_id| {
            let mut config = popup_config_for_field(
                &field_id,
                required.remove(field_id.as_str()),
                &document.category,
                &document.role_id,
            );
            apply_profession_defaults(&mut config, &document.category, &document.role_id);
            config
        })
        .collect::<Vec<_>>();

    for mut config in profession_role_fields(&document.category, &document.role_id) {
        if let Some(existing) = configs
            .iter_mut()
            .find(|existing| existing.field_id == config.field_id)
        {
            existing.required |= config.required;
            if existing.section.is_none() {
                existing.section = config.section.take();
            }
            if existing.help_text.is_none() {
                existing.help_text = config.help_text.take();
            }
            if existing.options.is_empty() {
                existing.options = config.options;
                existing.allow_custom_option = config.allow_custom_option;
            }
            continue;
        }
        configs.push(config);
    }
    configs.sort_by(|a, b| a.order.cmp(&b.order).then(a.field_id.cmp(&b.field_id)));
    configs
}

pub fn effective_popup_fields(document: &DocumentTemplateSpec) -> Vec<PopupFieldConfig> {
    let mut merged = if document.popup_configured {
        BTreeMap::<String, PopupFieldConfig>::new()
    } else {
        default_popup_fields_for_document(document)
            .into_iter()
            .map(|config| (config.field_id.clone(), config))
            .collect::<BTreeMap<_, _>>()
    };
    for config in &document.popup_fields {
        let Some(normalized) = normalize_popup_field(config.clone()) else {
            continue;
        };
        merged.insert(normalized.field_id.clone(), normalized);
    }
    // Fail closed: even a custom popup cannot hide a field that the selected template
    // or workflow has declared strictly required.
    for field_id in &document.required_fields {
        if !is_valid_field_id(field_id) || merged.contains_key(field_id) {
            continue;
        }
        merged.insert(
            field_id.clone(),
            popup_config_for_field(field_id, true, &document.category, &document.role_id),
        );
    }
    let mut fields = merged.into_values().collect::<Vec<_>>();
    fields.sort_by(|a, b| a.order.cmp(&b.order).then(a.field_id.cmp(&b.field_id)));
    fields
}

/// Validates the specialist-authored popup graph before normalization.
///
/// Normalization is intentionally not allowed to silently hide dangerous input:
/// duplicate fields, self-links, links to missing fields and cycles all make the
/// configuration ambiguous and therefore must block saving.
pub fn validate_popup_fields(fields: &[PopupFieldConfig]) -> Result<(), String> {
    let mut ids = BTreeSet::<String>::new();
    for field in fields {
        let field_id = field.field_id.trim();
        if !is_valid_field_id(field_id) {
            return Err(format!("Некорректное смысловое поле: {field_id}"));
        }
        if !ids.insert(field_id.to_string()) {
            return Err(format!("Поле «{field_id}» добавлено в popup повторно"));
        }
        if field.title.trim().is_empty() {
            return Err(format!(
                "Для поля «{field_id}» не задан понятный текст вопроса"
            ));
        }
        let mut options = BTreeSet::<String>::new();
        for option in &field.options {
            let normalized = option.trim().to_lowercase();
            if normalized.is_empty() {
                continue;
            }
            if !options.insert(normalized) {
                return Err(format!("У поля «{field_id}» повторяется вариант ответа"));
            }
        }
    }

    let links = fields
        .iter()
        .filter_map(|field| {
            field
                .linked_to
                .as_deref()
                .map(str::trim)
                .filter(|linked| !linked.is_empty())
                .map(|linked| (field.field_id.trim().to_string(), linked.to_string()))
        })
        .collect::<BTreeMap<_, _>>();
    for (field_id, linked_to) in &links {
        if field_id == linked_to {
            return Err(format!("Поле «{field_id}» не может ссылаться само на себя"));
        }
        if !ids.contains(linked_to) {
            return Err(format!(
                "Поле «{field_id}» связано с отсутствующим полем «{linked_to}»"
            ));
        }
    }

    for start in links.keys() {
        let mut seen = BTreeSet::<String>::new();
        let mut current = start.as_str();
        while let Some(next) = links.get(current) {
            if !seen.insert(current.to_string()) {
                return Err(format!(
                    "Обнаружен цикл связанных popup-полей, начинающийся с «{start}»"
                ));
            }
            current = next;
        }
    }
    Ok(())
}

pub fn normalize_popup_fields(fields: &[PopupFieldConfig]) -> Vec<PopupFieldConfig> {
    let mut out = BTreeMap::<String, PopupFieldConfig>::new();
    for field in fields {
        if let Some(normalized) = normalize_popup_field(field.clone()) {
            out.insert(normalized.field_id.clone(), normalized);
        }
    }
    let mut values = out.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| a.order.cmp(&b.order).then(a.field_id.cmp(&b.field_id)));
    values
}

pub fn normalize_popup_field(mut config: PopupFieldConfig) -> Option<PopupFieldConfig> {
    config.field_id = config.field_id.trim().to_string();
    if config.field_id.is_empty() || !is_valid_field_id(&config.field_id) {
        return None;
    }
    config.title = config.title.trim().to_string();
    if config.title.is_empty() {
        config.title = title_for_field(&config.field_id);
    }
    config.options = config
        .options
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    config.help_text = config
        .help_text
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    config.section = config
        .section
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    config.default_value = config
        .default_value
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    config.linked_to = config
        .linked_to
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| is_valid_field_id(value) && value != &config.field_id);
    if config.order == 0 {
        config.order = popup_order(&config.field_id) as i32;
    }
    Some(config)
}

pub fn popup_config_for_field(
    field_id: &str,
    required: bool,
    category: &DomainKind,
    role_id: &str,
) -> PopupFieldConfig {
    let mut config = PopupFieldConfig::new(field_id, title_for_field(field_id));
    config.required = required;
    config.input_kind = infer_input_kind(field_id);
    config.order = popup_order(field_id) as i32;
    config.section = Some(domain_section(category).to_string());
    config.ask_mode = PromptAskMode::IfMissing;
    config.help_text = validation_hint_for(field_id, config.input_kind);
    if matches!(config.input_kind, PromptInputKind::YesNo) {
        config.options = vec!["Нет".into(), "Да".into()];
    }
    if should_ask_fresh_each_run(field_id, role_id) {
        config.ask_mode = PromptAskMode::Always;
    } else if should_confirm_each_run(field_id, role_id) {
        config.ask_mode = PromptAskMode::Confirm;
    }
    if is_document_date(field_id) {
        config.default_value = Some("@today".into());
    }
    config
}

pub fn resolve_popup_default(value: Option<&str>) -> Option<String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some("@today") => Some(Local::now().date_naive().format("%d.%m.%Y").to_string()),
        Some("@current_year") => Some(Local::now().date_naive().format("%Y").to_string()),
        Some(value) => Some(value.to_string()),
        None => None,
    }
}

pub fn infer_input_kind(field_id: &str) -> PromptInputKind {
    let id = field_id.to_lowercase();
    if id.contains("diagnosis") || id.contains("icd10") || id.ends_with(".icd") {
        return PromptInputKind::Icd10;
    }
    if id.ends_with(".inn") || id == "org.inn" || id.contains("inn_") {
        return PromptInputKind::Inn;
    }
    if id.ends_with(".kpp") || id == "org.kpp" {
        return PromptInputKind::Kpp;
    }
    if id.ends_with(".ogrn") || id == "org.ogrn" {
        return PromptInputKind::Ogrn;
    }
    if id.ends_with(".snils") || id.contains("snils") {
        return PromptInputKind::Snils;
    }
    if id.contains("passport") {
        return PromptInputKind::Passport;
    }
    if id.ends_with(".vin") || id.contains("vehicle.vin") {
        return PromptInputKind::Vin;
    }
    if id.contains("date") || id.ends_with(".from") || id.ends_with(".until") {
        return PromptInputKind::Date;
    }
    if id.contains("amount") || id.contains("salary") || id.contains("price") || id.contains("cost")
    {
        return PromptInputKind::Money;
    }
    if id.contains("count") || id.contains("quantity") || id.ends_with(".days") {
        return PromptInputKind::Number;
    }
    if id.contains("treatment")
        || id.contains("complaints")
        || id.contains("anamnesis")
        || id.contains("conclusion")
        || id.contains("recommend")
        || id.contains("condition")
        || id.contains("status")
        || id.contains("labs")
        || id.contains("subject")
        || id.contains("notes")
        || id.contains("description")
    {
        return PromptInputKind::LongText;
    }
    PromptInputKind::Text
}

fn apply_profession_defaults(config: &mut PopupFieldConfig, category: &DomainKind, role_id: &str) {
    let id = config.field_id.as_str();
    match category {
        DomainKind::Medical => {
            config.section = Some("Медицинские данные".into());
            if id == "medical.rvk_commissariat" {
                config.input_kind = PromptInputKind::Select;
                config.options = vec![
                    "Районный военный комиссариат".into(),
                    "Городской военный комиссариат".into(),
                    "Областной военный комиссариат".into(),
                ];
                config.allow_custom_option = true;
            }
            if id == "medical.icd10" || id == "medical.diagnosis_code" {
                config.input_kind = PromptInputKind::Icd10;
            }
            if matches!(
                id,
                "medical.protocol_date" | "medical.sick_leave_commission_date"
            ) {
                config.linked_to = Some("medical.commission_date".into());
                config.help_text =
                    Some("Изначально повторяет дату комиссии; поле можно изменить вручную".into());
            }
            if role_id.contains("diar") && id == "medical.discharge_date" {
                config.help_text = Some("Записи не будут создаваться после даты выписки".into());
            }
        }
        DomainKind::Legal => config.section = Some("Реквизиты юридического документа".into()),
        DomainKind::Hr => config.section = Some("Кадровые данные".into()),
        DomainKind::Accounting => {
            config.section = Some("Бухгалтерские реквизиты".into());
            if id == "accounting.currency" {
                config.input_kind = PromptInputKind::Select;
                config.options = vec!["RUB".into(), "USD".into(), "EUR".into(), "CNY".into()];
                config.allow_custom_option = true;
            }
        }
        DomainKind::Education => config.section = Some("Данные обучающегося и документа".into()),
        DomainKind::Generic | DomainKind::Custom(_) => {
            config.section = Some("Данные документа".into())
        }
    }
}

fn profession_role_fields(category: &DomainKind, role_id: &str) -> Vec<PopupFieldConfig> {
    let role = role_id.to_lowercase();
    let mut fields = Vec::new();
    let mut add = |field_id: &str, required: bool| {
        let mut config = popup_config_for_field(field_id, required, category, role_id);
        apply_profession_defaults(&mut config, category, role_id);
        fields.push(config);
    };
    match category {
        DomainKind::Medical => {
            if role.contains("discharge") || role.contains("выпис") {
                add("medical.case_number", true);
                add("medical.discharge_date", true);
                add("medical.diagnosis", true);
                add("medical.treatment", true);
                add("medical.discharge_condition", false);
                add("medical.recommendations", false);
            } else if role.contains("rvk") || role.contains("рвк") {
                add("medical.case_number", true);
                add("medical.discharge_date", true);
                add("medical.rvk_act_number", true);
                add("medical.rvk_commissariat", true);
                add("medical.diagnosis", true);
                add("medical.treatment", false);
            } else if role.contains("vk_mse") || role.contains("мсэ") {
                add("medical.case_number", true);
                add("medical.commission_date", true);
                add("medical.protocol_number", true);
                add("medical.protocol_date", true);
                add("medical.workplace", true);
                add("medical.position", false);
            } else if role.contains("sick_leave_vk") || role.contains("больнич") {
                add("medical.case_number", true);
                add("medical.commission_date", true);
                add("medical.protocol_number", true);
                add("medical.protocol_date", true);
                add("medical.sick_leave_commission_date", true);
                add("medical.sick_leave_number", true);
                add("medical.workplace", true);
                add("medical.position", false);
            } else if role.contains("commission") || role.contains("совмест") {
                add("medical.case_number", true);
                add("medical.commission_date", true);
                add("medical.commission_number", true);
            } else if role.contains("diar") || role.contains("днев") {
                add("medical.admission_date", true);
                add("medical.discharge_date", true);
                add("medical.diagnosis", true);
                add("medical.treatment", false);
            } else if role.contains("primary")
                || role.contains("reception")
                || role.contains("первич")
            {
                add("medical.case_number", true);
                add("medical.admission_date", true);
                add("medical.diagnosis", true);
                add("medical.treatment", false);
            }
        }
        DomainKind::Legal => {
            if role.contains("contract") || role.contains("договор") || role.contains("контракт")
            {
                add("contract.number", true);
                add("contract.date", true);
                add("contract.party_a", true);
                add("contract.party_b", true);
                add("contract.subject", false);
                add("contract.amount", false);
                add("contract.start_date", false);
                add("contract.end_date", false);
            } else if role.contains("claim") || role.contains("претенз") {
                add("document.number", true);
                add("document.date", true);
                add("organization.name", true);
                add("subject.name", true);
                add("legal.claim_subject", true);
                add("legal.claim_amount", false);
            } else if role.contains("act") || role.contains("акт") {
                add("document.number", true);
                add("document.date", true);
                add("contract.number", false);
                add("contract.date", false);
            }
        }
        DomainKind::Hr => {
            if role.contains("order") || role.contains("приказ") {
                add("hr.order_number", true);
                add("hr.order_date", true);
                add("person.full_name", true);
                add("employee.position", true);
                add("employee.department", false);
                add("employee.hire_date", false);
                add("employee.salary", false);
                add("employee.contract_number", false);
            }
        }
        DomainKind::Accounting => {
            if role.contains("invoice") || role.contains("счет") || role.contains("счёт") {
                add("accounting.invoice_number", true);
                add("accounting.invoice_date", true);
                add("accounting.client", true);
                add("org.inn", false);
                add("org.kpp", false);
                add("amount.total", true);
                add("amount.vat", false);
                add("accounting.currency", false);
            }
        }
        DomainKind::Education => {
            if role.contains("certificate") || role.contains("справ") {
                add("education.student_name", true);
                add("education.group", false);
                add("document.number", true);
                add("document.date", true);
                add("education.institution", false);
            }
        }
        DomainKind::Generic | DomainKind::Custom(_) => {}
    }
    fields
}

fn validation_hint_for(field_id: &str, kind: PromptInputKind) -> Option<String> {
    let text = match kind {
        PromptInputKind::Date => "Дата: ДД.ММ.ГГГГ",
        PromptInputKind::Inn => "ИНН: 10 или 12 цифр, контрольное число проверяется",
        PromptInputKind::Kpp => "КПП: 9 знаков",
        PromptInputKind::Ogrn => "ОГРН/ОГРНИП: 13 или 15 цифр",
        PromptInputKind::Snils => "СНИЛС: 11 цифр, контрольное число проверяется",
        PromptInputKind::Vin => "VIN: 17 символов без I, O, Q",
        PromptInputKind::Money => "Сумма: число, допустима запятая или точка",
        PromptInputKind::Number => "Введите число",
        PromptInputKind::Select => "Выберите вариант или введите свой, если это разрешено",
        _ if field_id.contains("number") => "Укажите актуальный номер для этого комплекта",
        _ => return None,
    };
    Some(text.into())
}

fn is_document_date(field_id: &str) -> bool {
    matches!(
        field_id,
        "document.date"
            | "contract.date"
            | "legal.contract_date"
            | "hr.order_date"
            | "accounting.invoice_date"
            | "medical.commission_date"
            | "medical.protocol_date"
            | "medical.sick_leave_commission_date"
    )
}

fn should_ask_fresh_each_run(field_id: &str, role_id: &str) -> bool {
    let role = role_id.trim().to_lowercase();
    match field_id {
        // These identify the document being CREATED. They must never silently inherit
        // the number/date of the source document (for example, an act from a contract).
        "document.number" | "document.date" => true,
        "contract.number" | "contract.date" => {
            role.contains("contract") || role.contains("договор") || role.contains("контракт")
        }
        "accounting.invoice_number" | "accounting.invoice_date" => {
            role.contains("invoice") || role.contains("счёт") || role.contains("счет")
        }
        "hr.order_number" | "hr.order_date" => {
            role.contains("order") || role.contains("приказ")
        }
        "medical.rvk_act_number" => role.contains("rvk") || role.contains("рвк"),
        "medical.commission_number" | "medical.commission_date" => {
            role.contains("commission") || role.contains("комисс") || role.contains("совмест")
        }
        "medical.protocol_number" | "medical.protocol_date" => {
            role.contains("vk_mse") || role.contains("мсэ") || role.contains("sick_leave_vk")
        }
        "medical.sick_leave_number" | "medical.sick_leave_commission_date" => true,
        _ => false,
    }
}

fn should_confirm_each_run(field_id: &str, role_id: &str) -> bool {
    let role = role_id.to_lowercase();
    is_document_date(field_id)
        || field_id.ends_with(".order_number")
        || field_id.ends_with(".invoice_number")
        || field_id == "contract.number"
        || field_id == "medical.rvk_act_number"
        || field_id == "medical.protocol_number"
        || field_id == "medical.commission_number"
        || field_id == "medical.sick_leave_number"
        || (role.contains("commission") && field_id.contains("number"))
}

fn domain_section(category: &DomainKind) -> &'static str {
    match category {
        DomainKind::Medical => "Медицинские данные",
        DomainKind::Legal => "Юридические данные",
        DomainKind::Hr => "Кадровые данные",
        DomainKind::Accounting => "Бухгалтерские данные",
        DomainKind::Education => "Образовательные данные",
        DomainKind::Generic | DomainKind::Custom(_) => "Данные документа",
    }
}

pub fn popup_order(field_id: &str) -> usize {
    match field_id {
        "document.number"
        | "contract.number"
        | "legal.contract_number"
        | "hr.order_number"
        | "accounting.invoice_number"
        | "medical.case_number" => 10,
        "document.date"
        | "contract.date"
        | "legal.contract_date"
        | "hr.order_date"
        | "accounting.invoice_date"
        | "medical.admission_date" => 20,
        "period.start_date" | "contract.start_date" => 30,
        "period.end_date" | "contract.end_date" | "medical.discharge_date" => 40,
        "subject.name" | "person.full_name" | "hr.employee_name" | "education.student_name" => 50,
        "organization.name" | "org.name" | "accounting.client" => 60,
        "contract.party_a" | "legal.party_a" => 70,
        "contract.party_b" | "legal.party_b" => 80,
        "org.inn" | "accounting.inn" => 90,
        "org.kpp" | "accounting.kpp" => 100,
        "medical.diagnosis" | "medical.icd10" | "medical.diagnosis_code" => 110,
        "medical.treatment" => 120,
        "medical.commission_date"
        | "medical.protocol_date"
        | "medical.sick_leave_commission_date" => 130,
        "medical.protocol_number" | "medical.commission_number" | "medical.rvk_act_number" => 140,
        "medical.rvk_commissariat" => 150,
        "medical.workplace" | "employee.department" => 160,
        "medical.position" | "employee.position" | "hr.position" => 170,
        "amount.total" | "accounting.amount_total" | "contract.amount" | "legal.amount" => 180,
        _ => 500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_contract_gets_profession_specific_prompts() {
        let document = DocumentTemplateSpec {
            id: "contract".into(),
            button_label: "Договор".into(),
            template_path: "contract.docx".into(),
            category: DomainKind::Legal,
            role_id: "contract".into(),
            required_fields: vec!["contract.number".into(), "contract.date".into()],
            placeholders: vec!["contract.number".into(), "contract.date".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        let fields = default_popup_fields_for_document(&document);
        assert!(fields
            .iter()
            .any(|field| field.field_id == "contract.party_a"));
        assert!(fields
            .iter()
            .any(|field| field.field_id == "contract.party_b"));
        assert!(fields.iter().any(|field| field.field_id == "contract.date"
            && field.default_value.as_deref() == Some("@today")));
    }

    #[test]
    fn created_document_identity_is_always_fresh() {
        let number = popup_config_for_field(
            "document.number",
            true,
            &DomainKind::Legal,
            "acceptance_act",
        );
        let date = popup_config_for_field(
            "document.date",
            true,
            &DomainKind::Legal,
            "acceptance_act",
        );
        assert_eq!(number.ask_mode, PromptAskMode::Always);
        assert_eq!(date.ask_mode, PromptAskMode::Always);
        assert_eq!(date.default_value.as_deref(), Some("@today"));
    }

    #[test]
    fn referenced_contract_identity_is_confirmed_not_cleared_for_an_act() {
        let number = popup_config_for_field(
            "contract.number",
            false,
            &DomainKind::Legal,
            "acceptance_act",
        );
        let date = popup_config_for_field(
            "contract.date",
            false,
            &DomainKind::Legal,
            "acceptance_act",
        );
        assert_eq!(number.ask_mode, PromptAskMode::Confirm);
        assert_eq!(date.ask_mode, PromptAskMode::Confirm);
    }

    #[test]
    fn user_popup_config_overrides_automatic_config() {
        let mut document = DocumentTemplateSpec {
            id: "x".into(),
            button_label: "X".into(),
            template_path: "x.docx".into(),
            category: DomainKind::Generic,
            role_id: "document".into(),
            required_fields: vec!["document.number".into()],
            placeholders: vec!["document.number".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        let mut custom = PopupFieldConfig::new("document.number", "Мой номер");
        custom.ask_mode = PromptAskMode::Always;
        document.popup_fields.push(custom);
        let fields = effective_popup_fields(&document);
        let number = fields
            .iter()
            .find(|field| field.field_id == "document.number")
            .unwrap();
        assert_eq!(number.title, "Мой номер");
        assert_eq!(number.ask_mode, PromptAskMode::Always);
    }

    #[test]
    fn explicitly_configured_popup_can_remove_optional_profession_defaults() {
        let mut document = DocumentTemplateSpec {
            id: "contract".into(),
            button_label: "Договор".into(),
            template_path: "contract.docx".into(),
            category: DomainKind::Legal,
            role_id: "contract".into(),
            required_fields: vec!["contract.number".into(), "contract.date".into()],
            placeholders: vec!["contract.number".into(), "contract.date".into()],
            is_static_copy: false,
            popup_fields: vec![popup_config_for_field(
                "contract.number",
                true,
                &DomainKind::Legal,
                "contract",
            )],
            popup_configured: true,
        };
        document.popup_fields[0].title = "Номер моего договора".into();
        let fields = effective_popup_fields(&document);
        assert!(fields
            .iter()
            .any(|field| field.field_id == "contract.number"));
        assert!(fields.iter().any(|field| field.field_id == "contract.date"));
        assert!(!fields
            .iter()
            .any(|field| field.field_id == "contract.subject"));
    }

    #[test]
    fn medical_protocol_date_is_linked_but_independently_editable() {
        let config = popup_config_for_field(
            "medical.protocol_date",
            true,
            &DomainKind::Medical,
            "vk_mse",
        );
        let mut configured = config;
        apply_profession_defaults(&mut configured, &DomainKind::Medical, "vk_mse");
        assert_eq!(
            configured.linked_to.as_deref(),
            Some("medical.commission_date")
        );
        assert_eq!(configured.input_kind, PromptInputKind::Date);
    }
}
