//! Domain-neutral selection of user-owned text snippets for repeated records.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentCandidate {
    pub id: String,
    pub label: String,
    pub text: String,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentMatch {
    pub candidate_id: String,
    pub score: u32,
    pub matched_terms: Vec<String>,
}

pub fn rank_content_candidates(query: &str, candidates: &[ContentCandidate]) -> Vec<ContentMatch> {
    let terms = normalized_terms(query);
    let mut out = candidates
        .iter()
        .map(|candidate| {
            let haystack =
                format!("{} {}", candidate.label, candidate.keywords.join(" ")).to_lowercase();
            let matched_terms = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let exact_label_bonus =
                u32::from(candidate.label.trim().eq_ignore_ascii_case(query.trim())) * 100;
            ContentMatch {
                candidate_id: candidate.id.clone(),
                score: exact_label_bonus + matched_terms.len() as u32 * 10,
                matched_terms,
            }
        })
        .filter(|result| result.score > 0)
        .collect::<Vec<_>>();
    out.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    out
}

pub fn select_best_content_candidate<'a>(
    query: &str,
    candidates: &'a [ContentCandidate],
) -> Option<&'a ContentCandidate> {
    let best = rank_content_candidates(query, candidates)
        .into_iter()
        .next()?;
    candidates
        .iter()
        .find(|candidate| candidate.id == best.candidate_id)
}

fn normalized_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate(id: &str, label: &str, keywords: &[&str]) -> ContentCandidate {
        ContentCandidate {
            id: id.into(),
            label: label.into(),
            text: format!("text-{id}"),
            keywords: keywords.iter().map(|x| (*x).into()).collect(),
        }
    }
    #[test]
    fn works_for_legal_and_education_content_not_only_diagnoses() {
        let items = vec![
            candidate("legal", "Договор аренды", &["аренда", "помещение"]),
            candidate(
                "lesson",
                "Урок алгебры",
                &["математика", "квадратные уравнения"],
            ),
        ];
        assert_eq!(
            select_best_content_candidate("проверка договора аренды", &items)
                .unwrap()
                .id,
            "legal"
        );
        assert_eq!(
            select_best_content_candidate("математика квадратные уравнения", &items)
                .unwrap()
                .id,
            "lesson"
        );
    }
    #[test]
    fn unrelated_query_does_not_select_arbitrary_text() {
        let items = vec![candidate("a", "Смена", &["касса"])];
        assert!(select_best_content_candidate("лабораторный анализ", &items).is_none());
    }
}
