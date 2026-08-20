//! Conservative document-type discovery and ready-set recommendation.
//!
//! The router never generates a document by itself. It ranks configured templates,
//! groups structurally similar inputs, and proposes a set only when the best match
//! has a clear margin. Ambiguous or previously unseen layouts remain review-only.

use crate::{classify_source_domain, DocumentPack, DomainKind, SemanticCase};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentMatch {
    pub document_id: String,
    pub button_label: String,
    pub role_id: String,
    pub score: f32,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentRoutingRecommendation {
    pub domain: DomainKind,
    pub domain_confidence: f32,
    pub predicted_role: Option<String>,
    pub cluster_id: String,
    pub cluster_confidence: f32,
    pub recommended_document_ids: Vec<String>,
    pub matches: Vec<DocumentMatch>,
    pub auto_select: bool,
    pub review_required: bool,
    pub reasons: Vec<String>,
}

impl Default for DocumentRoutingRecommendation {
    fn default() -> Self {
        Self {
            domain: DomainKind::Generic,
            domain_confidence: 0.0,
            predicted_role: None,
            cluster_id: "unclassified".into(),
            cluster_confidence: 0.0,
            recommended_document_ids: Vec::new(),
            matches: Vec::new(),
            auto_select: false,
            review_required: true,
            reasons: vec!["Тип входящего документа не определён уверенно.".into()],
        }
    }
}

pub fn recommend_document_bundle(
    source_text: &str,
    case: &SemanticCase,
    pack: &DocumentPack,
) -> DocumentRoutingRecommendation {
    let domain_prediction = classify_source_domain(source_text, case);
    let source_tokens = routing_tokens(source_text);
    let source_labels = layout_labels(source_text);
    let cluster_id = stable_cluster_id(&source_labels, &source_tokens);
    let role_scores = score_roles(source_text, case);
    let predicted_role = role_scores.first().map(|item| item.0.clone());
    let role_confidence = role_confidence(&role_scores);

    // Домен и его уверенность обязаны выводиться вместе.
    //
    // До 18.4.0 fallback по сходству с пакетом возвращал домен, но оставлял
    // `domain_prediction.confidence` равной нулю. Поскольку
    //   cluster = domain_conf*0.45 + role_conf*0.35 + top*0.20,
    // максимум при нулевой первой компоненте равен
    //   0.35*0.99 + 0.20*1.0 = 0.5465 < 0.60,
    // то есть порог `cluster_confidence >= 0.60` не мог быть взят никогда.
    // Ветка была мёртвой: любой документ, чей домен опознан только по
    // сходству с настроенными шаблонами, гарантированно уходил на ручное
    // подтверждение. На «акте сверки» — с весом фразы 6.0 — это и наблюдалось.
    let (effective_domain, effective_domain_confidence) =
        if domain_prediction.domain != DomainKind::Generic {
            (
                domain_prediction.domain.clone(),
                domain_prediction.confidence,
            )
        } else {
            match infer_domain_from_pack_match_scored(pack, &source_tokens) {
                // Уверенность вывода по сходству намеренно ниже фразовой:
                // это косвенное свидетельство, и оно не должно давать
                // автозапуск в одиночку.
                Some((domain, similarity)) => (domain, (similarity * 2.4).clamp(0.0, 0.80)),
                None => (DomainKind::Generic, 0.0),
            }
        };

    let mut matches = pack
        .documents
        .iter()
        .map(|document| {
            let mut score = 0.0_f32;
            let mut evidence = Vec::new();
            if effective_domain != DomainKind::Generic && document.category == effective_domain {
                score += 0.34;
                evidence.push("совпадает профессиональная область".into());
            }
            let normalized_role = normalize(document.role_id.as_str());
            if let Some(role) = predicted_role.as_deref() {
                let role_match = role_equivalent(role, &normalized_role);
                if role_match {
                    score += 0.36 * role_confidence.max(0.55);
                    evidence.push(format!("роль источника: {role}"));
                }
            }
            let label_tokens = routing_tokens(&format!(
                "{} {} {}",
                document.button_label,
                document.role_id,
                document.required_fields.join(" ")
            ));
            let lexical = jaccard(&source_tokens, &label_tokens);
            if lexical > 0.0 {
                score += lexical * 0.18;
                evidence.push(format!("сходство названия/полей {:.0}%", lexical * 100.0));
            }
            let coverage = required_field_coverage(case, &document.required_fields);
            if coverage > 0.0 {
                score += coverage * 0.18;
                evidence.push(format!(
                    "распознано обязательных полей {:.0}%",
                    coverage * 100.0
                ));
            }
            if document.is_static_copy {
                score -= 0.04;
            }
            DocumentMatch {
                document_id: document.id.clone(),
                button_label: document.button_label.clone(),
                role_id: document.role_id.clone(),
                score: score.clamp(0.0, 1.0),
                evidence,
            }
        })
        .filter(|candidate| candidate.score > 0.05)
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.button_label.cmp(&right.button_label))
    });

    let top = matches.first().map(|item| item.score).unwrap_or_default();
    let runner_up = matches.get(1).map(|item| item.score).unwrap_or_default();
    let clear_margin = top - runner_up >= 0.08;
    let primary_confident = top >= 0.62 && clear_margin;
    let cluster_confidence =
        ((effective_domain_confidence * 0.45) + (role_confidence * 0.35) + (top * 0.20))
            .clamp(0.0, 0.99);

    let mut recommended = Vec::new();
    if let Some(primary) = matches.first() {
        if top >= 0.56 {
            recommended.push(primary.document_id.clone());
            if let Some(role) = predicted_role.as_deref() {
                for companion_role in related_document_roles(role) {
                    if let Some(companion) = matches.iter().find(|candidate| {
                        role_equivalent(companion_role, &normalize(&candidate.role_id))
                            && candidate.score >= 0.32
                    }) {
                        if !recommended.contains(&companion.document_id) {
                            recommended.push(companion.document_id.clone());
                        }
                    }
                }
            }
        }
    }
    recommended.truncate(8);

    let mut reasons = Vec::new();
    if domain_prediction.domain == DomainKind::Generic {
        reasons.push("Профессиональная область неоднозначна; применено только сходство с настроенными шаблонами.".into());
    } else {
        reasons.push(format!(
            "Область определена с уверенностью {:.0}%.",
            domain_prediction.confidence * 100.0
        ));
    }
    if let Some(role) = predicted_role.as_deref() {
        reasons.push(format!(
            "Предполагаемый тип документа: {role} ({:.0}%).",
            role_confidence * 100.0
        ));
    } else {
        reasons.push("Новый или неописанный тип: требуется подтверждение специалиста.".into());
    }
    if !clear_margin && matches.len() > 1 {
        reasons.push("Два лучших шаблона слишком близки по оценке; автозапуск запрещён.".into());
    }
    if recommended.is_empty() {
        // Формулировка обязана отличать «похожего нет» от «похожие есть,
        // но ни один не взял порог». Второе — обычная неоднозначность
        // («акт выполненных работ» против «акта приёма-передачи»), и она
        // разрешается выбором, а не сообщением об отсутствии шаблона.
        if top >= 0.32 {
            reasons.push(format!(
                "Ближайший шаблон набрал {:.0}%: ниже порога автозапуска, нужен выбор.",
                top * 100.0
            ));
        } else {
            reasons.push("В текущем наборе нет достаточно похожего документа.".into());
        }
    }

    let auto_select = primary_confident
        && cluster_confidence >= 0.60
        && !recommended.is_empty()
        && effective_domain != DomainKind::Generic;
    DocumentRoutingRecommendation {
        domain: effective_domain,
        domain_confidence: effective_domain_confidence,
        predicted_role,
        cluster_id,
        cluster_confidence,
        recommended_document_ids: recommended,
        matches,
        auto_select,
        review_required: !auto_select,
        reasons,
    }
}

