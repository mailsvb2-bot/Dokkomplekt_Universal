use crate::{
    prepare_professional_collections, render_advanced_text_template, render_advanced_xml_template,
    RenderResult, SemanticCase,
};

pub fn render_text_template(template: &str, case: &SemanticCase, strict: bool) -> RenderResult {
    let prepared = prepare_professional_collections(template, case);
    render_advanced_text_template(template, &prepared, strict)
}
pub fn render_docx_xml_template(template: &str, case: &SemanticCase, strict: bool) -> RenderResult {
    let prepared = prepare_professional_collections(template, case);
    render_advanced_xml_template(template, &prepared, strict)
}
pub fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
pub fn render_diary_text_with_signatures(body: &str) -> String {
    let mut out = body.trim().to_string();
    if !has_signature_line(&out, &["лечащий врач", "врач-психиатр", "врач психиатр"])
    {
        out.push_str("\n\nЛечащий врач __________________ /____________/");
    }
    if !has_signature_line(
        &out,
        &["заведующий отделением", "зав. отделением", "зав отделением"],
    ) {
        out.push_str("\nЗаведующий отделением __________ /____________/");
    }
    out
}

fn has_signature_line(text: &str, labels: &[&str]) -> bool {
    text.lines().any(|line| {
        let normalized = line
            .replace('\u{00a0}', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let has_signature_cue = normalized.contains("___")
            || normalized.contains("подпись")
            || normalized.contains("/____")
            || normalized.contains("м.п.");
        has_signature_cue && labels.iter().any(|label| normalized.contains(label))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};
    use std::collections::BTreeMap;

    fn case_with(pairs: &[(&str, &str)]) -> SemanticCase {
        let mut values = BTreeMap::new();
        for (k, v) in pairs {
            values.insert(
                (*k).to_string(),
                SemanticValue {
                    field_id: (*k).to_string(),
                    value: (*v).to_string(),
                    source: ValueSource::UserConfirmed,
                    confidence: 1.0,
                    evidence: Vec::new(),
                },
            );
        }
        SemanticCase {
            values,
            active_domains: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn diary_renderer_adds_two_distinct_signature_lines() {
        let rendered = render_diary_text_with_signatures("Состояние стабильное.");
        assert!(rendered.contains("Лечащий врач __________________"));
        assert!(rendered.contains("Заведующий отделением __________"));
    }

    #[test]
    fn diary_renderer_does_not_treat_narrative_as_a_signature() {
        let rendered = render_diary_text_with_signatures(
            "Лечащий врач осмотрел пациента. Заведующий отделением согласовал лечение.",
        );
        assert!(rendered.contains("Лечащий врач __________________"));
        assert!(rendered.contains("Заведующий отделением __________"));
    }

    #[test]
    fn plain_render_does_not_escape() {
        let case = case_with(&[("org.name", "A & B")]);
        let r = render_text_template("{{org.name}}", &case, true);
        assert_eq!(r.output_text, "A & B");
    }

    #[test]
    fn docx_render_escapes_xml_special_chars() {
        let case = case_with(&[("org.name", "ООО «Иванов & К°» <тест>")]);
        let r = render_docx_xml_template("<w:t>{{org.name}}</w:t>", &case, true);
        assert_eq!(
            r.output_text,
            "<w:t>ООО «Иванов &amp; К°» &lt;тест&gt;</w:t>"
        );
        assert!(r.missing_fields.is_empty());
    }

    #[test]
    fn docx_render_reports_missing_and_keeps_marker_in_strict() {
        let r = render_docx_xml_template("<w:t>{{org.name}}</w:t>", &SemanticCase::default(), true);
        assert_eq!(r.missing_fields, vec!["org.name".to_string()]);
        assert!(r.output_text.contains("{{org.name}}"));
    }

    #[test]
    fn human_and_camel_case_placeholders_use_canonical_case_values() {
        let case = case_with(&[
            ("subject.name", "Иванов Иван"),
            ("medical.case_number", "ИБ-77"),
            ("medical.discharge_date", "12.06.2026"),
        ]);
        let result = render_text_template(
            "{{patientName}} | {{История болезни №}} | {{Дата выписки}}",
            &case,
            true,
        );
        assert_eq!(result.output_text, "Иванов Иван | ИБ-77 | 12.06.2026");
        assert!(result.missing_fields.is_empty());
        assert!(result.unknown_fields.is_empty());
    }

    #[test]
    fn ambiguous_alias_uses_the_only_populated_domain_value() {
        let case = case_with(&[("hr.position", "Инженер")]);
        let result = render_text_template("{{Должность}}", &case, true);
        assert_eq!(result.output_text, "Инженер");
    }
}
