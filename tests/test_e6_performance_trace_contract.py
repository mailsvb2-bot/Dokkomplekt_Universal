from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_performance_trace_is_closed_privacy_safe_and_bounded() -> None:
    core = read("crates/dokkomplekt-core/src/performance_trace.rs")
    runtime = read("src-tauri/src/performance_trace_runtime.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")
    workspace = read("src-tauri/src/universal_intake/workspace_session.rs")

    for stage in (
        "source_open",
        "source_parse",
        "candidate_index",
        "source_resolve",
        "prompt_plan",
        "preflight",
        "reference_clone",
        "replay",
        "physical_readback",
        "verify",
        "publish",
        "recovery",
    ):
        assert f'"{stage}"' in core

    context = core[
        core.index("pub struct PerformanceTraceContext"):
        core.index("pub struct PerformanceTrace", core.index("pub struct PerformanceTraceContext") + 1)
    ]
    for required in (
        "run_id",
        "app_version",
        "class",
        "cache_state",
        "run_phase",
        "workload",
        "batch_size",
        "ocr_used",
        "runtime_layout_used",
        "pdf_used",
    ):
        assert required in context
    for forbidden in (
        "path",
        "source_text",
        "raw_text",
        "snippet",
        "patient",
        "diagnosis",
        "fio",
        "name:",
        "address",
    ):
        assert forbidden not in context.lower()

    assert "valid_opaque_identifier" in core
    assert "performance stages must follow canonical order" in core
    assert "duplicate performance stage" in core
    assert "total_machine_ms does not match stage durations" in core
    assert "PerformanceRunPhase" in core
    assert "PerformanceWorkload" in core
    assert "slow_run_report_if_over" in core
    assert "bottleneck_stage" in core

    assert "PERFORMANCE_TRACE_QUOTA_BYTES: u64 = 16 * 1024 * 1024" in runtime
    assert "PERFORMANCE_TRACE_RETENTION_SECONDS: u64 = 14 * 24 * 60 * 60" in runtime
    assert "PERFORMANCE_TRACE_MAX_ENTRY_BYTES: usize = 128 * 1024" in runtime
    assert "enforce_ephemeral_workspace_quota" in runtime
    assert "create_completed_retained_workspace_file" in runtime
    assert "list_owned_workspace_files" in runtime
    assert "trace.validate()?" in runtime
    assert "PERFORMANCE_TRACE_LOCK" in runtime

    assert "create_completed_retained_workspace_file" in workspace
    assert "session_has_verified_ownership" in workspace
    assert "list_owned_workspace_files" in workspace
    assert "metadata_is_link_like" in workspace

    assert '"performance-traces"' in privacy
    assert '"Performance traces (без содержимого документов)"' in privacy
    assert "PERFORMANCE_TRACE_QUOTA_BYTES" in privacy
    assert "PERFORMANCE_TRACE_RETENTION_SECONDS" in privacy
    assert "owned_performance_trace_bytes" in privacy
    assert "cleanup_performance_traces" in privacy