fn required_field_coverage(case: &SemanticCase, fields: &[String]) -> f32 {
    let fields = fields
        .iter()
        .map(|field| crate::canonical_storage_field_id(field))
        .collect::<BTreeSet<_>>();
    if fields.is_empty() {
        return 0.0;
    }
    let found = fields.iter().filter(|field| case.has(field)).count();
    found as f32 / fields.len() as f32
}

fn infer_domain_from_pack_match_scored(
    pack: &DocumentPack,
    source_tokens: &BTreeSet<String>,
) -> Option<(DomainKind, f32)> {
    let mut scores = BTreeMap::<DomainKind, f32>::new();
    for document in &pack.documents {
        let document_tokens = routing_tokens(&format!(
            "{} {} {}",
            document.button_label,
            document.role_id,
            document.required_fields.join(" ")
        ));
        *scores.entry(document.category.clone()).or_default() +=
            jaccard(source_tokens, &document_tokens);
    }
    scores
        .into_iter()
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .filter(|(_, score)| *score >= 0.12)
}

fn score_roles(source_text: &str, case: &SemanticCase) -> Vec<(String, f32)> {
    let normalized = normalize(source_text);
    let mut scores = BTreeMap::<String, f32>::new();
    for (role, phrases) in role_phrases() {
        for (phrase, weight) in phrases {
            if normalized.contains(phrase) {
                *scores.entry(role.to_string()).or_default() += weight;
            }
        }
    }
    for field in case.values.keys() {
        for (prefix, role, weight) in field_role_hints() {
            if field.starts_with(prefix) {
                *scores.entry(role.to_string()).or_default() += weight;
            }
        }
    }
    let max = scores.values().copied().fold(0.0_f32, f32::max).max(1.0);
    let mut ranked = scores
        .into_iter()
        .map(|(role, score)| (role, (score / (max + 1.5)).clamp(0.0, 0.99)))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    ranked
}

