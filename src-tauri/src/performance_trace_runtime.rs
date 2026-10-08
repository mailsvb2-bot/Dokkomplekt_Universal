use dokkomplekt_core::{
    PerformanceCacheState, PerformanceClass, PerformanceOutcome, PerformanceRunPhase,
    PerformanceStage, PerformanceStageMeasurement, PerformanceTrace, PerformanceTraceContext,
    PerformanceWorkload,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager as _;

const PERFORMANCE_TRACE_WORKSPACE: &str = "performance-traces";
const PERFORMANCE_TRACE_FILE: &str = "trace.json";
pub(crate) const PERFORMANCE_TRACE_QUOTA_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const PERFORMANCE_TRACE_RETENTION_SECONDS: u64 = 14 * 24 * 60 * 60;
const PERFORMANCE_TRACE_MAX_ENTRY_BYTES: usize = 128 * 1024;
const PERFORMANCE_TRACE_RESERVE_BYTES: u64 = 4 * 1024;
static PERFORMANCE_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn performance_trace_workspace(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(PERFORMANCE_TRACE_WORKSPACE))
        .map_err(|error| format!("Каталог performance trace недоступен: {error}"))
}

pub(crate) fn elapsed_milliseconds(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn automatic_source_uses_ocr(
    normalized: &crate::universal_intake::NormalizedSource,
) -> bool {
    normalized.ocr_used
}

pub(crate) fn automatic_source_uses_pdf(
    normalized: &crate::universal_intake::NormalizedSource,
) -> bool {
    if normalized.source_kind.to_ascii_lowercase().contains("pdf")
        || normalized.processed_files.iter().any(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
    {
        return true;
    }

    // EML/MSG keep source_kind="email" and only retain the top-level message
    // in processed_files. Nested attachment identity survives in layout evidence.
    normalized.layout_items.iter().any(|item| {
        item.source_reference.as_deref().is_some_and(|reference| {
            reference.split(';').any(|segment| {
                segment
                    .rsplit(['/', '\\', ':'])
                    .next()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with(".pdf"))
            })
        })
    })
}

fn automatic_runtime_cache_state() -> PerformanceCacheState {
    // Ordinary production journeys do not prove benchmark cache preparation.
    // Cold/warm cache labels require a dedicated benchmark harness that owns
    // reset/priming and can bind that lifecycle to the resulting trace.
    PerformanceCacheState::Unclassified
}

fn automatic_runtime_run_phase() -> PerformanceRunPhase {
    // Resume/reissue/reused-document semantics are business workflow state,
    // not Canon E6 benchmark first/repeat phases. Keep this fail-closed until
    // a benchmark harness proves the actual measurement phase.
    PerformanceRunPhase::Unclassified
}

fn performance_workload(batch_size: u32) -> PerformanceWorkload {
    match batch_size {
        1 => PerformanceWorkload::SingleDocument,
        10 => PerformanceWorkload::Batch10,
        50 => PerformanceWorkload::Batch50,
        _ => PerformanceWorkload::OtherBatch,
    }
}

fn persist_manual_document_trace(
    app: &tauri::AppHandle,
    workload: PerformanceWorkload,
    batch_size: u32,
    stages: Vec<PerformanceStageMeasurement>,
    end_to_end_ms: u64,
) -> Result<PathBuf, String> {
    let trace = PerformanceTrace::new(
        PerformanceTraceContext {
            run_id: uuid::Uuid::new_v4().simple().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source_sha256: None,
            class: PerformanceClass::Unclassified,
            cache_state: PerformanceCacheState::Unclassified,
            run_phase: PerformanceRunPhase::Unclassified,
            workload,
            batch_size,
            // These features are not invoked inside the manual render commands.
            // Upstream source-analysis choices are outside this command's click→ready interval.
            ocr_used: false,
            runtime_layout_used: false,
            pdf_used: false,
        },
        stages,
        end_to_end_ms,
        0,
        PerformanceOutcome::Completed,
    )?;
    persist_performance_trace(app, &trace)
}

pub(crate) fn persist_manual_single_document_trace(
    app: &tauri::AppHandle,
    stages: Vec<PerformanceStageMeasurement>,
    end_to_end_ms: u64,
) -> Result<PathBuf, String> {
    persist_manual_document_trace(
        app,
        PerformanceWorkload::SingleDocument,
        1,
        stages,
        end_to_end_ms,
    )
}

pub(crate) fn persist_manual_batch_trace(
    app: &tauri::AppHandle,
    batch_size: u32,
    stages: Vec<PerformanceStageMeasurement>,
    end_to_end_ms: u64,
) -> Result<PathBuf, String> {
    persist_manual_document_trace(
        app,
        performance_workload(batch_size),
        batch_size,
        stages,
        end_to_end_ms,
    )
}

pub(crate) struct AutomaticPerformanceTraceContext<'a> {
    pub source_sha256: &'a str,
    pub ocr_used: bool,
    pub pdf_used: bool,
}

