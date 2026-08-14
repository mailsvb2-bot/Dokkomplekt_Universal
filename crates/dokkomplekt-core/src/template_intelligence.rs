use crate::{
    canonical_field_id_for_domain, inspect_template_syntax, is_valid_field_id, known_field_ids,
    template_field_references, DocumentTemplateSpec, DomainKind, TemplateAnalysis,
};
use std::collections::BTreeMap;

pub fn analyze_template_text(text: &str) -> TemplateAnalysis {
    let title = detect_title(text).unwrap_or_else(|| "Документ".to_string());
    let raw_placeholders = template_field_references(text);
    let initial_scores = score_domains(text, &raw_placeholders);
    let preferred_domain = domain_from_scores(&initial_scores);
    let placeholders = raw_placeholders
        .iter()
        .filter_map(|placeholder| {
            canonical_field_id_for_domain(placeholder, Some(&preferred_domain))
        })
        .collect::<Vec<_>>();
    let unknown_placeholders = raw_placeholders
        .iter()
        .filter(|placeholder| {
            canonical_field_id_for_domain(placeholder, Some(&preferred_domain)).is_none()
        })
        .cloned()
        .collect::<Vec<_>>();
    let known = known_field_ids();
    let custom_count = placeholders
        .iter()
        .filter(|placeholder| !known.contains(*placeholder) && is_valid_field_id(placeholder))
        .count();
    let domain_scores = score_domains(text, &placeholders);
    let role_id = detect_role(text, &title);
    let suggested_button_label = normalize_button_label(&title);
    let is_static = raw_placeholders.is_empty();
    let mut warnings = Vec::new();
    if is_static {
        warnings.push("Шаблон не содержит placeholder-полей: его нельзя использовать как динамический документ без явной разметки".to_string());
    }
    if custom_count > 0 {
        warnings.push(format!("Найдено пользовательских полей: {custom_count}. Они будут показаны в общем popup-плане."));
    }
    if !unknown_placeholders.is_empty() {
        warnings.push(format!(
            "Найдены небезопасные имена полей: {:?}. Их надо переименовать.",
            unknown_placeholders
        ));
    }
    TemplateAnalysis {
        title,
        suggested_button_label,
        placeholders,
        unknown_placeholders,
        domain_scores,
        role_id,
        is_static,
        warnings,
        template_errors: inspect_template_syntax(text),
    }
}

pub fn detect_title(text: &str) -> Option<String> {
    for raw in text.lines().take(20) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let stripped = strip_leading_date(line);
        if stripped.chars().any(|c| c.is_alphabetic()) {
            return Some(stripped.to_string());
        }
    }
    None
}

pub fn detect_placeholders(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let id = after[..end].trim();
            if !id.is_empty() && !result.iter().any(|x| x == id) {
                result.push(id.to_string());
            }
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    result
}

pub fn create_document_spec(
    document_id: &str,
    template_path: &str,
    analysis: &TemplateAnalysis,
    explicit_label: Option<&str>,
) -> DocumentTemplateSpec {
    let category = best_domain(analysis);
    let mut document = DocumentTemplateSpec {
        id: document_id.to_string(),
        button_label: explicit_label
            .unwrap_or(&analysis.suggested_button_label)
            .trim()
            .to_string(),
        template_path: template_path.to_string(),
        category,
        role_id: analysis.role_id.clone(),
        required_fields: analysis
            .placeholders
            .iter()
            .filter(|p| is_valid_field_id(p))
            .cloned()
            .collect(),
        placeholders: analysis.placeholders.clone(),
        is_static_copy: analysis.is_static,
        popup_fields: Vec::new(),
        popup_configured: false,
    };
    document.popup_fields = crate::default_popup_fields_for_document(&document);
    document
}

fn domain_from_scores(scores: &BTreeMap<String, usize>) -> DomainKind {
    match scores
        .iter()
        .max_by_key(|(_, score)| **score)
        .map(|(domain, _)| domain.as_str())
        .unwrap_or("generic")
    {
        "medical" => DomainKind::Medical,
        "legal" => DomainKind::Legal,
        "hr" => DomainKind::Hr,
        "education" => DomainKind::Education,
        "accounting" => DomainKind::Accounting,
        _ => DomainKind::Generic,
    }
}

pub fn best_domain(analysis: &TemplateAnalysis) -> DomainKind {
    domain_from_scores(&analysis.domain_scores)
}

fn score_domains(text: &str, placeholders: &[String]) -> BTreeMap<String, usize> {
    let lower = text.to_lowercase().replace('ё', "е");
    let mut scores = BTreeMap::new();
    scores.insert(
        "generic".to_string(),
        1 + placeholders
            .iter()
            .filter(|p| p.starts_with("custom.") || p.starts_with("data."))
            .count(),
    );
    let medical = [
        "диагноз",
        "лечение",
        "анамнез",
        "история болезни",
        "мкб",
        "выпис",
        "дневник",
        "комисс",
        "рвк",
        "мсэ",
        "больнич",
        "приемного покоя",
    ]
    .iter()
    .filter(|w| lower.contains(**w))
    .count()
        + placeholders
            .iter()
            .filter(|p| p.starts_with("medical."))
            .count()
            * 3;
    let legal = ["договор", "сторона", "заказчик", "исполнитель", "акт"]
        .iter()
        .filter(|w| lower.contains(**w))
        .count()
        + placeholders
            .iter()
            .filter(|p| p.starts_with("legal."))
            .count()
            * 3;
    let hr = ["сотрудник", "должность", "отдел", "приказ", "кадров"]
        .iter()
        .filter(|w| lower.contains(**w))
        .count()
        + placeholders.iter().filter(|p| p.starts_with("hr.")).count() * 3;
    let accounting = ["счет", "инн", "кпп", "сумма", "итого"]
        .iter()
        .filter(|w| lower.contains(**w))
        .count()
        + placeholders
            .iter()
            .filter(|p| p.starts_with("accounting."))
            .count()
            * 3;
    let education = [
        "обучение",
        "учащ",
        "студент",
        "оценк",
        "экзамен",
        "диплом",
        "ведомость",
    ]
    .iter()
    .filter(|w| lower.contains(**w))
    .count()
        + placeholders
            .iter()
            .filter(|p| p.starts_with("education."))
            .count()
            * 3;
    scores.insert("medical".to_string(), medical);
    scores.insert("legal".to_string(), legal);
    scores.insert("hr".to_string(), hr);
    scores.insert("accounting".to_string(), accounting);
    scores.insert("education".to_string(), education);
    scores
}

