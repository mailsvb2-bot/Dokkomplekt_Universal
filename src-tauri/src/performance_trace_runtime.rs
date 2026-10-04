use dokkomplekt_core::{
    PerformanceCacheState, PerformanceClass, PerformanceOutcome, PerformanceRunPhase,
    PerformanceStageMeasurement, PerformanceTrace, PerformanceTraceContext, PerformanceWorkload,
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

pub(crate) fn persist_manual_single_document_trace(
    app: &tauri::AppHandle,
    stages: Vec<PerformanceStageMeasurement>,
    end_to_end_ms: u64,
) -> Result<PathBuf, String> {
    let trace = PerformanceTrace::new(
        PerformanceTraceContext {
            run_id: uuid::Uuid::new_v4().simple().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            class: PerformanceClass::Unclassified,
            cache_state: PerformanceCacheState::Unclassified,
            run_phase: PerformanceRunPhase::Unclassified,
            workload: PerformanceWorkload::SingleDocument,
            batch_size: 1,
            // These features are not invoked inside the manual render command.
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
