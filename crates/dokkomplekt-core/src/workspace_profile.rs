use crate::{DocumentPack, DomainKind, TemplateAnalysis};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceInferenceLevel {
    High,
    Medium,
    #[default]
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceProfileEvidence {
    pub document_id: String,
    pub title: String,
    pub role_id: String,
    pub attributed_domain: DomainKind,
    pub score: usize,
    pub field_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceProfileInference {
    pub suggested_domain: Option<DomainKind>,
    pub confidence: f32,
    pub level: WorkspaceInferenceLevel,
    pub auto_apply: bool,
    pub mixed_domains: bool,
    pub domain_scores: BTreeMap<String, usize>,
    pub evidence: Vec<WorkspaceProfileEvidence>,
    pub reasons: Vec<String>,
}

impl Default for WorkspaceProfileInference {
    fn default() -> Self {
        Self {
            suggested_domain: None,
            confidence: 0.0,
            level: WorkspaceInferenceLevel::Low,
            auto_apply: false,
            mixed_domains: false,
            domain_scores: BTreeMap::new(),
            evidence: Vec::new(),
            reasons: vec!["Недостаточно согласованных профессиональных признаков.".into()],
        }
    }
}

const SPECIFIC_DOMAINS: [(&str, DomainKind); 5] = [
    ("medical", DomainKind::Medical),
    ("legal", DomainKind::Legal),
    ("hr", DomainKind::Hr),
    ("accounting", DomainKind::Accounting),
    ("education", DomainKind::Education),
];

/// Infer one workspace profile from the complete set of templates selected by the
/// specialist. This function deliberately does not own a profession vocabulary:
/// it aggregates the canonical per-template `domain_scores` produced by Template
/// Intelligence. That keeps one source of truth for terminology and avoids a
/// parallel classifier/"second brain".
pub fn infer_workspace_profile(
    analyses: &[(String, TemplateAnalysis)],
) -> WorkspaceProfileInference {
    if analyses.is_empty() {
        return WorkspaceProfileInference::default();
    }

    let mut totals = BTreeMap::<String, usize>::new();
    for (key, _) in SPECIFIC_DOMAINS {
        totals.insert(key.to_string(), 0);
    }
    for (_, analysis) in analyses {
        for (key, _) in SPECIFIC_DOMAINS {
            *totals.entry(key.to_string()).or_default() +=
                analysis.domain_scores.get(key).copied().unwrap_or_default();
        }
    }

    let mut ranked = totals
        .iter()
        .map(|(domain, score)| (domain.as_str(), *score))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    let Some((top_key, top_score)) = ranked.first().copied() else {
        return WorkspaceProfileInference::default();
    };
    let runner_score = ranked.get(1).map(|(_, score)| *score).unwrap_or_default();
    if top_score == 0 {
        return WorkspaceProfileInference {
            domain_scores: totals,
            ..WorkspaceProfileInference::default()
        };
    }

    let Some(top_domain) = domain_for_key(top_key) else {
        return WorkspaceProfileInference {
            domain_scores: totals,
            ..WorkspaceProfileInference::default()
        };
    };
    let mut support_count = 0usize;
    let mut contradictory_count = 0usize;
    let mut evidence = Vec::new();

    for (document_id, analysis) in analyses {
        let mut per_document = SPECIFIC_DOMAINS
            .iter()
            .map(|(key, _)| {
                (
                    *key,
                    analysis
                        .domain_scores
                        .get(*key)
                        .copied()
                        .unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        per_document.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        let (document_key, document_score) = per_document[0];
        let next_score = per_document
            .get(1)
            .map(|(_, score)| *score)
            .unwrap_or_default();
        let has_clear_signal = document_score >= 2 && document_score > next_score;

        if has_clear_signal && document_key == top_key {
            support_count += 1;
            evidence.push(WorkspaceProfileEvidence {
                document_id: document_id.clone(),
                title: analysis.title.clone(),
                role_id: analysis.role_id.clone(),
                attributed_domain: top_domain.clone(),
                score: document_score,
                field_ids: analysis.placeholders.iter().take(8).cloned().collect(),
            });
        } else if has_clear_signal && document_key != top_key {
            contradictory_count += 1;
        }
    }

    evidence.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    evidence.truncate(6);

    let total_specific = totals.values().copied().sum::<usize>().max(1);
    let dominance = top_score as f32 / total_specific as f32;
    let margin = top_score.saturating_sub(runner_score) as f32 / top_score.max(1) as f32;
    let coverage = support_count as f32 / analyses.len().max(1) as f32;
    let expected_strength = analyses.len().max(1) * 5;
    let strength = (top_score as f32 / expected_strength as f32).min(1.0);
    let confidence =
        (dominance * 0.32 + margin * 0.28 + coverage * 0.30 + strength * 0.10).clamp(0.0, 0.99);

    // A single template with a clear, conflicting signal is enough to prove
    // that this is not one homogeneous workspace. Aggregate dominance must not
    // drag that document into the majority domain during automatic setup.
    let mixed_domains = contradictory_count > 0;
    let minimum_high_score = if analyses.len() == 1 {
        5
    } else {
        (analyses.len() * 2).max(6)
    };
    let minimum_support = analyses.len().div_ceil(2).max(1);

    let level = if !mixed_domains
        && top_score >= minimum_high_score
        && support_count >= minimum_support
        && confidence >= 0.70
    {
        WorkspaceInferenceLevel::High
    } else if !mixed_domains && top_score >= 4 && support_count >= 1 && confidence >= 0.52 {
        WorkspaceInferenceLevel::Medium
    } else {
        WorkspaceInferenceLevel::Low
    };
    let suggested_domain = match level {
        WorkspaceInferenceLevel::High | WorkspaceInferenceLevel::Medium => Some(top_domain.clone()),
        WorkspaceInferenceLevel::Low => None,
    };
    let auto_apply = level == WorkspaceInferenceLevel::High;

    let mut reasons = Vec::new();
    if mixed_domains {
        reasons.push(format!(
            "Набор содержит уверенные признаки нескольких областей: {top_key}={top_score}, следующий профиль={runner_score}. Автовыбор отключён."
        ));
    } else {
        reasons.push(format!(
            "{support_count} из {} шаблонов независимо поддерживают один рабочий профиль.",
            analyses.len()
        ));
        reasons.push(format!(
            "Суммарный вес профиля {top_key}: {top_score}; ближайшего альтернативного: {runner_score}."
        ));
    }
    if level == WorkspaceInferenceLevel::Low {
        reasons.push(
            "Слабые или неоднозначные признаки не используются для автоматического выбора профессии."
                .into(),
        );
    }

    WorkspaceProfileInference {
        suggested_domain,
        confidence,
        level,
        auto_apply,
        mixed_domains,
        domain_scores: totals,
        evidence,
        reasons,
    }
}

/// Returns a reusable workspace domain only when the persisted pack has exactly
/// one non-generic structural profile. Generic documents are neutral; a mixed
/// pack never becomes a global hint.
pub fn stable_workspace_domain_from_pack(pack: &DocumentPack) -> Option<DomainKind> {
    let domains = pack
        .documents
        .iter()
        .filter_map(|document| normalized_reusable_domain(&document.category))
        .collect::<std::collections::BTreeSet<_>>();
    (domains.len() == 1)
        .then(|| domains.into_iter().next())
        .flatten()
}

/// Reuse confirmed/persisted workspace structure only as a conservative context
/// for weak new-template evidence. A clear conflicting signal is deliberately
/// preserved so a second professional contour can be created in the same pack.
pub fn reinforce_workspace_inference_with_pack(
    mut inference: WorkspaceProfileInference,
    pack: &DocumentPack,
) -> WorkspaceProfileInference {
    let Some(existing_domain) = stable_workspace_domain_from_pack(pack) else {
        return inference;
    };
    if inference.mixed_domains {
        return inference;
    }
    if inference.auto_apply {
        return inference;
    }
    if let Some(suggested) = inference.suggested_domain.as_ref() {
        if suggested != &existing_domain {
            return inference;
        }
    }
    if has_clear_conflict_with_existing_domain(&inference, &existing_domain) {
        return inference;
    }

    inference.suggested_domain = Some(existing_domain.clone());
    inference.level = WorkspaceInferenceLevel::High;
    inference.auto_apply = true;
    inference.confidence = inference.confidence.max(0.82);
    inference.reasons.insert(
        0,
        format!(
            "Неоднозначные новые шаблоны используют уже подтверждённый профиль рабочего комплекта: {}. Явно противоречащий документ всегда остаётся отдельным контуром.",
            domain_key_for_display(&existing_domain)
        ),
    );
    inference
}

fn normalized_reusable_domain(domain: &DomainKind) -> Option<DomainKind> {
    match domain {
        DomainKind::Generic => None,
        DomainKind::Custom(value) if value.trim().is_empty() => None,
        DomainKind::Custom(value) => Some(DomainKind::Custom(value.trim().to_string())),
        value => Some(value.clone()),
    }
}

fn has_clear_conflict_with_existing_domain(
    inference: &WorkspaceProfileInference,
    existing_domain: &DomainKind,
) -> bool {
    let mut ranked = inference
        .domain_scores
        .iter()
        .filter(|(key, _)| {
            SPECIFIC_DOMAINS
                .iter()
                .any(|(candidate, _)| candidate == &key.as_str())
        })
        .map(|(key, score)| (key.as_str(), *score))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    let Some((top_key, top_score)) = ranked.first().copied() else {
        return false;
    };
    let runner_score = ranked.get(1).map(|(_, score)| *score).unwrap_or_default();
    if top_score < 2 || top_score <= runner_score {
        return false;
    }
    domain_for_key(top_key).as_ref() != Some(existing_domain)
}

fn domain_key_for_display(domain: &DomainKind) -> &'static str {
    match domain {
        DomainKind::Medical => "медицина",
        DomainKind::Legal => "юридическая работа",
        DomainKind::Hr => "кадровая работа",
        DomainKind::Accounting => "бухгалтерия",
        DomainKind::Education => "образование",
        DomainKind::Custom(_) => "пользовательский профиль",
        DomainKind::Generic => "универсальный документооборот",
    }
}

fn domain_for_key(key: &str) -> Option<DomainKind> {
    SPECIFIC_DOMAINS
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, domain)| domain.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze_template_text;

    fn item(id: &str, text: &str) -> (String, TemplateAnalysis) {
        (id.into(), analyze_template_text(text))
    }

    #[test]
    fn coherent_medical_workspace_is_high_confidence_and_auto_applied() {
        let inference = infer_workspace_profile(&[
            item(
                "primary",
                "Первичный осмотр\nДиагноз: {{medical.diagnosis}}\nМКБ-10: {{medical.icd10}}\nЛечение: {{medical.treatment}}",
            ),
            item(
                "discharge",
                "Выписной эпикриз\nИстория болезни № {{medical.case_number}}\nДата выписки {{medical.discharge_date}}",
            ),
            item(
                "diaries",
                "Дневники наблюдения\nДиагноз\nЛечение\nЛечащий врач",
            ),
            item("consent", "Согласие пациента\n{{subject.name}}"),
        ]);

        assert_eq!(inference.suggested_domain, Some(DomainKind::Medical));
        assert_eq!(inference.level, WorkspaceInferenceLevel::High);
        assert!(inference.auto_apply);
        assert!(!inference.mixed_domains);
        assert!(inference.confidence >= 0.70);
    }

    #[test]
    fn one_ambiguous_word_does_not_choose_a_profession() {
        let inference = infer_workspace_profile(&[item("act", "Акт\nНомер: {{document.number}}")]);

        assert_eq!(inference.suggested_domain, None);
        assert_eq!(inference.level, WorkspaceInferenceLevel::Low);
        assert!(!inference.auto_apply);
    }

    #[test]
    fn mixed_legal_and_hr_workspace_never_auto_applies_one_side() {
        let inference = infer_workspace_profile(&[
            item("claim", "Исковое заявление\nИстец\nОтветчик\nСуд\nДело"),
            item("contract", "Договор\nСторона\nЗаказчик\nИсполнитель"),
            item(
                "hire",
                "Приказ о приёме сотрудника\nДолжность\nОтдел\nКадровая служба",
            ),
            item(
                "dismiss",
                "Приказ об увольнении сотрудника\nДолжность\nОтдел\nКадровая служба",
            ),
        ]);

        assert!(!inference.auto_apply);
        assert!(inference.mixed_domains || inference.level != WorkspaceInferenceLevel::High);
    }

    #[test]
    fn strong_minority_domain_is_never_overridden_by_a_large_majority() {
        let inference = infer_workspace_profile(&[
            item(
                "primary",
                "Первичный осмотр\nДиагноз\nЛечение\nАнамнез\nИстория болезни\nМКБ-10",
            ),
            item(
                "discharge",
                "Выписной эпикриз\nДиагноз\nЛечение\nДата выписки\nИстория болезни",
            ),
            item(
                "diary",
                "Дневник наблюдения\nДиагноз\nЛечение\nЛечащий врач",
            ),
            item(
                "consultation",
                "Медицинская консультация\nПациент\nДиагноз\nЛечение\nМКБ-10",
            ),
            item(
                "claim",
                "Исковое заявление\nИстец\nОтветчик\nСуд\nДело\nГоспошлина",
            ),
        ]);

        assert!(inference.mixed_domains);
        assert!(!inference.auto_apply);
        assert_eq!(inference.suggested_domain, None);
        assert_eq!(inference.level, WorkspaceInferenceLevel::Low);
    }

    #[test]
    fn strong_single_template_can_be_recognized_without_profession_question() {
        let inference = infer_workspace_profile(&[item(
            "medical",
            "Выписной эпикриз\nДиагноз\nЛечение\nАнамнез\nИстория болезни\nМКБ-10\nДневник",
        )]);

        assert_eq!(inference.suggested_domain, Some(DomainKind::Medical));
        assert_eq!(inference.level, WorkspaceInferenceLevel::High);
        assert!(inference.auto_apply);
    }

    fn pack_with_domains(domains: &[DomainKind]) -> DocumentPack {
        DocumentPack {
            pack_id: "workspace".into(),
            name: "workspace".into(),
            documents: domains
                .iter()
                .enumerate()
                .map(|(index, domain)| crate::DocumentTemplateSpec {
                    id: format!("existing-{index}"),
                    button_label: format!("Existing {index}"),
                    template_path: format!("existing-{index}.docx"),
                    category: domain.clone(),
                    role_id: "unknown".into(),
                    required_fields: Vec::new(),
                    placeholders: Vec::new(),
                    is_static_copy: false,
                    popup_fields: Vec::new(),
                    popup_configured: false,
                })
                .collect(),
        }
    }

    #[test]
    fn ambiguous_new_template_reuses_one_persisted_workspace_domain() {
        let inferred = infer_workspace_profile(&[item("act", "Акт\nНомер {{document.number}}")]);
        assert_eq!(inferred.level, WorkspaceInferenceLevel::Low);
        let reinforced = reinforce_workspace_inference_with_pack(
            inferred,
            &pack_with_domains(&[DomainKind::Legal, DomainKind::Generic]),
        );
        assert_eq!(reinforced.suggested_domain, Some(DomainKind::Legal));
        assert_eq!(reinforced.level, WorkspaceInferenceLevel::High);
        assert!(reinforced.auto_apply);
        assert!(reinforced.reasons[0].contains("подтверждённый профиль"));
    }

    #[test]
    fn mixed_persisted_workspace_never_becomes_global_context() {
        let inferred = infer_workspace_profile(&[item("act", "Акт\nНомер {{document.number}}")]);
        let reinforced = reinforce_workspace_inference_with_pack(
            inferred.clone(),
            &pack_with_domains(&[DomainKind::Legal, DomainKind::Hr]),
        );
        assert_eq!(reinforced, inferred);
    }

    #[test]
    fn clear_new_conflict_is_never_dragged_into_old_workspace_domain() {
        let inferred = WorkspaceProfileInference {
            suggested_domain: None,
            confidence: 0.4,
            level: WorkspaceInferenceLevel::Low,
            auto_apply: false,
            mixed_domains: false,
            domain_scores: BTreeMap::from([
                ("legal".into(), 3),
                ("hr".into(), 0),
                ("medical".into(), 0),
                ("accounting".into(), 0),
                ("education".into(), 0),
            ]),
            evidence: Vec::new(),
            reasons: Vec::new(),
        };
        let reinforced = reinforce_workspace_inference_with_pack(
            inferred.clone(),
            &pack_with_domains(&[DomainKind::Hr]),
        );
        assert_eq!(reinforced, inferred);
    }
}