pub(crate) fn persist_automatic_trace(
    app: &tauri::AppHandle,
    batch_size: u32,
    stages: Vec<PerformanceStageMeasurement>,
    end_to_end_ms: u64,
    context: AutomaticPerformanceTraceContext<'_>,
) -> Result<PathBuf, String> {
    let trace = PerformanceTrace::new(
        PerformanceTraceContext {
            run_id: uuid::Uuid::new_v4().simple().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source_sha256: Some(context.source_sha256.to_string()),
            class: PerformanceClass::Unclassified,
            cache_state: automatic_runtime_cache_state(),
            run_phase: automatic_runtime_run_phase(),
            workload: performance_workload(batch_size),
            batch_size,
            ocr_used: context.ocr_used,
            // Automatic template replay uses the existing user template layout.
            // Runtime-layout synthesis is a different path and is not invoked here.
            runtime_layout_used: false,
            pdf_used: context.pdf_used,
        },
        stages,
        end_to_end_ms,
        0,
        PerformanceOutcome::Completed,
    )?;
    persist_performance_trace(app, &trace)
}

pub(crate) fn persist_failed_recovery_trace(
    app: &tauri::AppHandle,
    batch_size: u32,
    recovery_ms: u64,
) -> Result<PathBuf, String> {
    // This trace is intentionally recovery-local. A failed run must never claim
    // the canonical Publish stage, and no error text, paths, source data, or
    // document values are accepted by this interface.
    let trace = PerformanceTrace::new(
        PerformanceTraceContext {
            run_id: uuid::Uuid::new_v4().simple().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source_sha256: None,
            class: PerformanceClass::Unclassified,
            cache_state: PerformanceCacheState::Unclassified,
            run_phase: PerformanceRunPhase::Unclassified,
            workload: performance_workload(batch_size),
            batch_size,
            ocr_used: false,
            runtime_layout_used: false,
            pdf_used: false,
        },
        vec![PerformanceStageMeasurement {
            stage: PerformanceStage::Recovery,
            duration_ms: recovery_ms,
        }],
        recovery_ms,
        0,
        PerformanceOutcome::Failed,
    )?;
    persist_performance_trace(app, &trace)
}

pub(crate) fn persist_performance_trace(
    app: &tauri::AppHandle,
    trace: &PerformanceTrace,
) -> Result<PathBuf, String> {
    trace.validate()?;
    let bytes = serde_json::to_vec(trace)
        .map_err(|error| format!("Не удалось сериализовать performance trace: {error}"))?;
    if bytes.len() > PERFORMANCE_TRACE_MAX_ENTRY_BYTES {
        return Err(format!(
            "Performance trace превышает безопасный размер {} байт.",
            PERFORMANCE_TRACE_MAX_ENTRY_BYTES
        ));
    }
    let required_bytes = (bytes.len() as u64).saturating_add(PERFORMANCE_TRACE_RESERVE_BYTES);
    let workspace = performance_trace_workspace(app)?;
    let _guard = PERFORMANCE_TRACE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "performance trace lock failed".to_string())?;

    crate::universal_intake::enforce_ephemeral_workspace_quota(
        &workspace,
        Duration::from_secs(PERFORMANCE_TRACE_RETENTION_SECONDS),
        PERFORMANCE_TRACE_QUOTA_BYTES,
        required_bytes,
    )?;
    let path = crate::universal_intake::create_completed_retained_workspace_file(
        &workspace,
        PERFORMANCE_TRACE_FILE,
        &bytes,
    )?;
    // Re-apply the quota after the exact on-disk size exists. This catches marker
    // overhead and keeps concurrent admission + write atomic under the same lock.
    crate::universal_intake::enforce_ephemeral_workspace_quota(
        &workspace,
        Duration::from_secs(PERFORMANCE_TRACE_RETENTION_SECONDS),
        PERFORMANCE_TRACE_QUOTA_BYTES,
        0,
    )?;
    Ok(path)
}

