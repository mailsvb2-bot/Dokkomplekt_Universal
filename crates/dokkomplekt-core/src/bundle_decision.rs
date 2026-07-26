//! Exact generation-plan decision for zero-touch automation.
//!
//! The core invariant is intentionally strict: readiness and rendering may only
//! see documents present in the final decision. Ambiguous recommendations never
//! silently fall back to the whole configured pack.

use crate::{DocumentPack, DocumentRoutingRecommendation, KitLearningDecision};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleDecisionSource {
    SpecialistConfirmation,
    PromotedLearningRule,
    DeterministicRoute,
    ReviewProposal,
    /// Ни один шаблон не взял порог рекомендации, но есть близкие кандидаты.
    /// Специалисту предлагается выбор из них, а не пустой экран.
    AmbiguousCandidates,
    NoSafeProposal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleDecision {
    pub document_ids: Vec<String>,
    pub source: BundleDecisionSource,
    pub confidence: f32,
    pub auto_apply: bool,
    pub review_required: bool,
    pub question: Option<String>,
    pub reasons: Vec<String>,
}

impl BundleDecision {
    pub fn is_generation_ready(&self) -> bool {
        self.auto_apply && !self.document_ids.is_empty() && !self.review_required
    }
}

pub fn decide_document_bundle(
    pack: &DocumentPack,
    routing: &DocumentRoutingRecommendation,
    learned: Option<&KitLearningDecision>,
    specialist_confirmed_ids: &[String],
) -> BundleDecision {
    let known = pack
        .documents
        .iter()
        .map(|document| document.id.as_str())
        .collect::<BTreeSet<_>>();
    let labels = pack
        .documents
        .iter()
        .map(|document| (document.id.as_str(), document.button_label.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();

    // Порядок обязан сохранять ранжирование маршрутизатора.
    //
    // До 18.4.0 здесь стоял `collect::<BTreeSet<_>>()`, то есть сортировка
    // по алфавиту. Маршрутизатор аккуратно ранжирует кандидатов по оценке,
    // а сборка комплекта это ранжирование выбрасывала: для источника
    // «Приказ о приёме» специалист видел «Трудовой договор, Приказ о приёме»,
    // потому что `hr.employment_contract` < `hr.employment_order`
    // лексикографически. Первым обязан идти документ, ради которого
    // комплект вообще собран.
    let normalize = |values: &[String]| {
        let mut seen = BTreeSet::new();
        values
            .iter()
            .map(|value| value.trim())
            .filter(|value| known.contains(*value))
            .filter(|value| seen.insert(*value))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    let confirmed = normalize(specialist_confirmed_ids);
    if !confirmed.is_empty() {
        return BundleDecision {
            document_ids: confirmed,
            source: BundleDecisionSource::SpecialistConfirmation,
            confidence: 1.0,
            auto_apply: true,
            review_required: false,
            question: None,
            reasons: vec!["Специалист подтвердил точный состав комплекта.".into()],
        };
    }

    if let Some(rule) = learned.filter(|rule| rule.auto_apply) {
        let selected = normalize(&rule.document_ids);
        if !selected.is_empty() {
            return BundleDecision {
                document_ids: selected,
                source: BundleDecisionSource::PromotedLearningRule,
                confidence: rule.confidence.clamp(0.0, 1.0),
                auto_apply: true,
                review_required: false,
                question: None,
                reasons: vec![rule.reason.clone()],
            };
        }
    }

    let routed = normalize(&routing.recommended_document_ids);
    if routing.auto_select && !routed.is_empty() {
        return BundleDecision {
            document_ids: routed,
            source: BundleDecisionSource::DeterministicRoute,
            confidence: routing.cluster_confidence.clamp(0.0, 1.0),
            auto_apply: true,
            review_required: false,
            question: None,
            reasons: routing.reasons.clone(),
        };
    }

    if !routed.is_empty() {
        let names = routed
            .iter()
            .map(|id| {
                labels
                    .get(id.as_str())
                    .copied()
                    .unwrap_or(id.as_str())
                    .to_string()
            })
            .collect::<Vec<_>>();
        return BundleDecision {
            document_ids: routed,
            source: BundleDecisionSource::ReviewProposal,
            confidence: routing.cluster_confidence.clamp(0.0, 1.0),
            auto_apply: false,
            review_required: true,
            question: Some(format!("Создать комплект: {}?", names.join(", "))),
            reasons: routing.reasons.clone(),
        };
    }

    // Порог рекомендации не взят, но правдоподобные кандидаты есть.
    //
    // До 18.4.0 этот случай проваливался в NoSafeProposal с сообщением
    // «Не удалось безопасно определить комплект. Выберите документы один раз»,
    // хотя ранжированные кандидаты уже были посчитаны и лежали в
    // `routing.matches`. Специалист, уронивший «Акт выполненных работ» —
    // один из самых частых документов в обороте, — получал пустой экран
    // вместо выбора из двух, потому что оценка 0.52 не дотянула до 0.56,
    // а «акт выполненных работ» и «акт приёма-передачи» делили её пополам.
    //
    // Кандидаты именуются, но НЕ становятся планом: auto_apply = false
    // и review_required = true, а вызывающая сторона на этой ветке делает
    // ранний возврат до формирования плана генерации.
    let ambiguous = plausible_candidates(routing, &known);
    if !ambiguous.is_empty() {
        let names = ambiguous
            .iter()
            .map(|id| {
                labels
                    .get(id.as_str())
                    .copied()
                    .unwrap_or(id.as_str())
                    .to_string()
            })
            .collect::<Vec<_>>();
        let mut reasons = routing.reasons.clone();
        reasons.push(format!(
            "Ни один шаблон не набрал уверенности для автозапуска; ближайших кандидатов: {}.",
            ambiguous.len()
        ));
        return BundleDecision {
            document_ids: ambiguous,
            source: BundleDecisionSource::AmbiguousCandidates,
            confidence: routing.cluster_confidence.clamp(0.0, 1.0),
            auto_apply: false,
            review_required: true,
            question: Some(format!(
                "Какой документ создать: {}? Выбор будет запомнен для этого типа дела.",
                names.join(" или ")
            )),
            reasons,
        };
    }

    BundleDecision {
        document_ids: Vec::new(),
        source: BundleDecisionSource::NoSafeProposal,
        confidence: routing.cluster_confidence.clamp(0.0, 1.0),
        auto_apply: false,
        review_required: true,
        question: Some(
            "Не удалось безопасно определить комплект. Выберите документы один раз для этого типа дела."
                .into(),
        ),
        reasons: routing.reasons.clone(),
    }
}

/// Правдоподобные, но не победившие кандидаты.
///
/// Порог 0.32 взят тот же, что уже используется для документов-спутников
/// в `document_routing.rs`, чтобы в коде не появилось второй константы
/// «достаточно похоже». Окно 0.15 от лидера отсекает случайный хвост:
/// смысл здесь — «два-три близких варианта, выбери», а не «вот весь пакет».
fn plausible_candidates(
    routing: &DocumentRoutingRecommendation,
    known: &BTreeSet<&str>,
) -> Vec<String> {
    const CANDIDATE_FLOOR: f32 = 0.32;
    const CANDIDATE_WINDOW: f32 = 0.15;
    const MAX_CANDIDATES: usize = 3;

    let top = routing
        .matches
        .iter()
        .map(|item| item.score)
        .fold(0.0_f32, f32::max);
    if top < CANDIDATE_FLOOR {
        return Vec::new();
    }
    let mut selected = routing
        .matches
        .iter()
        .filter(|item| item.score >= CANDIDATE_FLOOR && top - item.score <= CANDIDATE_WINDOW)
        .filter(|item| known.contains(item.document_id.as_str()))
        .map(|item| item.document_id.clone())
        .collect::<Vec<_>>();
    selected.dedup();
    selected.truncate(MAX_CANDIDATES);
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentMatch, DocumentTemplateSpec, DomainKind};

    fn pack() -> DocumentPack {
        DocumentPack {
            pack_id: "hr".into(),
            name: "HR".into(),
            documents: ["a", "b", "z"]
                .into_iter()
                .map(|id| DocumentTemplateSpec {
                    id: id.into(),
                    button_label: format!("Документ {id}"),
                    template_path: format!("{id}.docx"),
                    category: DomainKind::Hr,
                    role_id: id.into(),
                    required_fields: vec![format!("field.{id}")],
                    placeholders: vec![format!("field.{id}")],
                    is_static_copy: false,
                    popup_fields: Vec::new(),
                    popup_configured: false,
                })
                .collect(),
        }
    }

    fn route(auto_select: bool, ids: &[&str]) -> DocumentRoutingRecommendation {
        DocumentRoutingRecommendation {
            recommended_document_ids: ids.iter().map(|id| id.to_string()).collect(),
            auto_select,
            cluster_confidence: 0.91,
            reasons: vec!["route".into()],
            ..DocumentRoutingRecommendation::default()
        }
    }

    fn scored(pairs: &[(&str, f32)]) -> Vec<DocumentMatch> {
        pairs
            .iter()
            .map(|(id, score)| DocumentMatch {
                document_id: (*id).to_string(),
                button_label: format!("Документ {id}"),
                role_id: (*id).to_string(),
                score: *score,
                evidence: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn near_tied_candidates_are_offered_as_a_choice_not_a_blank_screen() {
        // «Акт выполненных работ»: 0.52 не дотягивает до порога 0.56,
        // потому что близкий шаблон делит оценку. До 18.4.0 специалист
        // получал «не удалось определить» без единого кандидата.
        let mut routing = route(false, &[]);
        routing.matches = scored(&[("a", 0.52), ("b", 0.48), ("z", 0.05)]);
        let decision = decide_document_bundle(&pack(), &routing, None, &[]);
        assert_eq!(decision.source, BundleDecisionSource::AmbiguousCandidates);
        assert_eq!(
            decision.document_ids,
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(decision.review_required);
        assert!(!decision.auto_apply);
        assert!(!decision.is_generation_ready());
        let question = decision.question.expect("вопрос обязателен");
        assert!(question.contains("Документ a"));
        assert!(question.contains("Документ b"));
    }

    #[test]
    fn faint_matches_do_not_become_candidates() {
        // Хвост ниже порога 0.32 — это шум, а не выбор.
        let mut routing = route(false, &[]);
        routing.matches = scored(&[("a", 0.20), ("b", 0.09)]);
        let decision = decide_document_bundle(&pack(), &routing, None, &[]);
        assert_eq!(decision.source, BundleDecisionSource::NoSafeProposal);
        assert!(decision.document_ids.is_empty());
    }

    #[test]
    fn distant_runner_up_is_excluded_from_the_choice() {
        // Окно 0.15: второй кандидат слишком далёк, предлагать его не за что.
        let mut routing = route(false, &[]);
        routing.matches = scored(&[("a", 0.55), ("b", 0.33)]);
        let decision = decide_document_bundle(&pack(), &routing, None, &[]);
        assert_eq!(decision.source, BundleDecisionSource::AmbiguousCandidates);
        assert_eq!(decision.document_ids, vec!["a".to_string()]);
    }

    #[test]
    fn unknown_document_ids_never_leak_into_the_choice() {
        let mut routing = route(false, &[]);
        routing.matches = scored(&[("ghost", 0.60), ("a", 0.55)]);
        let decision = decide_document_bundle(&pack(), &routing, None, &[]);
        assert_eq!(decision.document_ids, vec!["a".to_string()]);
    }

    #[test]
    fn empty_matches_still_reach_the_no_safe_proposal_path() {
        let decision = decide_document_bundle(&pack(), &route(false, &[]), None, &[]);
        assert_eq!(decision.source, BundleDecisionSource::NoSafeProposal);
        assert!(decision.question.is_some());
    }

    #[test]
    fn a_confident_route_is_unaffected_by_the_candidate_path() {
        let mut routing = route(true, &["a"]);
        routing.matches = scored(&[("a", 0.80), ("b", 0.40)]);
        let decision = decide_document_bundle(&pack(), &routing, None, &[]);
        assert_eq!(decision.source, BundleDecisionSource::DeterministicRoute);
        assert!(decision.is_generation_ready());
    }

    #[test]
    fn confident_route_selects_exact_subset() {
        let decision = decide_document_bundle(&pack(), &route(true, &["a", "b"]), None, &[]);
        assert!(decision.is_generation_ready());
        assert_eq!(decision.document_ids, vec!["a", "b"]);
        assert!(!decision.document_ids.contains(&"z".to_string()));
    }

    #[test]
    fn ambiguous_route_never_falls_back_to_entire_pack() {
        let decision = decide_document_bundle(&pack(), &route(false, &["a", "b"]), None, &[]);
        assert!(decision.review_required);
        assert!(!decision.auto_apply);
        assert_eq!(decision.document_ids, vec!["a", "b"]);
        assert!(decision.question.as_deref().unwrap().contains("Документ a"));
    }

    #[test]
    fn specialist_confirmation_has_highest_priority() {
        let learned = KitLearningDecision {
            document_ids: vec!["a".into()],
            source: "learned".into(),
            confidence: 1.0,
            auto_apply: true,
            reason: "learned".into(),
        };
        let decision = decide_document_bundle(
            &pack(),
            &route(true, &["a", "b"]),
            Some(&learned),
            &["b".into()],
        );
        assert_eq!(
            decision.source,
            BundleDecisionSource::SpecialistConfirmation
        );
        assert_eq!(decision.document_ids, vec!["b"]);
    }

    #[test]
    fn unknown_document_ids_are_never_selected() {
        let decision = decide_document_bundle(&pack(), &route(true, &["a", "missing"]), None, &[]);
        assert_eq!(decision.document_ids, vec!["a"]);
    }
}
