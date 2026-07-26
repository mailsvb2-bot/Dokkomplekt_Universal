//! Conservative local classifier for choosing an input domain without asking the specialist.
//!
//! The classifier never selects a domain from one weak word. It combines phrase
//! evidence and already validated semantic field prefixes. Ambiguous inputs stay
//! `Generic`, so classification cannot silently route a case into the wrong pack.

use crate::{DomainKind, SemanticCase};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceDomainPrediction {
    pub domain: DomainKind,
    pub confidence: f32,
    pub score: u32,
    pub runner_up_score: u32,
    pub evidence: Vec<String>,
}

impl Default for SourceDomainPrediction {
    fn default() -> Self {
        Self {
            domain: DomainKind::Generic,
            confidence: 0.0,
            score: 0,
            runner_up_score: 0,
            evidence: Vec::new(),
        }
    }
}

pub fn classify_source_domain(text: &str, case: &SemanticCase) -> SourceDomainPrediction {
    let normalized = normalize(text);
    let mut scores = BTreeMap::<DomainKind, u32>::new();
    let mut evidence = BTreeMap::<DomainKind, Vec<String>>::new();
    for (domain, phrases) in phrase_sets() {
        for (phrase, weight) in phrases {
            if normalized.contains(phrase) {
                *scores.entry(domain.clone()).or_default() += weight;
                evidence
                    .entry(domain.clone())
                    .or_default()
                    .push(phrase.to_string());
            }
        }
    }
    // Доказательство из полей ОБЯЗАНО насыщаться по пространству имён.
    //
    // До 18.4.0 каждое поле с доменным префиксом добавляло +4 независимо,
    // поэтому уверенность росла от того, сколько полей заполнила ОДНА фраза:
    //   contract.number                                   -> 0.667
    //   contract.number + contract.date                   -> 0.800
    //   + contract.subject                                -> 0.857
    //   + legal.claim_subject                             -> 0.889
    // Все они извлекаются из одного «№ 77 от 03.03.2026»: одно доказательство,
    // учтённое столько раз, сколько полей оно породило.
    //
    // Хуже того, производный сигнал перевешивал прямой: одиночная фраза
    // «настоящий договор» (вес 3) давала Generic, а одно поле contract.number
    // (вес 4) — уверенный Legal. Поле, извлечённое из фразы, не может быть
    // сильнее самой фразы.
    //
    // Теперь: первое пространство имён домена даёт 3 (ниже самой сильной
    // фразы), каждое ДОПОЛНИТЕЛЬНОЕ пространство имён того же домена — ещё 1,
    // повторные поля внутри пространства — ноль. Это возвращает коду его
    // собственное намерение, объявленное ниже: одного слабого сигнала мало.
    const FIRST_NAMESPACE_WEIGHT: u32 = 3;
    const EXTRA_NAMESPACE_WEIGHT: u32 = 1;
    let mut seen_namespaces = BTreeMap::<DomainKind, BTreeSet<&'static str>>::new();
    for field_id in case.values.keys() {
        let matched: Option<(DomainKind, &'static str)> = if field_id.starts_with("medical.") {
            Some((DomainKind::Medical, "medical."))
        } else if field_id.starts_with("legal.") {
            Some((DomainKind::Legal, "legal."))
        } else if field_id.starts_with("contract.") {
            Some((DomainKind::Legal, "contract."))
        } else if field_id.starts_with("hr.") {
            Some((DomainKind::Hr, "hr."))
        } else if field_id.starts_with("employment.") {
            Some((DomainKind::Hr, "employment."))
        } else if field_id.starts_with("employee.") {
            Some((DomainKind::Hr, "employee."))
        } else if field_id.starts_with("education.") {
            Some((DomainKind::Education, "education."))
        } else if field_id.starts_with("accounting.") {
            Some((DomainKind::Accounting, "accounting."))
        } else if field_id.starts_with("invoice.") {
            Some((DomainKind::Accounting, "invoice."))
        } else if field_id.starts_with("payment.") {
            Some((DomainKind::Accounting, "payment."))
        } else {
            None
        };
        let Some((domain, namespace)) = matched else {
            continue;
        };
        let namespaces = seen_namespaces.entry(domain.clone()).or_default();
        if !namespaces.insert(namespace) {
            // Это пространство имён уже учтено: то же доказательство.
            continue;
        }
        let weight = if namespaces.len() == 1 {
            FIRST_NAMESPACE_WEIGHT
        } else {
            EXTRA_NAMESPACE_WEIGHT
        };
        *scores.entry(domain.clone()).or_default() += weight;
        evidence
            .entry(domain)
            .or_default()
            .push(format!("namespace:{namespace}"));
    }

    let mut ranked = scores.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let Some((domain, score)) = ranked.first().cloned() else {
        return SourceDomainPrediction::default();
    };
    let runner_up_score = ranked.get(1).map(|entry| entry.1).unwrap_or_default();
    // Require at least two independent weak signals (score >= 4) and a clear
    // margin. Otherwise the safe result is Generic and the configured pack is
    // not filtered automatically.
    if score < 4 || score.saturating_sub(runner_up_score) < 2 {
        return SourceDomainPrediction {
            score,
            runner_up_score,
            ..SourceDomainPrediction::default()
        };
    }
    let confidence = ((score.saturating_sub(runner_up_score) as f32 + score as f32)
        / (score as f32 * 2.0 + 4.0))
        .clamp(0.60, 0.98);
    SourceDomainPrediction {
        evidence: evidence.remove(&domain).unwrap_or_default(),
        domain,
        confidence,
        score,
        runner_up_score,
    }
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn phrase_sets() -> Vec<(DomainKind, Vec<(&'static str, u32)>)> {
    vec![
        (
            DomainKind::Medical,
            vec![
                ("история болезни", 4),
                ("дата поступления", 3),
                ("лечащий врач", 3),
                ("диагноз", 2),
                ("пациент", 2),
                ("лечение", 2),
            ],
        ),
        (
            DomainKind::Legal,
            vec![
                ("исковое заявление", 4),
                ("судебное дело", 4),
                ("стороны договора", 3),
                ("настоящий договор", 3),
                ("истец", 2),
                ("ответчик", 2),
                ("доверенность", 2),
            ],
        ),
        (
            DomainKind::Hr,
            vec![
                ("трудовой договор", 4),
                ("приказ о приеме", 4),
                ("личная карточка", 3),
                ("работодатель", 2),
                ("работник", 2),
                ("должность", 1),
            ],
        ),
        (
            DomainKind::Education,
            vec![
                ("образовательная программа", 4),
                ("учебный план", 4),
                ("зачетная книжка", 3),
                ("обучающийся", 2),
                ("студент", 2),
                ("ученик", 2),
            ],
        ),
        (
            DomainKind::Accounting,
            vec![
                ("счет-фактура", 4),
                ("универсальный передаточный документ", 4),
                ("акт сверки", 3),
                ("бухгалтерский учет", 3),
                ("ндс", 2),
                ("к оплате", 2),
                ("банковские реквизиты", 2),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    #[test]
    fn clear_medical_source_is_classified_without_user_profile_selection() {
        let result = classify_source_domain(
            "История болезни. Пациент поступил. Диагноз: ... Лечащий врач: ...",
            &SemanticCase::default(),
        );
        assert_eq!(result.domain, DomainKind::Medical);
        assert!(result.confidence >= 0.6);
    }

    #[test]
    fn one_ambiguous_word_does_not_route_the_case() {
        let result = classify_source_domain("Должность: консультант", &SemanticCase::default());
        assert_eq!(result.domain, DomainKind::Generic);
    }

    #[test]
    fn validated_field_prefix_strengthens_the_prediction() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "accounting.amount".into(),
            SemanticValue::new("accounting.amount", "1000", ValueSource::Scanner, 0.9),
        );
        let result = classify_source_domain("Сумма к оплате", &case);
        assert_eq!(result.domain, DomainKind::Accounting);
    }
}