fn role_confidence(scores: &[(String, f32)]) -> f32 {
    let Some((_, top)) = scores.first() else {
        return 0.0;
    };
    let runner_up = scores.get(1).map(|item| item.1).unwrap_or_default();
    if *top < 0.45 || *top - runner_up < 0.08 {
        (*top * 0.65).clamp(0.0, 0.69)
    } else {
        (*top + (*top - runner_up) * 0.25).clamp(0.0, 0.99)
    }
}

pub fn predict_document_role(source_text: &str) -> Option<(String, f32)> {
    let scores = score_roles(source_text, &SemanticCase::default());
    let confidence = role_confidence(&scores);
    scores.first().map(|(role, _)| (role.clone(), confidence))
}

pub fn related_document_roles(role: &str) -> &'static [&'static str] {
    match role {
        "employment_contract" => &[
            "employment_order",
            "personal_data_consent",
            "familiarization_sheet",
        ],
        "employment_order" => &[
            "employment_contract",
            "personal_data_consent",
            "familiarization_sheet",
        ],
        "contract" => &["cover_letter", "acceptance_act"],
        "invoice" => &["service_act"],
        "service_act" => &["invoice"],
        "discharge" => &["diaries"],
        "primary" => &["diaries"],
        _ => &[],
    }
}

/// Сегменты role_id: `employment_contract` -> {`employment`, `contract`}.
fn role_segments(value: &str) -> BTreeSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn role_equivalent(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    // До 18.4.0 здесь стояло `actual.contains(expected)`. Это давало
    // `role_equivalent("contract", "employment_contract") == true`:
    // договор поставки подтягивал кадровый шаблон трудового договора
    // при смешанном наборе документов. Сравнение переведено на полные
    // сегменты идентификатора, подстрочного совпадения больше нет.
    let expected_segments = role_segments(expected);
    let actual_segments = role_segments(actual);
    if !expected_segments.is_empty() && expected_segments == actual_segments {
        return true;
    }
    // Эти ветки сопоставляют роль со СВОБОДНЫМ русским именем, которое
    // специалист дал своему шаблону, поэтому здесь основа слова уместна.
    // Но взаимоисключающие роли обязаны исключать друг друга явно.
    let employment_flavoured = actual.contains("трудов") || actual.contains("employment");
    match expected {
        "discharge" => actual.contains("выпис") || actual.contains("epicrisis"),
        "diaries" => actual.contains("дневник") || actual.contains("diary"),
        "primary" => actual.contains("первич") || actual.contains("primary"),
        "employment_contract" => employment_flavoured && actual.contains("договор"),
        "employment_order" => actual.contains("приказ") && actual.contains("прием"),
        "personal_data_consent" => actual.contains("персональ") && actual.contains("соглас"),
        "familiarization_sheet" => actual.contains("ознаком"),
        // Гражданско-правовой договор — это НЕ трудовой договор.
        "contract" => {
            !employment_flavoured && (actual.contains("договор") || actual.contains("contract"))
        }
        "acceptance_act" => actual.contains("акт") && actual.contains("прием"),
        "cover_letter" => actual.contains("сопровод"),
        "invoice" => actual.contains("счет") || actual.contains("invoice"),
        "service_act" => actual.contains("акт") && actual.contains("услуг"),
        "reconciliation" => actual.contains("сверк"),
        _ => false,
    }
}