pub(crate) fn recent_performance_traces(
    app: &tauri::AppHandle,
    limit: usize,
) -> Result<Vec<PerformanceTrace>, String> {
    let workspace = performance_trace_workspace(app)?;
    let _guard = PERFORMANCE_TRACE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "performance trace lock failed".to_string())?;
    crate::universal_intake::enforce_ephemeral_workspace_quota(
        &workspace,
        Duration::from_secs(PERFORMANCE_TRACE_RETENTION_SECONDS),
        PERFORMANCE_TRACE_QUOTA_BYTES,
        0,
    )?;
    let paths = crate::universal_intake::list_owned_workspace_files(
        &workspace,
        PERFORMANCE_TRACE_FILE,
        limit,
    )?;
    let mut traces = Vec::with_capacity(paths.len());
    for path in paths {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("Не удалось проверить performance trace: {error}"))?;
        if metadata.len() > PERFORMANCE_TRACE_MAX_ENTRY_BYTES as u64 {
            return Err(format!(
                "Сохранённый performance trace превышает безопасный размер: {}",
                path.display()
            ));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("Не удалось прочитать performance trace: {error}"))?;
        let trace: PerformanceTrace = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Повреждён performance trace: {error}"))?;
        trace.validate()?;
        traces.push(trace);
    }
    Ok(traces)
}

pub(crate) fn owned_performance_trace_bytes(app: &tauri::AppHandle) -> Result<u64, String> {
    let workspace = performance_trace_workspace(app)?;
    let _guard = PERFORMANCE_TRACE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "performance trace lock failed".to_string())?;
    crate::universal_intake::owned_workspace_bytes(&workspace)
}

pub(crate) fn cleanup_performance_traces(app: &tauri::AppHandle) -> Result<usize, String> {
    let workspace = performance_trace_workspace(app)?;
    let _guard = PERFORMANCE_TRACE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "performance trace lock failed".to_string())?;
    crate::universal_intake::enforce_ephemeral_workspace_quota(
        &workspace,
        Duration::from_secs(PERFORMANCE_TRACE_RETENTION_SECONDS),
        PERFORMANCE_TRACE_QUOTA_BYTES,
        0,
    )
}

#[cfg(test)]
mod performance_e6_provenance_tests {
    use super::*;

    #[test]
    fn automatic_ocr_provenance_uses_canonical_source_kind_only() {
        let make =
            |source_kind: &str, warnings: Vec<String>| crate::universal_intake::NormalizedSource {
                text: "fixture".into(),
                source_kind: source_kind.into(),
                ocr_used: matches!(
                    source_kind,
                    "scanned_image" | "scanned_pdf_ocr" | "mixed_pdf_page_ocr"
                ),
                warnings,
                processed_files: vec![PathBuf::from("fixture.txt")],
                layout_items: Vec::new(),
            };

        for source_kind in ["scanned_image", "scanned_pdf_ocr", "mixed_pdf_page_ocr"] {
            assert!(automatic_source_uses_ocr(&make(source_kind, Vec::new())));
        }
        assert!(!automatic_source_uses_ocr(&make(
            "pdf_text",
            vec!["OCR is mentioned in an unrelated warning".into()],
        )));
        let nested_container = crate::universal_intake::NormalizedSource {
            text: "fixture".into(),
            source_kind: "archive".into(),
            ocr_used: true,
            warnings: Vec::new(),
            processed_files: vec![PathBuf::from("fixture.zip")],
            layout_items: Vec::new(),
        };
        assert!(automatic_source_uses_ocr(&nested_container));
        assert!(!automatic_source_uses_ocr(&make("word", Vec::new())));
    }

    #[test]
    fn ordinary_automatic_runtime_does_not_claim_benchmark_cache_or_phase() {
        assert_eq!(
            automatic_runtime_cache_state(),
            PerformanceCacheState::Unclassified
        );
        assert_eq!(
            automatic_runtime_run_phase(),
            PerformanceRunPhase::Unclassified
        );
    }
}
