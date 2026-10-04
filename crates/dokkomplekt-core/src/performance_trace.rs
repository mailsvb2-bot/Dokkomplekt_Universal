use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canon §26.4 stage names. Keep this list closed so a performance trace cannot
/// quietly turn into an arbitrary diagnostic log containing document content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceStage {
    SourceOpen,
    SourceParse,
    CandidateIndex,
    SourceResolve,
    PromptPlan,
    Preflight,
    ReferenceClone,
    Replay,
    PhysicalReadback,
    Verify,
    Publish,
    Recovery,
}

impl PerformanceStage {
    pub const ORDERED: [Self; 12] = [
        Self::SourceOpen,
        Self::SourceParse,
        Self::CandidateIndex,
        Self::SourceResolve,
        Self::PromptPlan,
        Self::Preflight,
        Self::ReferenceClone,
        Self::Replay,
        Self::PhysicalReadback,
        Self::Verify,
        Self::Publish,
        Self::Recovery,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceOpen => "source_open",
            Self::SourceParse => "source_parse",
            Self::CandidateIndex => "candidate_index",
            Self::SourceResolve => "source_resolve",
            Self::PromptPlan => "prompt_plan",
            Self::Preflight => "preflight",
            Self::ReferenceClone => "reference_clone",
            Self::Replay => "replay",
            Self::PhysicalReadback => "physical_readback",
            Self::Verify => "verify",
            Self::Publish => "publish",
            Self::Recovery => "recovery",
        }
    }

    const fn ordinal(self) -> usize {
        match self {
            Self::SourceOpen => 0,
            Self::SourceParse => 1,
            Self::CandidateIndex => 2,
            Self::SourceResolve => 3,
            Self::PromptPlan => 4,
            Self::Preflight => 5,
            Self::ReferenceClone => 6,
            Self::Replay => 7,
            Self::PhysicalReadback => 8,
            Self::Verify => 9,
            Self::Publish => 10,
            Self::Recovery => 11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceClass {
    SmallDocx,
    TypicalDocx,
    LargeDocx,
    TableHeavy,
    HeaderFooterHeavy,
    LongText,
    RepeatedBlocks,
    Batch10,
    Batch50,
    AccountingTable,
    HrKit,
    ManyRoles,
    Ocr,
    RuntimeLayout,
    Pdf,
    SlowStorage,
    NetworkStorage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceCacheState {
    ColdCache,
    WarmCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceRunPhase {
    FirstRun,
    RepeatRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceWorkload {
    SingleDocument,
    Batch10,
    Batch50,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceOutcome {
    Completed,
    Attention,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceStageMeasurement {
    pub stage: PerformanceStage,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceTraceContext {
    /// Opaque local identifier only. Never a path, name, case number or source text.
    pub run_id: String,
    pub app_version: String,
    pub class: PerformanceClass,
    pub cache_state: PerformanceCacheState,
    pub run_phase: PerformanceRunPhase,
    pub workload: PerformanceWorkload,
    pub batch_size: u32,
    pub ocr_used: bool,
    pub runtime_layout_used: bool,
    pub pdf_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceTrace {
    pub schema: String,
    pub context: PerformanceTraceContext,
    pub stages: Vec<PerformanceStageMeasurement>,
    pub total_machine_ms: u64,
    pub outcome: PerformanceOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowRunReport {
    pub schema: String,
    pub run_id: String,
    pub total_machine_ms: u64,
    pub bottleneck_stage: Option<PerformanceStage>,
    pub bottleneck_duration_ms: u64,
    pub stages: Vec<PerformanceStageMeasurement>,
    pub class: PerformanceClass,
    pub cache_state: PerformanceCacheState,
    pub run_phase: PerformanceRunPhase,
    pub workload: PerformanceWorkload,
    pub batch_size: u32,
}

fn valid_opaque_identifier(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 128
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

impl PerformanceTraceContext {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_opaque_identifier(&self.run_id) {
            return Err(
                "performance run_id must be an opaque ASCII identifier without paths or text"
                    .into(),
            );
        }
        if !valid_opaque_identifier(&self.app_version) || self.app_version.len() > 64 {
            return Err("performance app_version must be a short ASCII version token".into());
        }
        if self.batch_size == 0 {
            return Err("performance batch_size must be positive".into());
        }
        match self.workload {
            PerformanceWorkload::SingleDocument if self.batch_size != 1 => {
                return Err("single_document performance workload must have batch_size=1".into())
            }
            PerformanceWorkload::Batch10 if self.batch_size != 10 => {
                return Err("batch_10 performance workload must have batch_size=10".into())
            }
            PerformanceWorkload::Batch50 if self.batch_size != 50 => {
                return Err("batch_50 performance workload must have batch_size=50".into())
            }
            _ => {}
        }
        Ok(())
    }
}

impl PerformanceTrace {
    pub const SCHEMA: &'static str = "dokkomplekt.performance-trace.v1";

    pub fn new(
        context: PerformanceTraceContext,
        stages: Vec<PerformanceStageMeasurement>,
        outcome: PerformanceOutcome,
    ) -> Result<Self, String> {
        context.validate()?;
        let mut seen = BTreeSet::new();
        let mut previous_ordinal = None;
        let mut total_machine_ms = 0u64;

        for measurement in &stages {
            if !seen.insert(measurement.stage) {
                return Err(format!(
                    "duplicate performance stage: {}",
                    measurement.stage.as_str()
                ));
            }
            let ordinal = measurement.stage.ordinal();
            if let Some(previous) = previous_ordinal {
                if ordinal <= previous {
                    return Err("performance stages must follow canonical order".into());
                }
            }
            previous_ordinal = Some(ordinal);
            total_machine_ms = total_machine_ms.saturating_add(measurement.duration_ms);
        }

        let has_publish = seen.contains(&PerformanceStage::Publish);
        let has_recovery = seen.contains(&PerformanceStage::Recovery);
        match outcome {
            PerformanceOutcome::Completed if !has_publish => {
                return Err("completed performance trace must include publish".into())
            }
            PerformanceOutcome::Completed if has_recovery => {
                return Err("completed performance trace cannot include recovery".into())
            }
            PerformanceOutcome::Failed | PerformanceOutcome::Attention | PerformanceOutcome::Cancelled
                if has_publish =>
            {
                return Err("non-completed performance trace cannot claim publish".into())
            }
            _ => {}
        }

        Ok(Self {
            schema: Self::SCHEMA.into(),
            context,
            stages,
            total_machine_ms,
            outcome,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != Self::SCHEMA {
            return Err(format!(
                "unsupported performance trace schema: {}",
                self.schema
            ));
        }
        let rebuilt = Self::new(
            self.context.clone(),
            self.stages.clone(),
            self.outcome,
        )?;
        if rebuilt.total_machine_ms != self.total_machine_ms {
            return Err("performance trace total_machine_ms does not match stage durations".into());
        }
        Ok(())
    }

    pub fn slow_run_report(&self) -> SlowRunReport {
        let bottleneck = self
            .stages
            .iter()
            .max_by_key(|measurement| measurement.duration_ms);
        SlowRunReport {
            schema: "dokkomplekt.slow-run-report.v1".into(),
            run_id: self.context.run_id.clone(),
            total_machine_ms: self.total_machine_ms,
            bottleneck_stage: bottleneck.map(|measurement| measurement.stage),
            bottleneck_duration_ms: bottleneck
                .map(|measurement| measurement.duration_ms)
                .unwrap_or(0),
            stages: self.stages.clone(),
            class: self.context.class,
            cache_state: self.context.cache_state,
            run_phase: self.context.run_phase,
            workload: self.context.workload,
            batch_size: self.context.batch_size,
        }
    }

    pub fn slow_run_report_if_over(&self, threshold_ms: u64) -> Result<Option<SlowRunReport>, String> {
        if threshold_ms == 0 {
            return Err("slow-run threshold must be positive".into());
        }
        Ok((self.total_machine_ms > threshold_ms).then(|| self.slow_run_report()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> PerformanceTraceContext {
        PerformanceTraceContext {
            run_id: "run_01HXYZ".into(),
            app_version: "1.0.0".into(),
            class: PerformanceClass::TypicalDocx,
            cache_state: PerformanceCacheState::WarmCache,
            run_phase: PerformanceRunPhase::RepeatRun,
            workload: PerformanceWorkload::SingleDocument,
            batch_size: 1,
            ocr_used: false,
            runtime_layout_used: false,
            pdf_used: false,
        }
    }

    #[test]
    fn completed_trace_preserves_canonical_stage_order_and_finds_bottleneck() {
        let trace = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 12,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceParse,
                    duration_ms: 90,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Preflight,
                    duration_ms: 40,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Replay,
                    duration_ms: 220,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::PhysicalReadback,
                    duration_ms: 80,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Verify,
                    duration_ms: 25,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Publish,
                    duration_ms: 20,
                },
            ],
            PerformanceOutcome::Completed,
        )
        .unwrap();
        assert_eq!(trace.total_machine_ms, 487);
        let report = trace.slow_run_report();
        assert_eq!(report.bottleneck_stage, Some(PerformanceStage::Replay));
        assert_eq!(report.bottleneck_duration_ms, 220);
    }

    #[test]
    fn arbitrary_text_or_path_cannot_be_used_as_run_id() {
        let mut invalid = context();
        invalid.run_id = r"C:\Users\Иванов\source.docx".into();
        assert!(invalid.validate().is_err());

        invalid.run_id = "Иванов Иван".into();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn duplicate_or_reordered_stages_fail_closed() {
        let duplicate = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 1,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 2,
                },
            ],
            PerformanceOutcome::Attention,
        );
        assert!(duplicate.is_err());

        let reordered = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Preflight,
                    duration_ms: 1,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceParse,
                    duration_ms: 2,
                },
            ],
            PerformanceOutcome::Attention,
        );
        assert!(reordered.is_err());
    }

    #[test]
    fn completed_trace_requires_real_publish_stage() {
        let trace = PerformanceTrace::new(
            context(),
            vec![PerformanceStageMeasurement {
                stage: PerformanceStage::Verify,
                duration_ms: 1,
            }],
            PerformanceOutcome::Completed,
        );
        assert!(trace.is_err());
    }

    #[test]
    fn batch_identity_must_match_run_kind() {
        let mut invalid = context();
        invalid.workload = PerformanceWorkload::Batch10;
        invalid.batch_size = 9;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn tampered_serialized_totals_or_schema_fail_validation() {
        let mut trace = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 1,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Publish,
                    duration_ms: 2,
                },
            ],
            PerformanceOutcome::Completed,
        )
        .unwrap();
        trace.total_machine_ms = 999;
        assert!(trace.validate().is_err());

        trace.total_machine_ms = 3;
        trace.schema = "unknown".into();
        assert!(trace.validate().is_err());
    }

    #[test]
    fn slow_run_report_requires_explicit_positive_threshold() {
        let trace = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 20,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Publish,
                    duration_ms: 30,
                },
            ],
            PerformanceOutcome::Completed,
        )
        .unwrap();
        assert!(trace.slow_run_report_if_over(100).unwrap().is_none());
        assert_eq!(
            trace
                .slow_run_report_if_over(40)
                .unwrap()
                .unwrap()
                .bottleneck_stage,
            Some(PerformanceStage::Publish)
        );
        assert!(trace.slow_run_report_if_over(0).is_err());
    }

    #[test]
    fn failed_or_attention_trace_cannot_claim_publish() {
        for outcome in [
            PerformanceOutcome::Failed,
            PerformanceOutcome::Attention,
            PerformanceOutcome::Cancelled,
        ] {
            assert!(PerformanceTrace::new(
                context(),
                vec![PerformanceStageMeasurement {
                    stage: PerformanceStage::Publish,
                    duration_ms: 1,
                }],
                outcome,
            )
            .is_err());
        }
    }

    #[test]
    fn serialized_trace_has_no_free_form_document_fields() {
        let trace = PerformanceTrace::new(
            context(),
            vec![
                PerformanceStageMeasurement {
                    stage: PerformanceStage::SourceOpen,
                    duration_ms: 1,
                },
                PerformanceStageMeasurement {
                    stage: PerformanceStage::Publish,
                    duration_ms: 2,
                },
            ],
            PerformanceOutcome::Completed,
        )
        .unwrap();
        let json = serde_json::to_value(trace).unwrap();
        let object = json.as_object().unwrap();
        assert_eq!(
            object.keys().cloned().collect::<BTreeSet<_>>(),
            ["context", "outcome", "schema", "stages", "total_machine_ms"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
        let context = object["context"].as_object().unwrap();
        assert_eq!(
            context.keys().cloned().collect::<BTreeSet<_>>(),
            [
                "app_version",
                "batch_size",
                "cache_state",
                "class",
                "ocr_used",
                "pdf_used",
                "run_id",
                "run_phase",
                "runtime_layout_used",
                "workload",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        );
    }
}