fn role_phrases() -> Vec<(&'static str, Vec<(&'static str, f32)>)> {
    vec![
        (
            "discharge",
            vec![
                ("выписной эпикриз", 5.0),
                ("дата выписки", 2.0),
                ("выписан", 1.5),
            ],
        ),
        (
            "diaries",
            vec![("дневник наблюдения", 5.0), ("динамика состояния", 2.0)],
        ),
        (
            "primary",
            vec![("первичный осмотр", 5.0), ("дата поступления", 1.5)],
        ),
        (
            "employment_contract",
            vec![
                ("трудовой договор", 5.0),
                ("работодатель", 1.5),
                ("работник", 1.5),
            ],
        ),
        (
            "employment_order",
            vec![("приказ о приеме", 5.0), ("принять на работу", 2.0)],
        ),
        (
            "personal_data_consent",
            vec![("согласие на обработку персональных данных", 6.0)],
        ),
        (
            "familiarization_sheet",
            vec![("лист ознакомления", 5.0), ("ознакомлен", 1.5)],
        ),
        (
            "contract",
            vec![
                ("настоящий договор", 5.0),
                ("предмет договора", 2.0),
                ("стороны договора", 2.0),
            ],
        ),
        (
            "acceptance_act",
            vec![("акт приема-передачи", 5.0), ("акт выполненных работ", 4.0)],
        ),
        ("cover_letter", vec![("сопроводительное письмо", 5.0)]),
        (
            "claim",
            vec![
                ("исковое заявление", 6.0),
                ("истец", 1.5),
                ("ответчик", 1.5),
            ],
        ),
        (
            "invoice",
            vec![
                ("счет на оплату", 5.0),
                ("счет-фактура", 5.0),
                ("к оплате", 1.5),
            ],
        ),
        (
            "service_act",
            vec![("акт оказанных услуг", 5.0), ("акт выполненных работ", 5.0)],
        ),
        ("reconciliation", vec![("акт сверки", 6.0)]),
        (
            "certificate",
            vec![("справка об обучении", 5.0), ("настоящая справка", 1.5)],
        ),
        (
            "grade_report",
            vec![("ведомость успеваемости", 5.0), ("оценка", 1.0)],
        ),
    ]
}

fn field_role_hints() -> Vec<(&'static str, &'static str, f32)> {
    vec![
        ("medical.discharge_", "discharge", 2.5),
        ("medical.admission_", "primary", 1.5),
        ("employee.contract_", "employment_contract", 2.0),
        ("employee.hire_", "employment_order", 1.5),
        ("contract.", "contract", 1.0),
        ("legal.claim_", "claim", 2.5),
        ("accounting.invoice_", "invoice", 2.5),
        ("education.grade", "grade_report", 2.0),
    ]
}

fn layout_labels(text: &str) -> BTreeSet<String> {
    text.lines()
        .take(120)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.chars().count() > 120 {
                return None;
            }
            let label = line
                .split_once(':')
                .map(|(left, _)| left)
                .unwrap_or(line)
                .trim();
            let normalized = normalize(label);
            if normalized.chars().count() < 3 {
                None
            } else {
                Some(normalized)
            }
        })
        .take(40)
        .collect()
}

