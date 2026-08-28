use crate::{
    analyze_template_structure_v2, extract_semantic, title_for_field, TemplateStructureAnalysisV2,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateMarkupCandidate {
    pub field_id: String,
    pub title: String,
    pub value: String,
    pub confidence: f32,
    pub occurrences: usize,
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateLearningInput {
    pub blank_template_text: String,
    pub completed_examples: Vec<String>,
    #[serde(default)]
    pub source_examples: Vec<String>,
    pub default_year: i32,
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_locale() -> String {
    "ru-RU".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnedTemplateField {
    pub field_id: String,
    pub title: String,
    pub line_index: usize,
    pub label_prefix: String,
    pub blank_line: String,
    pub common_prefix: String,
    pub common_suffix: String,
    pub example_values: Vec<String>,
    pub source_matches: Vec<String>,
    pub placeholder: String,
    pub confidence: f32,
    pub required: bool,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateDiffHunk {
    pub line_index: usize,
    pub blank_line: String,
    pub example_lines: Vec<String>,
    pub common_prefix: String,
    pub common_suffix: String,
    pub variable_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateLearningReport {
    pub locale: String,
    pub fields: Vec<LearnedTemplateField>,
    pub immutable_lines: Vec<usize>,
    pub conditional_lines: Vec<usize>,
    pub repeated_line_groups: Vec<Vec<usize>>,
    pub structure: TemplateStructureAnalysisV2,
    pub diff: Vec<TemplateDiffHunk>,
    pub confidence: f32,
    pub requires_confirmation: bool,
    pub warnings: Vec<String>,
}

pub fn suggest_template_markup(text: &str, default_year: i32) -> Vec<TemplateMarkupCandidate> {
    let (case, report) = extract_semantic(text, default_year);
    let mut out = Vec::new();
    for f in report.fields {
        if f.value.trim().len() < 2 {
            continue;
        }
        let occurrences = text.matches(&f.value).count();
        if occurrences == 0 {
            continue;
        }
        let pinned = case.get(&f.field_id).unwrap_or(&f.value);
        out.push(TemplateMarkupCandidate {
            field_id: f.field_id.clone(),
            title: title_for_field(&f.field_id),
            value: pinned.to_string(),
            confidence: f.confidence,
            occurrences,
            selected_by_default: f.confidence >= 0.85 && occurrences <= 5,
        });
    }
    out.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.field_id.cmp(&b.field_id))
    });
    out
}

/// Suggest patient/case fields that can be safely removed from a filled medical
/// working document before it becomes a reusable template. Unlike the generic
/// scanner above, this uses the Medical source parser so ambiguous labels such as
/// `Должность` resolve to the medical field rather than an HR field.
///
/// Only values that are present verbatim in the extracted document and have
/// strong parser evidence are selected automatically. Repeated identical values
/// owned by different semantic fields are left for manual review. Signer fields
/// are intentionally excluded because a doctor's name can legitimately be fixed
/// template content.
pub fn suggest_filled_medical_template_markup(
    text: &str,
    default_year: i32,
) -> Vec<TemplateMarkupCandidate> {
    const AUTO_FIELDS: &[&str] = &[
        "subject.name",
        "subject.birth_date",
        "subject.age",
        "subject.address",
        "medical.case_number",
        "medical.admission_date",
        "medical.discharge_date",
        "medical.complaints",
        "medical.anamnesis_life",
        "medical.anamnesis_disease",
        "medical.epidemiology",
        "medical.profile_observation",
        "medical.disability",
        "medical.rvk_referral",
        "medical.profile_status",
        "medical.somatic_status",
        "medical.examination_plan",
        "medical.diagnosis",
        "medical.icd10",
        "medical.treatment",
        "medical.treatment_result",
        "medical.discharge_condition",
        "medical.recommendations",
        "medical.labs",
        "medical.labs_date",
        "medical.workplace",
        "medical.position",
        "medical.sick_leave_number",
    ];

    let (case, _) = crate::parse_source_text(text, default_year);
    let mut out = AUTO_FIELDS
        .iter()
        .filter_map(|field_id| {
            let semantic = case.value(field_id)?;
            let value = semantic.value.trim();
            if value.len() < 2 {
                return None;
            }
            let occurrences = text.matches(value).count();
            if occurrences == 0 {
                return None;
            }
            Some(TemplateMarkupCandidate {
                field_id: (*field_id).to_string(),
                title: title_for_field(field_id),
                value: value.to_string(),
                confidence: semantic.confidence,
                occurrences,
                selected_by_default: semantic.confidence >= 0.80 && occurrences <= 5,
            })
        })
        .collect::<Vec<_>>();

    let mut owners = BTreeMap::<String, BTreeSet<String>>::new();
    for candidate in &out {
        owners
            .entry(candidate.value.clone())
            .or_default()
            .insert(candidate.field_id.clone());
    }
    let nested_values = out
        .iter()
        .map(|candidate| {
            let nested = out.iter().any(|other| {
                candidate.field_id != other.field_id
                    && candidate.value.len() < other.value.len()
                    && other.value.contains(&candidate.value)
                    && candidate.occurrences <= other.occurrences
            });
            (candidate.field_id.clone(), nested)
        })
        .collect::<BTreeMap<_, _>>();
    for candidate in &mut out {
        if owners
            .get(&candidate.value)
            .is_some_and(|field_ids| field_ids.len() > 1)
            || nested_values.get(&candidate.field_id).copied() == Some(true)
        {
            candidate.selected_by_default = false;
        }
    }
    out.sort_by(|left, right| left.field_id.cmp(&right.field_id));
    out
}

/// Infer a field map by comparing a blank template with several previously
/// completed documents. The result is always reviewable: no inferred field is
/// silently written into the user's DOCX before explicit confirmation.
pub fn learn_template_from_examples(input: &TemplateLearningInput) -> TemplateLearningReport {
    let completed = input
        .completed_examples
        .iter()
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>();
    let structure = analyze_template_structure_v2(&input.blank_template_text);
    let mut warnings = Vec::new();
    if completed.len() < 3 {
        warnings.push(
            "Для устойчивого обучения рекомендуется не менее трёх ранее заполненных примеров."
                .into(),
        );
    }
    if completed.len() > 10 {
        warnings.push(
            "Использованы первые 10 примеров; остальные нужно обработать отдельной серией.".into(),
        );
    }
    let completed = completed.into_iter().take(10).collect::<Vec<_>>();
    if completed.is_empty() {
        warnings.push("Не передано ни одного заполненного примера.".into());
        return TemplateLearningReport {
            locale: input.locale.clone(),
            fields: Vec::new(),
            immutable_lines: Vec::new(),
            conditional_lines: Vec::new(),
            repeated_line_groups: Vec::new(),
            structure,
            diff: Vec::new(),
            confidence: 0.0,
            requires_confirmation: true,
            warnings,
        };
    }

    let blank_lines = normalized_lines(&input.blank_template_text);
    let example_lines = completed
        .iter()
        .map(|text| normalized_lines(text))
        .collect::<Vec<_>>();
    let source_maps = input
        .source_examples
        .iter()
        .map(|text| semantic_value_map(text, input.default_year))
        .collect::<Vec<_>>();
    let completed_maps = completed
        .iter()
        .map(|text| semantic_value_map(text, input.default_year))
        .collect::<Vec<_>>();

    let max_lines = example_lines
        .iter()
        .map(Vec::len)
        .chain(std::iter::once(blank_lines.len()))
        .max()
        .unwrap_or_default();
    let mut immutable_lines = Vec::new();
    let mut conditional_lines = Vec::new();
    let mut diff = Vec::new();
    let mut fields = Vec::new();
    let mut used_field_ids = BTreeSet::new();

    for line_index in 0..max_lines {
        let blank = blank_lines.get(line_index).cloned().unwrap_or_default();
        let lines = example_lines
            .iter()
            .map(|example| example.get(line_index).cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        let non_empty = lines.iter().filter(|line| !line.trim().is_empty()).count();
        if non_empty == 0 {
            continue;
        }
        if lines.iter().all(|line| line == &lines[0]) && (blank.is_empty() || blank == lines[0]) {
            immutable_lines.push(line_index);
            continue;
        }
        if non_empty < lines.len() {
            conditional_lines.push(line_index);
        }
        let common_prefix = longest_common_prefix(&lines);
        let common_suffix = longest_common_suffix(&lines, common_prefix.chars().count());
        let values = lines
            .iter()
            .map(|line| variable_between(line, &common_prefix, &common_suffix))
            .filter(|value| !value.trim().is_empty())
            .collect::<Vec<_>>();
        let unique_values = values.iter().cloned().collect::<BTreeSet<_>>();
        if unique_values.len() < 2 {
            continue;
        }
        diff.push(TemplateDiffHunk {
            line_index,
            blank_line: blank.clone(),
            example_lines: lines.clone(),
            common_prefix: common_prefix.clone(),
            common_suffix: common_suffix.clone(),
            variable_values: values.clone(),
        });

        let field_scores = score_field_candidates(&values, &completed_maps, &source_maps);
        let Some((field_id, score, source_matches)) = field_scores.first().cloned() else {
            warnings.push(format!(
                "Строка {} изменяется между примерами, но источник значения не определён: {}",
                line_index + 1,
                values.join(" | ")
            ));
            continue;
        };
        if used_field_ids.contains(&field_id) && values.len() == 1 {
            continue;
        }
        used_field_ids.insert(field_id.clone());
        let presence = non_empty as f32 / lines.len().max(1) as f32;
        let confidence = ((score * 0.72) + (presence * 0.18) + 0.10).clamp(0.0, 0.99);
        fields.push(LearnedTemplateField {
            title: title_for_field(&field_id),
            placeholder: format!("{{{{{field_id}}}}}"),
            field_id,
            line_index,
            label_prefix: choose_label_prefix(&blank, &common_prefix),
            blank_line: blank.clone(),
            common_prefix: common_prefix.clone(),
            common_suffix: common_suffix.clone(),
            example_values: values,
            source_matches,
            confidence,
            required: presence >= 0.999,
            condition: (presence < 0.999).then(|| {
                format!(
                    "Строка присутствует в {non_empty} из {} примеров; подтвердите условие появления.",
                    lines.len()
                )
            }),
        });
    }

    fields.sort_by(|left, right| {
        left.line_index
            .cmp(&right.line_index)
            .then_with(|| left.field_id.cmp(&right.field_id))
    });
    let repeated_line_groups = repeated_lines(&blank_lines);
    let confidence = if fields.is_empty() {
        0.0
    } else {
        fields.iter().map(|field| field.confidence).sum::<f32>() / fields.len() as f32
    };
    if fields.is_empty() {
        warnings.push(
            "Автоматическая карта не построена. Используйте визуальный diff и отметьте поля вручную."
                .into(),
        );
    }

    TemplateLearningReport {
        locale: input.locale.clone(),
        fields,
        immutable_lines,
        conditional_lines,
        repeated_line_groups,
        structure,
        diff,
        confidence,
        requires_confirmation: true,
        warnings,
    }
}

fn normalized_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn semantic_value_map(text: &str, default_year: i32) -> BTreeMap<String, String> {
    let (case, _) = extract_semantic(text, default_year);
    case.values
        .into_iter()
        .filter_map(|(field_id, value)| {
            let value = value.value.trim().to_string();
            (!value.is_empty()).then_some((field_id, value))
        })
        .collect()
}

fn score_field_candidates(
    values: &[String],
    completed_maps: &[BTreeMap<String, String>],
    source_maps: &[BTreeMap<String, String>],
) -> Vec<(String, f32, Vec<String>)> {
    let mut candidates = BTreeMap::<String, (usize, Vec<String>)>::new();
    for (index, value) in values.iter().enumerate() {
        let normalized = normalize_value(value);
        for map in completed_maps
            .get(index)
            .into_iter()
            .chain(source_maps.get(index))
        {
            for (field_id, candidate_value) in map {
                if values_equivalent(&normalized, candidate_value) {
                    let entry = candidates.entry(field_id.clone()).or_default();
                    entry.0 += 1;
                    if !entry.1.contains(candidate_value) {
                        entry.1.push(candidate_value.clone());
                    }
                }
            }
        }
    }
    let denominator = values.len().max(1) as f32;
    let mut ranked = candidates
        .into_iter()
        .map(|(field_id, (matches, sources))| (field_id, matches as f32 / denominator, sources))
        .filter(|(_, score, _)| *score >= 0.50)
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked
}

fn values_equivalent(normalized_value: &str, candidate: &str) -> bool {
    let candidate = normalize_value(candidate);
    !normalized_value.is_empty()
        && (candidate == normalized_value
            || candidate.contains(normalized_value)
            || normalized_value.contains(&candidate))
}

fn normalize_value(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn longest_common_prefix(lines: &[String]) -> String {
    let Some(first) = lines.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for line in lines.iter().skip(1) {
        let count = prefix
            .chars()
            .zip(line.chars())
            .take_while(|(left, right)| left == right)
            .count();
        prefix = prefix.chars().take(count).collect();
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

fn longest_common_suffix(lines: &[String], prefix_chars: usize) -> String {
    let Some(first) = lines.first() else {
        return String::new();
    };
    let mut suffix = first.clone();
    for line in lines.iter().skip(1) {
        let count = suffix
            .chars()
            .rev()
            .zip(line.chars().rev())
            .take_while(|(left, right)| left == right)
            .count();
        suffix = suffix
            .chars()
            .rev()
            .take(count)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if suffix.is_empty() {
            break;
        }
    }
    let first_len = first.chars().count();
    if prefix_chars + suffix.chars().count() > first_len {
        String::new()
    } else {
        suffix
    }
}

fn variable_between(line: &str, prefix: &str, suffix: &str) -> String {
    let without_prefix = line.strip_prefix(prefix).unwrap_or(line);
    without_prefix
        .strip_suffix(suffix)
        .unwrap_or(without_prefix)
        .trim()
        .to_string()
}

fn choose_label_prefix(blank: &str, common_prefix: &str) -> String {
    let blank = blank.trim();
    if !blank.is_empty() {
        blank
            .trim_end_matches(['_', '.', ':', ' '])
            .trim()
            .to_string()
    } else {
        common_prefix
            .trim_end_matches(['_', '.', ':', ' '])
            .trim()
            .to_string()
    }
}

fn repeated_lines(lines: &[String]) -> Vec<Vec<usize>> {
    let mut positions = BTreeMap::<String, Vec<usize>>::new();
    for (index, line) in lines.iter().enumerate() {
        let normalized = line.trim().to_lowercase();
        if normalized.len() >= 4 {
            positions.entry(normalized).or_default().push(index);
        }
    }
    positions
        .into_values()
        .filter(|indexes| indexes.len() > 1)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounded_only() {
        let text = "ИНН: 7736050003";
        let candidates = suggest_template_markup(text, 2026);
        assert!(candidates
            .iter()
            .all(|candidate| text.contains(&candidate.value)));
    }

    #[test]
    fn filled_medical_discharge_suggests_basic_patient_and_case_fields() {
        let text = "Выписной эпикриз\nФ.И.О.: Иванов Иван Иванович\nНомер истории болезни: 1234\nДата поступления: 01.09.2026\nДиагноз: F20.0 состояние стабильное\nДата выписки: 09.09.2026\nЛечение: терапия\nМесто работы: Завод\nДолжность: инженер";
        let candidates = suggest_filled_medical_template_markup(text, 2026);
        let selected = candidates
            .iter()
            .filter(|candidate| candidate.selected_by_default)
            .map(|candidate| candidate.field_id.as_str())
            .collect::<BTreeSet<_>>();
        for field_id in [
            "subject.name",
            "medical.case_number",
            "medical.admission_date",
            "medical.discharge_date",
            "medical.diagnosis",
            "medical.treatment",
            "medical.workplace",
            "medical.position",
        ] {
            assert!(
                selected.contains(field_id),
                "missing safe markup candidate: {field_id}; candidates={candidates:?}"
            );
        }
        assert!(!candidates
            .iter()
            .any(|candidate| candidate.field_id == "employee.position"));
        let icd = candidates
            .iter()
            .find(|candidate| candidate.field_id == "medical.icd10")
            .expect("ICD is recognized independently");
        assert!(
            !icd.selected_by_default,
            "nested ICD must not compete with the diagnosis replacement"
        );
    }

    #[test]
    fn filled_medical_markup_does_not_auto_select_one_value_for_two_fields() {
        let text = "Выписной эпикриз\nФ.И.О.: Иванов Иван Иванович\nДата поступления: 09.09.2026\nДата выписки: 09.09.2026\nДиагноз: F20.0";
        let candidates = suggest_filled_medical_template_markup(text, 2026);
        for field_id in ["medical.admission_date", "medical.discharge_date"] {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.field_id == field_id)
                .expect(field_id);
            assert!(!candidate.selected_by_default);
        }
    }

    #[test]
    fn learns_variable_field_from_multiple_completed_examples() {
        let report = learn_template_from_examples(&TemplateLearningInput {
            blank_template_text: "Приказ\nСотрудник: __________\nДолжность: __________".into(),
            completed_examples: vec![
                "Приказ\nСотрудник: Иванов Иван Иванович\nДолжность: Врач".into(),
                "Приказ\nСотрудник: Петров Пётр Петрович\nДолжность: Юрист".into(),
                "Приказ\nСотрудник: Сидоров Сергей Сергеевич\nДолжность: Бухгалтер".into(),
            ],
            source_examples: Vec::new(),
            default_year: 2026,
            locale: "ru-RU".into(),
        });
        assert!(report.requires_confirmation);
        assert!(report.diff.len() >= 2);
        assert!(report
            .fields
            .iter()
            .any(|field| { matches!(field.field_id.as_str(), "employee.name" | "subject.name") }));
    }

    #[test]
    fn conditional_line_is_never_silently_made_required() {
        let report = learn_template_from_examples(&TemplateLearningInput {
            blank_template_text: "Справка\nБольничный: __________".into(),
            completed_examples: vec![
                "Справка\nБольничный: 123".into(),
                "Справка\n".into(),
                "Справка\nБольничный: 456".into(),
            ],
            source_examples: Vec::new(),
            default_year: 2026,
            locale: "ru-RU".into(),
        });
        assert!(report.conditional_lines.contains(&1));
        assert!(report
            .fields
            .iter()
            .all(|field| !field.required || field.line_index != 1));
    }
}