fn detect_role(text: &str, title: &str) -> String {
    // Generated-document roles belong to the professional profile. Keep the
    // recognizer conservative, but do not collapse distinct legacy forms into
    // a nearby medical role: their popup requisites and render contracts differ.
    let hay = format!("{}\n{}", title, text)
        .to_lowercase()
        .replace('ё', "е");
    if hay.contains("дневник") {
        "diaries".into()
    } else if hay.contains("выпис") || hay.contains("эпикриз") {
        "discharge".into()
    } else if hay.contains("рвк") || hay.contains("военный комиссариат") {
        "rvk_act".into()
    } else if hay.contains("вк больнич")
        || hay.contains("вк по больнич")
        || (hay.contains("продлен") && hay.contains("больнич"))
    {
        "sick_leave_vk".into()
    } else if hay.contains("мсэ") || hay.contains("вк на мсэ") {
        "vk_mse".into()
    } else if hay.contains("осмотр врача приемного покоя")
        || hay.contains("врач приемного покоя")
    {
        "reception".into()
    } else if hay.contains("совместный осмотр") || hay.contains("комиссионный осмотр") {
        "commission".into()
    } else if hay.contains("первичный осмотр") || hay.contains("направление на госпитализацию")
    {
        "primary".into()
    } else {
        "unknown".into()
    }
}

fn strip_leading_date(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() >= 10
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && matches!(bytes[2], b'.' | b'/' | b'-')
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && matches!(bytes[5], b'.' | b'/' | b'-')
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit()
    {
        return line[10..].trim();
    }
    line
}

fn normalize_button_label(title: &str) -> String {
    let mut label = title.trim().replace('\n', " ");
    while label.contains("  ") {
        label = label.replace("  ", " ");
    }
    if label.chars().count() > 42 {
        label = label.chars().take(42).collect();
    }
    if label.is_empty() {
        "Документ".into()
    } else {
        label
    }
}

#[cfg(test)]
mod alias_regression_tests {
    use super::*;

    #[test]
    fn button_label_truncation_preserves_utf8_boundaries() {
        let title = "Очень длинное русское название документа для безопасной кнопки 🧾";
        let label = normalize_button_label(title);
        assert_eq!(label.chars().count(), 42);
        assert!(title.starts_with(label.as_str()));
        assert!(std::str::from_utf8(label.as_bytes()).is_ok());
    }

    #[test]
    fn short_unicode_button_label_is_unchanged() {
        assert_eq!(
            normalize_button_label("Выписной эпикриз 🧾"),
            "Выписной эпикриз 🧾"
        );
    }

    #[test]
    fn human_placeholders_become_canonical_fields() {
        let analysis = analyze_template_text(
            "Выписной документ\n{{ФИО}} {{История болезни №}} {{Дата выписки}} {{Код МКБ-10}}",
        );
        assert_eq!(
            analysis.placeholders,
            vec![
                "subject.name",
                "medical.case_number",
                "medical.discharge_date",
                "medical.icd10",
            ]
        );
        assert!(analysis.unknown_placeholders.is_empty());
    }

    #[test]
    fn ambiguous_position_is_resolved_by_document_domain() {
        let hr = analyze_template_text("Приказ о сотруднике\n{{Должность}}");
        assert_eq!(hr.placeholders, vec!["employee.position"]);
        let medical = analyze_template_text("Выписной эпикриз\n{{Должность}}");
        assert_eq!(medical.placeholders, vec!["medical.position"]);
    }

    #[test]
    fn legacy_medical_templates_keep_distinct_generated_document_roles() {
        let sick_leave = analyze_template_text("ВК больничный\nВыписка из ПРОТОКОЛА №");
        assert_eq!(sick_leave.role_id, "sick_leave_vk");
        assert_eq!(best_domain(&sick_leave), DomainKind::Medical);

        let reception = analyze_template_text("Осмотр врача приёмного покоя\nЖалобы:");
        assert_eq!(reception.role_id, "reception");
        assert_eq!(best_domain(&reception), DomainKind::Medical);

        let mse = analyze_template_text("ВК на МСЭ\nВыписка из ПРОТОКОЛА №");
        assert_eq!(mse.role_id, "vk_mse");
        assert_eq!(best_domain(&mse), DomainKind::Medical);

        let commission = analyze_template_text("Совместный осмотр\nДата комиссии");
        assert_eq!(commission.role_id, "commission");
        assert_eq!(best_domain(&commission), DomainKind::Medical);
    }
}