fn routing_tokens(text: &str) -> BTreeSet<String> {
    normalize(text)
        .split(|ch: char| !ch.is_alphanumeric() && ch != '.')
        .filter(|token| token.chars().count() >= 3)
        .filter(|token| !STOP_WORDS.contains(token))
        .take(500)
        .map(str::to_string)
        .collect()
}

fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    intersection as f32 / union as f32
}

fn stable_cluster_id(labels: &BTreeSet<String>, tokens: &BTreeSet<String>) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in labels
        .iter()
        .chain(tokens.iter().take(80))
        .flat_map(|value| value.as_bytes().iter().copied().chain([0]))
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("layout-{hash:016x}")
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ё', "е")
        .replace(['—', '–'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const STOP_WORDS: &[&str] = &[
    "для",
    "или",
    "это",
    "как",
    "при",
    "его",
    "она",
    "они",
    "под",
    "над",
    "без",
    "дата",
    "номер",
    "документ",
    "документа",
    "настоящий",
    "настоящая",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentTemplateSpec, ValueSource};

    fn document(id: &str, label: &str, role: &str, category: DomainKind) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: id.into(),
            button_label: label.into(),
            template_path: format!("{id}.docx"),
            category,
            role_id: role.into(),
            required_fields: Vec::new(),
            placeholders: Vec::new(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn hr_source_proposes_ready_onboarding_bundle() {
        let pack = DocumentPack {
            pack_id: "p".into(),
            name: "HR".into(),
            documents: vec![
                document(
                    "contract",
                    "Трудовой договор",
                    "employment_contract",
                    DomainKind::Hr,
                ),
                document(
                    "order",
                    "Приказ о приёме",
                    "employment_order",
                    DomainKind::Hr,
                ),
                document(
                    "consent",
                    "Согласие ПД",
                    "personal_data_consent",
                    DomainKind::Hr,
                ),
                document(
                    "sheet",
                    "Лист ознакомления",
                    "familiarization_sheet",
                    DomainKind::Hr,
                ),
            ],
        };
        let result = recommend_document_bundle(
            "Трудовой договор. Работодатель ООО Ромашка. Работник Иванов. Должность инженер.",
            &SemanticCase::default(),
            &pack,
        );
        assert_eq!(result.domain, DomainKind::Hr);
        assert_eq!(
            result.recommended_document_ids.first().map(String::as_str),
            Some("contract")
        );
        assert!(result
            .recommended_document_ids
            .contains(&"order".to_string()));
        assert!(result.auto_select);
    }

    #[test]
    fn ambiguous_text_never_auto_selects() {
        let pack = DocumentPack {
            pack_id: "p".into(),
            name: "mixed".into(),
            documents: vec![
                document("a", "Документ A", "custom_a", DomainKind::Generic),
                document("b", "Документ B", "custom_b", DomainKind::Generic),
            ],
        };
        let result = recommend_document_bundle("Иванов Иван", &SemanticCase::default(), &pack);
        assert!(!result.auto_select);
        assert!(result.review_required);
    }

    #[test]
    fn recognized_fields_raise_template_coverage() {
        let mut case = SemanticCase::default();
        case.values.insert(
            "accounting.invoice_number".into(),
            crate::SemanticValue::new(
                "accounting.invoice_number",
                "148",
                ValueSource::Scanner,
                0.95,
            ),
        );
        let mut invoice = document(
            "invoice",
            "Счёт на оплату",
            "invoice",
            DomainKind::Accounting,
        );
        invoice.required_fields = vec!["accounting.invoice_number".into()];
        let pack = DocumentPack {
            pack_id: "p".into(),
            name: "accounting".into(),
            documents: vec![invoice],
        };
        let result =
            recommend_document_bundle("Счёт на оплату № 148. К оплате 1000 руб.", &case, &pack);
        assert_eq!(result.recommended_document_ids, vec!["invoice".to_string()]);
        assert!(result.matches[0]
            .evidence
            .iter()
            .any(|item| item.contains("обязательных полей")));
    }
}
