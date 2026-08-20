use crate::domains::medical_document_plan::{build_medical_render_plan, MedicalDocumentRole};
use crate::domains::medical_semantics::{
    SICK_LEAVE_VK_COMMISSION_DATE, SICK_LEAVE_VK_POSITION, SICK_LEAVE_VK_PROTOCOL_DATE,
    SICK_LEAVE_VK_PROTOCOL_NUMBER, SICK_LEAVE_VK_WORKPLACE, VK_MSE_COMMISSION_DATE,
    VK_MSE_POSITION, VK_MSE_PROTOCOL_DATE, VK_MSE_PROTOCOL_NUMBER, VK_MSE_WORKPLACE,
};
use crate::professional_records::{
    DIARY_DAY_END_TIME, DIARY_DAY_START_TIME, DIARY_INTRADAY_RHYTHM, DIARY_SCHEDULE_STYLE,
};
use crate::{
    canonical_storage_field_id, is_valid_field_id, title_for_field, DocumentTemplateSpec,
    DomainKind, PopupFieldConfig, PromptAskMode, PromptInputKind,
};
use chrono::Local;
use std::collections::{BTreeMap, BTreeSet};

pub fn default_popup_fields_for_document(document: &DocumentTemplateSpec) -> Vec<PopupFieldConfig> {
    let mut required = document
        .required_fields
        .iter()
        .map(|field| canonical_storage_field_id(field))
        .collect::<BTreeSet<_>>();
    let mut ordered = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .filter(|field| !field.trim().is_empty())
        .map(|field| canonical_storage_field_id(field))
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
        let Some(mut normalized) = normalize_popup_field(config.clone()) else {
            continue;
        };
        if !document.popup_configured {
            normalized.input_kind = infer_input_kind(&normalized.field_id);
            normalized.help_text = validation_hint_for(&normalized.field_id, normalized.input_kind);
            apply_profession_defaults(&mut normalized, &document.category, &document.role_id);
        }
        merged.insert(normalized.field_id.clone(), normalized);
    }
    // Runtime controls are part of the profession workflow itself, not merely a
    // template-designer convenience. A previously customized popup must therefore
    // never be allowed to hide the doctor's diary schedule/rhythm confirmation.
    for field_id in profession_runtime_control_fields(&document.category, &document.role_id) {
        if merged.contains_key(&field_id) {
            continue;
        }
        let required = matches!(
            field_id.as_str(),
            DIARY_SCHEDULE_STYLE | DIARY_INTRADAY_RHYTHM
        );
        let mut config =
            popup_config_for_field(&field_id, required, &document.category, &document.role_id);
        apply_profession_defaults(&mut config, &document.category, &document.role_id);
        merged.insert(field_id, config);
    }

    // Fail closed: even a custom popup cannot hide a field that the selected template
    // or workflow has declared strictly required.
    for field_id in &document.required_fields {
        if !is_valid_field_id(field_id) {
            continue;
        }
        let canonical = canonical_storage_field_id(field_id);
        if merged.contains_key(&canonical) {
            continue;
        }
        let mut config =
            popup_config_for_field(&canonical, true, &document.category, &document.role_id);
        apply_profession_defaults(&mut config, &document.category, &document.role_id);
        merged.insert(canonical, config);
    }
    let document_uses_labs = document
        .placeholders
        .iter()
        .chain(document.required_fields.iter())
        .any(|field_id| canonical_storage_field_id(field_id) == "medical.labs");
    if matches!(document.category, DomainKind::Medical)
        && document_uses_labs
        && !merged.contains_key("medical.labs_without")
    {
        let mut config = popup_config_for_field(
            "medical.labs_without",
            false,
            &document.category,
            &document.role_id,
        );
        apply_profession_defaults(&mut config, &document.category, &document.role_id);
        config.ask_mode = PromptAskMode::Always;
        config.help_text = Some(
            "Выберите «Да», если исследований действительно нет; в документ будет записано «Нет анализов»."
                .into(),
        );
        merged.insert("medical.labs_without".into(), config);
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
        let raw_field_id = field.field_id.trim();
        if !is_valid_field_id(raw_field_id) {
            return Err(format!("Некорректное смысловое поле: {raw_field_id}"));
        }
        let field_id = canonical_storage_field_id(raw_field_id);
        if !ids.insert(field_id.clone()) {
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
                .map(|linked| {
                    (
                        canonical_storage_field_id(field.field_id.trim()),
                        canonical_storage_field_id(linked),
                    )
                })
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
    let raw_field_id = config.field_id.trim();
    if raw_field_id.is_empty() || !is_valid_field_id(raw_field_id) {
        return None;
    }
    config.field_id = canonical_storage_field_id(raw_field_id);
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
        .filter(|value| is_valid_field_id(value))
        .map(|value| canonical_storage_field_id(&value))
        .filter(|value| value != &config.field_id);
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
    let canonical = canonical_storage_field_id(field_id);
    let mut config = PopupFieldConfig::new(&canonical, title_for_field(&canonical));
    config.required = required;
    config.input_kind = infer_input_kind(&canonical);
    config.order = popup_order(&canonical) as i32;
    config.section = Some(domain_section(category).to_string());
    config.ask_mode = PromptAskMode::IfMissing;
    config.help_text = validation_hint_for(&canonical, config.input_kind);
    if matches!(config.input_kind, PromptInputKind::YesNo) {
        config.options = vec!["Нет".into(), "Да".into()];
    }
    if should_ask_fresh_each_run(&canonical, role_id) {
        config.ask_mode = PromptAskMode::Always;
    } else if should_confirm_each_run(&canonical, role_id) {
        config.ask_mode = PromptAskMode::Confirm;
    }
    if is_document_date(&canonical) {
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
    let id = canonical_storage_field_id(field_id).to_lowercase();
    if id == "medical.labs_without" {
        return PromptInputKind::YesNo;
    }
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
    let leaf = id.rsplit('.').next().unwrap_or(id.as_str());
    let is_money_field = matches!(
        leaf,
        "amount" | "total" | "salary" | "price" | "cost" | "fee" | "vat"
    ) || leaf.ends_with("_amount")
        || leaf.ends_with("_total")
        || leaf.ends_with("_salary")
        || leaf.ends_with("_price")
        || leaf.ends_with("_cost");
    if is_money_field {
        return PromptInputKind::Money;
    }
    let is_numeric_segment = matches!(leaf, "count" | "quantity" | "days")
        || leaf.ends_with("_count")
        || leaf.ends_with("_quantity");
    if is_numeric_segment {
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
                // Region-specific quick options are injected by the medical profile
                // configuration in the desktop runtime. The universal core only
                // defines the field contract and always preserves manual entry.
                config.options.clear();
                config.allow_custom_option = true;
                config.help_text = Some(
                    "Быстрые варианты задаются в медицинском профиле; можно ввести значение вручную.".into(),
                );
            }
            if id == "medical.icd10" || id == "medical.diagnosis_code" {
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
                // Donor contract: the specialist confirms the diary style for every
                // diary run. Never silently turn an absent answer into daily diaries.
                config.ask_mode = PromptAskMode::Always;
                config.required = true;
                config.default_value = None;
                config.help_text = Some(
                    "Выберите стиль как в рабочем Dokkomplekt: каждый день; 1, 2, 3, 7, затем 2 раза в неделю; каждый день по времени; либо введите свои дни, например 1, 4, 9.".into(),
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
                // The second donor popup is also a specialist confirmation, even when
                // the answer is "Один раз в день".
                config.ask_mode = PromptAskMode::Always;
                config.required = true;
                config.default_value = None;
                config.help_text = Some(
                    "Подтвердите ритм: один раз в день, каждые 4 часа, каждый час, 30/15/5 минут либо свой интервал/список времени.".into(),
                );
            }
            if matches!(id, DIARY_DAY_START_TIME | DIARY_DAY_END_TIME) {
                config.input_kind = PromptInputKind::Text;
                config.help_text = Some(
                    "ЧЧ:ММ. Нужен для ритма в минутах/часах; без явных границ внутридневная серия не создаётся".into(),
                );
            }
            let linked_commission = match id {
                VK_MSE_PROTOCOL_DATE => Some(VK_MSE_COMMISSION_DATE),
                SICK_LEAVE_VK_PROTOCOL_DATE => Some(SICK_LEAVE_VK_COMMISSION_DATE),
                "medical.protocol_date" | "medical.sick_leave_commission_date" => {
                    Some("medical.commission_date")
                }
                _ => None,
            };
            if let Some(linked_to) = linked_commission {
                config.linked_to = Some(linked_to.into());
                config.help_text = Some(
                    "Изначально повторяет дату своей комиссии; поле можно изменить вручную".into(),
                );
            }
            if role_id.contains("diar") && id == "medical.discharge_date" {
                config.help_text = Some("Записи не будут создаваться после даты выписки".into());
            }
        }
        DomainKind::Legal => config.section = Some("Реквизиты юридического документа".into()),
        DomainKind::Hr => config.section = Some("Кадровые данные".into()),
        DomainKind::Accounting => {
            config.section = Some("Бухгалтерские реквизиты".into());
            if id == "amount.currency" {
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
            // One source of truth: popup requirements come from the same
            // Medical role plan as the universal pipeline and completeness gate.
            let plan =
                build_medical_render_plan(MedicalDocumentRole::from_role_id(role_id), false, false);
            for field_id in &plan.required_fields {
                add(field_id, true);
            }
            for field_id in &plan.optional_fields {
                add(field_id, false);
            }
            if matches!(plan.role, MedicalDocumentRole::DischargeEpicrisis) {
                add("medical.discharge_condition", false);
                add("medical.recommendations", false);
            }
            if matches!(plan.role, MedicalDocumentRole::Diary) {
                // Same fail-closed contract as the donor wizard: style and rhythm
                // must be confirmed by the doctor before diaries can be generated.
                add(DIARY_SCHEDULE_STYLE, true);
                add(DIARY_INTRADAY_RHYTHM, true);
                add(DIARY_DAY_START_TIME, false);
                add(DIARY_DAY_END_TIME, false);
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
                add("counterparty.name", true);
                add("org.inn", false);
                add("org.kpp", false);
                add("amount.total", true);
                add("amount.vat", false);
                add("amount.currency", false);
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

pub fn profession_runtime_control_fields(category: &DomainKind, role_id: &str) -> BTreeSet<String> {
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
            | VK_MSE_COMMISSION_DATE
            | VK_MSE_PROTOCOL_DATE
            | SICK_LEAVE_VK_COMMISSION_DATE
            | SICK_LEAVE_VK_PROTOCOL_DATE
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
        "hr.order_number" | "hr.order_date" => role.contains("order") || role.contains("приказ"),
        "medical.rvk_act_number" => role.contains("rvk") || role.contains("рвк"),
        "medical.commission_number" | "medical.commission_date" => {
            role.contains("commission") || role.contains("комисс") || role.contains("совмест")
        }
        "medical.protocol_number" | "medical.protocol_date" => {
            role.contains("vk_mse") || role.contains("мсэ") || role.contains("sick_leave_vk")
        }
        VK_MSE_COMMISSION_DATE | VK_MSE_PROTOCOL_NUMBER | VK_MSE_PROTOCOL_DATE => {
            role.contains("vk_mse") || role.contains("мсэ")
        }
        SICK_LEAVE_VK_COMMISSION_DATE
        | SICK_LEAVE_VK_PROTOCOL_NUMBER
        | SICK_LEAVE_VK_PROTOCOL_DATE => role.contains("sick_leave_vk"),
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
        || field_id == VK_MSE_PROTOCOL_NUMBER
        || field_id == SICK_LEAVE_VK_PROTOCOL_NUMBER
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
        "organization.name" | "org.name" | "counterparty.name" | "accounting.client" => 60,
        "contract.party_a" | "legal.party_a" => 70,
        "contract.party_b" | "legal.party_b" => 80,
        "org.inn" | "accounting.inn" => 90,
        "org.kpp" | "accounting.kpp" => 100,
        "medical.diagnosis" | "medical.icd10" | "medical.diagnosis_code" => 110,
        "medical.treatment" => 120,
        "medical.labs" => 121,
        "medical.labs_without" => 122,
        DIARY_SCHEDULE_STYLE => 123,
        DIARY_INTRADAY_RHYTHM => 124,
        DIARY_DAY_START_TIME => 125,
        DIARY_DAY_END_TIME => 126,
        "medical.commission_date"
        | "medical.protocol_date"
        | "medical.sick_leave_commission_date"
        | VK_MSE_COMMISSION_DATE
        | VK_MSE_PROTOCOL_DATE
        | SICK_LEAVE_VK_COMMISSION_DATE
        | SICK_LEAVE_VK_PROTOCOL_DATE => 130,
        "medical.protocol_number"
        | "medical.commission_number"
        | "medical.rvk_act_number"
        | VK_MSE_PROTOCOL_NUMBER
        | SICK_LEAVE_VK_PROTOCOL_NUMBER => 140,
        "medical.rvk_commissariat" => 150,
        "medical.workplace"
        | VK_MSE_WORKPLACE
        | SICK_LEAVE_VK_WORKPLACE
        | "employee.department" => 160,
        "medical.position"
        | VK_MSE_POSITION
        | SICK_LEAVE_VK_POSITION
        | "employee.position"
        | "hr.position" => 170,
        "amount.total" | "accounting.amount_total" | "contract.amount" | "legal.amount" => 180,
        "amount.currency" | "accounting.currency" => 190,
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
        let date =
            popup_config_for_field("document.date", true, &DomainKind::Legal, "acceptance_act");
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
        let date =
            popup_config_for_field("contract.date", false, &DomainKind::Legal, "acceptance_act");
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
        for (role, protocol_date, commission_date) in [
            ("vk_mse", VK_MSE_PROTOCOL_DATE, VK_MSE_COMMISSION_DATE),
            (
                "sick_leave_vk",
                SICK_LEAVE_VK_PROTOCOL_DATE,
                SICK_LEAVE_VK_COMMISSION_DATE,
            ),
        ] {
            let mut configured =
                popup_config_for_field(protocol_date, true, &DomainKind::Medical, role);
            apply_profession_defaults(&mut configured, &DomainKind::Medical, role);
            assert_eq!(configured.linked_to.as_deref(), Some(commission_date));
            assert_eq!(configured.input_kind, PromptInputKind::Date);
            assert_eq!(configured.ask_mode, PromptAskMode::Always);
        }
    }

    #[test]
    fn medical_role_popups_follow_the_canonical_role_plan() {
        for role in [
            "primary",
            "discharge",
            "diaries",
            "rvk_act",
            "commission",
            "sick_leave_vk",
            "vk_mse",
            "reception",
        ] {
            let document = DocumentTemplateSpec {
                id: role.into(),
                button_label: role.into(),
                template_path: format!("{role}.docx"),
                category: DomainKind::Medical,
                role_id: role.into(),
                required_fields: Vec::new(),
                placeholders: Vec::new(),
                is_static_copy: false,
                popup_fields: Vec::new(),
                popup_configured: false,
            };
            let fields = default_popup_fields_for_document(&document);
            let plan =
                build_medical_render_plan(MedicalDocumentRole::from_role_id(role), false, false);
            for required in plan.required_fields {
                let config = fields
                    .iter()
                    .find(|field| field.field_id == required)
                    .unwrap_or_else(|| panic!("{role}: popup misses required {required}"));
                assert!(config.required, "{role}: {required} is not required");
            }
            for optional in plan.optional_fields {
                assert!(
                    fields.iter().any(|field| field.field_id == optional),
                    "{role}: popup misses optional {optional}"
                );
            }
        }
    }

    #[test]
    fn reception_does_not_inherit_primary_treatment_prompt() {
        let document = DocumentTemplateSpec {
            id: "reception".into(),
            button_label: "Осмотр врача приёмного покоя".into(),
            template_path: "reception.docx".into(),
            category: DomainKind::Medical,
            role_id: "reception".into(),
            required_fields: Vec::new(),
            placeholders: Vec::new(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };
        let fields = default_popup_fields_for_document(&document);
        assert!(fields
            .iter()
            .any(|field| field.field_id == "medical.admission_date" && field.required));
        assert!(!fields
            .iter()
            .any(|field| field.field_id == "medical.treatment"));
    }

    #[test]
    fn accounting_namespace_does_not_force_number_input() {
        assert_eq!(
            infer_input_kind("accounting.invoice_number"),
            PromptInputKind::Text
        );
        assert_eq!(infer_input_kind("accounting.client"), PromptInputKind::Text);
        assert_eq!(
            infer_input_kind("accounting.currency"),
            PromptInputKind::Text
        );
        assert_eq!(infer_input_kind("items.quantity"), PromptInputKind::Number);
        assert_eq!(
            infer_input_kind("items.item_count"),
            PromptInputKind::Number
        );
    }

    #[test]
    fn stale_automatic_accounting_popup_types_are_repaired_on_load() {
        let mut stale_client = PopupFieldConfig::new("accounting.client", "Клиент");
        stale_client.input_kind = PromptInputKind::Number;
        let mut stale_currency = PopupFieldConfig::new("accounting.currency", "Валюта");
        stale_currency.input_kind = PromptInputKind::Number;
        let document = DocumentTemplateSpec {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            template_path: "invoice.docx".into(),
            category: DomainKind::Accounting,
            role_id: "invoice".into(),
            required_fields: vec!["accounting.client".into()],
            placeholders: vec!["accounting.client".into(), "accounting.currency".into()],
            is_static_copy: false,
            popup_fields: vec![stale_client, stale_currency],
            popup_configured: false,
        };

        let fields = effective_popup_fields(&document);
        assert_eq!(
            fields
                .iter()
                .find(|field| field.field_id == "counterparty.name")
                .unwrap()
                .input_kind,
            PromptInputKind::Text
        );
        assert_eq!(
            fields
                .iter()
                .find(|field| field.field_id == "amount.currency")
                .unwrap()
                .input_kind,
            PromptInputKind::Select
        );
    }

    #[test]
    fn accounting_invoice_popup_uses_one_canonical_client_and_currency() {
        let document = DocumentTemplateSpec {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            template_path: "invoice.docx".into(),
            category: DomainKind::Accounting,
            role_id: "invoice".into(),
            required_fields: vec![
                "accounting.invoice_number".into(),
                "accounting.invoice_date".into(),
                "accounting.client".into(),
                "accounting.amount_total".into(),
            ],
            placeholders: vec![
                "accounting.invoice_number".into(),
                "accounting.invoice_date".into(),
                "counterparty.name".into(),
                "amount.total".into(),
                "accounting.currency".into(),
                "amount.currency".into(),
            ],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        };

        let fields = default_popup_fields_for_document(&document);
        assert_eq!(
            fields
                .iter()
                .filter(|field| field.field_id == "counterparty.name")
                .count(),
            1
        );
        assert_eq!(
            fields
                .iter()
                .filter(|field| field.field_id == "amount.currency")
                .count(),
            1
        );
        assert!(!fields.iter().any(|field| {
            matches!(
                field.field_id.as_str(),
                "accounting.client" | "accounting.currency" | "accounting.amount_total"
            )
        }));
        assert_eq!(
            fields
                .iter()
                .find(|field| field.field_id == "accounting.invoice_number")
                .unwrap()
                .input_kind,
            PromptInputKind::Text
        );
        assert_eq!(
            fields
                .iter()
                .find(|field| field.field_id == "counterparty.name")
                .unwrap()
                .input_kind,
            PromptInputKind::Text
        );
        let currency = fields
            .iter()
            .find(|field| field.field_id == "amount.currency")
            .unwrap();
        assert_eq!(currency.input_kind, PromptInputKind::Select);
        assert_eq!(currency.options, vec!["RUB", "USD", "EUR", "CNY"]);
    }

    #[test]
    fn popup_validation_rejects_alias_and_canonical_duplicate() {
        let legacy = PopupFieldConfig::new("accounting.client", "Клиент");
        let canonical = PopupFieldConfig::new("counterparty.name", "Контрагент");
        assert!(validate_popup_fields(&[legacy, canonical]).is_err());
    }
}
