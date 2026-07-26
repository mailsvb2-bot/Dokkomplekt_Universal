use crate::core_error::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ClockState {
    #[serde(default)]
    pub last_seen_utc: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockGuard {
    pub rollback_tolerance_seconds: i64,
}

impl Default for ClockGuard {
    fn default() -> Self {
        Self {
            rollback_tolerance_seconds: 48 * 60 * 60,
        }
    }
}

impl ClockGuard {
    pub fn validate(&self, now: OffsetDateTime, state: &ClockState) -> CoreResult<()> {
        if let Some(last_seen) = state.last_seen_utc {
            let delta = last_seen - now;
            if delta.whole_seconds() > self.rollback_tolerance_seconds {
                return Err(CoreError::ClockRollback);
            }
        }
        Ok(())
    }
}
