//! Evidence-based promotion of document-kit recommendations.
//!
//! Learned kit rules never become automatic merely because a classifier is
//! confident. Promotion requires repeated specialist-confirmed exact matches
//! for the same domain/source cluster and a measured minimum accuracy.

use crate::corpus_recorder::{CorpusAcceptanceSource, CorpusEntry};
use crate::DomainKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct KitRuleKey {
    pub domain: DomainKind,
    pub cluster_id: String,
    pub pack_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnedKitRule {
    pub key: KitRuleKey,
    pub document_ids: Vec<String>,
    pub observations: u32,
    pub exact_matches: u32,
    pub consecutive_clean_confirmations: u32,
    pub accuracy: f32,
    pub promoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KitPromotionPolicy {
    pub min_observations: u32,
    pub min_consecutive_clean: u32,
    pub min_accuracy: f32,
}

impl Default for KitPromotionPolicy {
    fn default() -> Self {
        Self {
            min_observations: 8,
            min_consecutive_clean: 8,
            min_accuracy: 0.98,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitLearningDecision {
    pub document_ids: Vec<String>,
    pub source: String,
    pub confidence: f32,
    pub auto_apply: bool,
    pub reason: String,
}

pub fn learn_kit_rules(
    entries: &[CorpusEntry],
    cluster_by_entry_id: &BTreeMap<String, String>,
    policy: KitPromotionPolicy,
) -> Vec<LearnedKitRule> {
    let mut grouped: BTreeMap<KitRuleKey, Vec<&CorpusEntry>> = BTreeMap::new();
    for entry in entries {
        let Some(cluster_id) = cluster_by_entry_id.get(&entry.entry_id) else {
            continue;
        };
        if entry.kit_documents.is_empty()
            || entry.kit_acceptance_source != CorpusAcceptanceSource::SpecialistConfirmed
        {
            continue;
        }
        grouped
            .entry(KitRuleKey {
                domain: entry.domain.clone(),
                cluster_id: cluster_id.trim().to_string(),
                pack_id: entry.pack_id.clone(),
            })
            .or_default()
            .push(entry);
    }

    let mut rules = Vec::new();
    for (key, observations) in grouped {
        let mut kit_counts: BTreeMap<Vec<String>, u32> = BTreeMap::new();
        for entry in &observations {
            *kit_counts
                .entry(normalize_kit(&entry.kit_documents))
                .or_default() += 1;
        }
        let Some((dominant_kit, dominant_count)) = kit_counts
            .into_iter()
            .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
        else {
            continue;
        };

        let mut consecutive_clean = 0_u32;
        for entry in observations.iter().rev() {
            if normalize_kit(&entry.kit_documents) == dominant_kit
                && normalize_kit(&entry.proposed_kit_documents) == dominant_kit
            {
                consecutive_clean = consecutive_clean.saturating_add(1);
            } else {
                break;
            }
        }
        let total = observations.len() as u32;
        let accuracy = dominant_count as f32 / total.max(1) as f32;
        let promoted = total >= policy.min_observations
            && consecutive_clean >= policy.min_consecutive_clean
            && accuracy >= policy.min_accuracy.clamp(0.0, 1.0);
        rules.push(LearnedKitRule {
            key,
            document_ids: dominant_kit,
            observations: total,
            exact_matches: dominant_count,
            consecutive_clean_confirmations: consecutive_clean,
            accuracy,
            promoted,
        });
    }
    rules.sort_by(|left, right| left.key.cmp(&right.key));
    rules
}

pub fn learn_kit_rules_from_entries(
    entries: &[CorpusEntry],
    policy: KitPromotionPolicy,
) -> Vec<LearnedKitRule> {
    let clusters = entries
        .iter()
        .filter_map(|entry| {
            entry
                .cluster_id
                .as_ref()
                .map(|cluster| (entry.entry_id.clone(), cluster.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    learn_kit_rules(entries, &clusters, policy)
}

pub fn decision_for_key(
    entries: &[CorpusEntry],
    key: &KitRuleKey,
    policy: KitPromotionPolicy,
) -> Option<KitLearningDecision> {
    learn_kit_rules_from_entries(entries, policy)
        .into_iter()
        .find(|rule| &rule.key == key)
        .map(|rule| decide_learned_kit(&rule))
}

pub fn decide_learned_kit(rule: &LearnedKitRule) -> KitLearningDecision {
    KitLearningDecision {
        document_ids: rule.document_ids.clone(),
        source: "learned_corpus_rule".into(),
        confidence: rule.accuracy.clamp(0.0, 1.0),
        auto_apply: rule.promoted,
        reason: if rule.promoted {
            format!(
                "Правило подтверждено: {} наблюдений, {} чистых подтверждений подряд, точность {:.1}%.",
                rule.observations,
                rule.consecutive_clean_confirmations,
                rule.accuracy * 100.0
            )
        } else {
            format!(
                "Комплект только предлагается: {} наблюдений, {} чистых подтверждений подряд, точность {:.1}%.",
                rule.observations,
                rule.consecutive_clean_confirmations,
                rule.accuracy * 100.0
            )
        },
    }
}

fn normalize_kit(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus_recorder::{CorpusAcceptanceSource, CorpusEntry};

    fn entry(index: usize, proposed: &[&str], actual: &[&str]) -> CorpusEntry {
        CorpusEntry {
            entry_id: format!("entry-{index}"),
            case_id: format!("case-{index}"),
            source_sha256: "a".repeat(64),
            input_text_sha256: "b".repeat(64),
            domain: DomainKind::Hr,
            pack_id: Some("hr.ru.v1".into()),
            cluster_id: Some("employment-intake".into()),
            model_proposals: vec![],
            deterministic: vec![],
            final_accepted: vec![],
            field_acceptance_source: CorpusAcceptanceSource::SpecialistConfirmed,
            proposed_kit_documents: proposed.iter().map(|value| value.to_string()).collect(),
            kit_proposal_source: Some("router".into()),
            kit_documents: actual.iter().map(|value| value.to_string()).collect(),
            kit_acceptance_source: CorpusAcceptanceSource::SpecialistConfirmed,
            created_at: format!("2026-07-{index:02}T00:00:00Z"),
        }
    }

    #[test]
    fn rule_is_not_promoted_before_clean_streak() {
        let entries = (1..=7)
            .map(|index| entry(index, &["contract", "order"], &["contract", "order"]))
            .collect::<Vec<_>>();
        let clusters = entries
            .iter()
            .map(|item| (item.entry_id.clone(), "employment-intake".into()))
            .collect();
        let rules = learn_kit_rules(&entries, &clusters, KitPromotionPolicy::default());
        assert_eq!(rules.len(), 1);
        assert!(!rules[0].promoted);
        assert!(!decide_learned_kit(&rules[0]).auto_apply);
    }

    #[test]
    fn eight_clean_confirmations_promote_exact_kit() {
        let entries = (1..=8)
            .map(|index| entry(index, &["contract", "order"], &["order", "contract"]))
            .collect::<Vec<_>>();
        let clusters = entries
            .iter()
            .map(|item| (item.entry_id.clone(), "employment-intake".into()))
            .collect();
        let rules = learn_kit_rules(&entries, &clusters, KitPromotionPolicy::default());
        assert!(rules[0].promoted);
        assert_eq!(rules[0].document_ids, vec!["contract", "order"]);
        assert!(decide_learned_kit(&rules[0]).auto_apply);
    }

    #[test]
    fn direct_entry_learning_uses_persisted_cluster() {
        let entries = (1..=8)
            .map(|index| entry(index, &["contract", "order"], &["contract", "order"]))
            .collect::<Vec<_>>();
        let rules = learn_kit_rules_from_entries(&entries, KitPromotionPolicy::default());
        assert_eq!(rules.len(), 1);
        assert!(rules[0].promoted);
        assert_eq!(rules[0].key.cluster_id, "employment-intake");
    }

    #[test]
    fn correction_resets_clean_streak_and_blocks_auto_apply() {
        let mut entries = (1..=8)
            .map(|index| entry(index, &["contract", "order"], &["contract", "order"]))
            .collect::<Vec<_>>();
        entries.push(entry(9, &["contract"], &["contract", "order"]));
        let clusters = entries
            .iter()
            .map(|item| (item.entry_id.clone(), "employment-intake".into()))
            .collect();
        let rules = learn_kit_rules(&entries, &clusters, KitPromotionPolicy::default());
        assert!(!rules[0].promoted);
        assert_eq!(rules[0].consecutive_clean_confirmations, 0);
    }

    #[test]
    fn zero_touch_and_legacy_entries_can_never_promote_a_rule() {
        let mut entries = (1..=8)
            .map(|index| entry(index, &["contract", "order"], &["contract", "order"]))
            .collect::<Vec<_>>();
        for item in &mut entries {
            item.kit_acceptance_source = CorpusAcceptanceSource::ZeroTouchShadow;
        }
        assert!(learn_kit_rules_from_entries(&entries, KitPromotionPolicy::default()).is_empty());

        for item in &mut entries {
            item.kit_acceptance_source = CorpusAcceptanceSource::LegacyUnverified;
        }
        assert!(learn_kit_rules_from_entries(&entries, KitPromotionPolicy::default()).is_empty());
    }

}
