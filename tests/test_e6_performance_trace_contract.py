from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_performance_trace_is_closed_privacy_safe_and_bounded() -> None:
    core = read("crates/dokkomplekt-core/src/performance_trace.rs")
    runtime = read("src-tauri/src/performance_trace_runtime.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")
    workspace = read("src-tauri/src/universal_intake/workspace_session.rs")
    documents = read("src-tauri/src/subsystems/document_commands.rs")
    publication = read("src-tauri/src/subsystems/publication_collision.rs")

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
    context_fields = "\n".join(
        line.strip() for line in context.splitlines() if line.strip().startswith("pub ")
    ).lower()
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
        assert forbidden not in context_fields

    assert "valid_opaque_identifier" in core
    assert "performance stages must follow canonical order" in core
    assert "duplicate performance stage" in core
    assert "total_machine_ms does not match stage durations" in core
    assert "PerformanceRunPhase" in core
    assert "PerformanceWorkload" in core
    assert "end_to_end_ms" in core
    assert "human_wait_ms" in core
    assert "performance human_wait_ms cannot exceed end_to_end_ms" in core
    assert "stage machine time cannot exceed end-to-end time minus human wait" in core
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
    assert "create_sensitive_session_with_lease" in workspace
    assert "session_has_verified_ownership" in workspace
    assert "filter(|session| !session.live)" in workspace
    assert "retained_file_listing_never_exposes_live_cross_process_session" in workspace
    assert "list_owned_workspace_files" in workspace
    assert "metadata_is_link_like" in workspace

    assert '"performance-traces"' in privacy
    assert '"Performance traces (без содержимого документов)"' in privacy
    assert "PERFORMANCE_TRACE_QUOTA_BYTES" in privacy
    assert "PERFORMANCE_TRACE_RETENTION_SECONDS" in privacy
    assert "owned_performance_trace_bytes" in privacy
    assert "cleanup_performance_traces" in privacy

    assert "ensure_rendered_document_readable" in publication
    assert "ensure_rendered_document_semantically_complete" in publication
    manual_single = documents[
        documents.index("fn render_docx("):
        documents.index("enum ExistingOutputPolicy")
    ]
    for stage in (
        "PerformanceStage::ReferenceClone",
        "PerformanceStage::Replay",
        "PerformanceStage::PhysicalReadback",
        "PerformanceStage::Verify",
        "PerformanceStage::Publish",
    ):
        assert stage in manual_single
    assert "persist_manual_single_document_trace" in manual_single
    assert "performance_trace_write_failures" in manual_single
    assert manual_single.index("PerformanceStage::PhysicalReadback") < manual_single.index(
        "PerformanceStage::Verify"
    )
    assert manual_single.index("PerformanceStage::Verify") < manual_single.index(
        "PerformanceStage::Publish"
    )


    assert "persist_manual_batch_trace" in runtime
    assert "PerformanceWorkload::Batch10" in runtime
    assert "PerformanceWorkload::Batch50" in runtime
    assert "PerformanceWorkload::OtherBatch" in runtime

    manual_batch = documents[
        documents.index("fn render_docx_batch("):
        documents.index("struct ScannerRequest")
    ]
    for stage in (
        "PerformanceStage::ReferenceClone",
        "PerformanceStage::Replay",
        "PerformanceStage::Verify",
        "PerformanceStage::Publish",
    ):
        assert stage in manual_batch
    assert "persist_manual_batch_trace" in manual_batch
    assert "performance_trace_write_failures" in manual_batch
    assert manual_batch.index("PerformanceStage::ReferenceClone") < manual_batch.index(
        "PerformanceStage::Replay"
    )
    assert manual_batch.index("PerformanceStage::Replay") < manual_batch.index(
        "PerformanceStage::Verify"
    )
    assert manual_batch.index("PerformanceStage::Verify") < manual_batch.index(
        "PerformanceStage::Publish"
    )
