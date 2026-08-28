from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_publication_completion_is_plan_bound_before_best_effort_metadata() -> None:
    main = (ROOT / "src-tauri/src/main.rs").read_text(encoding="utf-8")
    automation = (ROOT / "src-tauri/src/subsystems/automation_runtime.rs").read_text(encoding="utf-8")
    dedup = (ROOT / "src-tauri/src/subsystems/automation_dedup.rs").read_text(encoding="utf-8")
    publication_evidence = (ROOT / "src-tauri/src/generation_publication.rs").read_text(encoding="utf-8")

    assert "fn local_completion_receipt" in publication_evidence
    assert "fn mark_local_completion" in publication_evidence
    assert "fn local_completion_receipt_matches" in publication_evidence
    assert "fn plan_bound_emergency_completion_exists" in publication_evidence
    assert "fn mark_business_terminal" in main
    assert "self.terminal = true;" in main

    dedup_call = automation.index("automatic_generation_already_processed")
    publish = automation.index("let local_completion = generation_publication::mark_local_completion")
    terminal = automation.index("case_run.mark_business_terminal()")
    case_finish = automation.index('case_run.finish("completed"', terminal)
    corpus = automation.index("if corpus_recording_enabled", case_finish)
    # Filesystem publication is now the in-memory business terminal boundary,
    # while durable deduplication must be checked before any publication work.
    prepare = automation.index("generation_publication::prepare_publication")
    confirm = automation.index("generation_publication::confirm_publication", terminal)
    assert dedup_call < prepare < terminal < confirm < publish < case_finish < corpus
    for guard in (
        "completed_in_history",
        "completed_in_shared_queue",
        "completed_in_local_receipts",
        "completed_in_emergency_marker",
        "completed_in_publication_guard",
    ):
        assert guard in dedup
    assert "if force_reissue" in dedup
    assert "shared_completion_receipt_matches" in dedup
    assert "plan_bound_publication_guard_exists" in dedup
    assert "PublicationPlanBinding" in automation
    assert "complete_publication_receipt" in automation
    assert "local_completion.is_err() && queue_completion.is_err() && case_completion.is_err()" in automation
    assert "fn mark_plan_bound_emergency_guard" in publication_evidence
    assert "published_completion_ledgers_failed" in publication_evidence
    assert "unverified_publication_quarantined" in publication_evidence
    assert "generation_publication::mark_plan_bound_emergency_guard" in automation
    assert "processing_job_sha256" in automation
    assert "publication_completion_metadata" in automation
