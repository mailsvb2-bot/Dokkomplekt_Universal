use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UsageLedger {
    pub month_counters: BTreeMap<String, UsageCounter>,
    pub trial_created_total: u32,
    pub last_seen_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UsageCounter {
    pub created_documents: u32,
}

impl UsageLedger {
    pub fn documents_for_month(&self, month_key: &str) -> u32 {
        self.month_counters
            .get(month_key)
            .map(|counter| counter.created_documents)
            .unwrap_or(0)
    }

    pub fn record_documents(&mut self, month_key: &str, count: u32) {
        let counter = self
            .month_counters
            .entry(month_key.to_string())
            .or_default();
        counter.created_documents = counter.created_documents.saturating_add(count);
    }

    pub fn record_trial_documents(&mut self, month_key: &str, count: u32) {
        self.record_documents(month_key, count);
        self.trial_created_total = self.trial_created_total.saturating_add(count);
    }

    pub fn rollback_documents(&mut self, month_key: &str, count: u32, trial: bool) {
        if let Some(counter) = self.month_counters.get_mut(month_key) {
            counter.created_documents = counter.created_documents.saturating_sub(count);
            if counter.created_documents == 0 {
                self.month_counters.remove(month_key);
            }
        }
        if trial {
            self.trial_created_total = self.trial_created_total.saturating_sub(count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_updates_are_saturating_and_month_scoped() {
        let mut ledger = UsageLedger::default();
        ledger.record_documents("2026-07", 2);
        ledger.record_trial_documents("2026-07", 3);
        ledger.record_documents("2026-08", 1);
        assert_eq!(ledger.documents_for_month("2026-07"), 5);
        assert_eq!(ledger.documents_for_month("2026-08"), 1);
        assert_eq!(ledger.trial_created_total, 3);
        ledger.rollback_documents("2026-07", 2, true);
        assert_eq!(ledger.documents_for_month("2026-07"), 3);
        assert_eq!(ledger.trial_created_total, 1);
    }
}
