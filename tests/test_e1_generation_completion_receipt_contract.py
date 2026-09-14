from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PUBLICATION = ROOT / "src-tauri" / "src" / "generation_publication.rs"
DOCUMENT_COMMANDS = ROOT / "src-tauri" / "src" / "subsystems" / "document_commands.rs"
MANUAL_IDENTITY = ROOT / "src-tauri" / "src" / "subsystems" / "manual_publication_identity.rs"


def test_committed_generation_receipt_is_distinct_from_recovery_journal_and_non_pii() -> None:
    source = PUBLICATION.read_text(encoding="utf-8")
    start = source.index("struct GenerationCompletionReceipt")
    end = source.index("struct PublicationRecoveryContext", start)
    receipt = source[start:end]

    assert 'COMPLETION_RECEIPT_DIR: &str = "generation-completion-receipts"' in source
    assert "output_sha256" in receipt
    assert "output_id" in receipt
    assert "receipt_id" in receipt
    assert "status" in receipt
    assert "proof_contract" in receipt
    assert "verifier_contract" in receipt
    assert "plan_binding_sha256" in receipt
    for forbidden in (
        "reservation_id",
        "stage_location",
        "replacement_target",
        "replacement_backup",
        "patient",
        "fio",
        "output_path",
        "source_path",
    ):
        assert forbidden not in receipt


def test_committed_generation_receipt_replay_is_idempotent_and_conflicts_fail_closed() -> None:
    source = PUBLICATION.read_text(encoding="utf-8")
    start = source.index("fn persist_generation_completion_receipt")
    end = source.index("fn write_receipt", start)
    persist = source[start:end]

    assert "match std::fs::symlink_metadata(&path)" in persist
    assert "publication_metadata_is_link_or_reparse" in persist
    assert "!metadata.is_file()" in persist
    assert "ErrorKind::NotFound" in persist
    assert "completion_receipts_match_identity" in persist
    assert "return Ok(path);" in persist
    assert "Конфликт committed GenerationReceipt" in persist
    assert persist.index("match std::fs::symlink_metadata(&path)") < persist.index("crate::atomic_write_file")
    assert persist.index("completion_receipts_match_identity") < persist.index("crate::atomic_write_file")


def test_ui_success_boundary_requires_committed_generation_receipt_before_journal_cleanup() -> None:
    source = PUBLICATION.read_text(encoding="utf-8")
    confirm_start = source.index("pub(crate) fn confirm_publication")
    confirm_end = source.index("pub(crate) fn abort_prepared_publication", confirm_start)
    confirm = source[confirm_start:confirm_end]
    assert 'write_receipt(&path, &receipt)' in confirm
    assert 'persist_generation_completion_receipt(&app_data, &receipt)?' in confirm
    assert confirm.index('write_receipt(&path, &receipt)') < confirm.index(
        'persist_generation_completion_receipt(&app_data, &receipt)?'
    )

    finalize_start = source.index("pub(crate) fn finalize_published_generation")
    finalize_end = source.index("pub(crate) fn plan_bound_publication_guard_exists", finalize_start)
    finalize = source[finalize_start:finalize_end]
    assert finalize.index("persist_generation_completion_receipt") < finalize.index(
        "remove_publication_receipt"
    )

    commands = DOCUMENT_COMMANDS.read_text(encoding="utf-8")
    single = commands[commands.index("fn render_docx("):commands.index("fn render_docx_batch(")]
    assert single.index("generation_publication::confirm_publication") < single.index(
        "generation_publication::finalize_published_generation"
    )

def test_crash_reconciliation_rebuilds_committed_receipt_before_removing_guard() -> None:
    source = PUBLICATION.read_text(encoding="utf-8")
    start = source.index("pub(crate) fn reconcile_publication_receipts")
    end = source.index("fn recover_stale_prepublication_reservations", start)
    recovery = source[start:end]

    assert "persist_generation_completion_receipt(app_data, &receipt)" in recovery
    assert recovery.index("persist_generation_completion_receipt(app_data, &receipt)") < recovery.index(
        "std::fs::remove_file(path)"
    )
    assert "publication guard сохранён" in recovery


def test_manual_ui_publication_is_bound_to_frozen_source_case_plan_and_templates() -> None:
    commands = DOCUMENT_COMMANDS.read_text(encoding="utf-8")
    helper = MANUAL_IDENTITY.read_text(encoding="utf-8")
    single = commands[commands.index("fn render_docx("):commands.index("fn render_docx_batch(")]
    batch = commands[commands.index("fn render_docx_batch("):commands.index("struct ScannerRequest")]

    assert "manual_publication_plan_binding" in single
    assert "Some(&publication_binding)" in single
    assert "manual_publication_plan_binding" in batch
    assert "Some(&publication_binding)" in batch
    assert "template_sha256" in single
    assert "template_sha256" in batch
    assert "render_inputs" in batch
    assert "publication_plan" in batch
    assert "dokkomplekt-manual-source-set-v1" in helper
    assert "dokkomplekt-manual-resolved-case-v1" in helper
    assert "dokkomplekt-manual-processing-fingerprint-v1" in helper
    assert "dokkomplekt-manual-processing-job-v1" in helper
    assert "source_provenance" in helper
