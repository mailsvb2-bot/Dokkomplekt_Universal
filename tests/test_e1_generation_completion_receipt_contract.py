from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PUBLICATION = ROOT / "src-tauri" / "src" / "generation_publication.rs"
DOCUMENT_COMMANDS = ROOT / "src-tauri" / "src" / "subsystems" / "document_commands.rs"


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

    assert "if path.exists()" in persist
    assert "completion_receipts_match_identity" in persist
    assert "return Ok(path);" in persist
    assert "Конфликт committed GenerationReceipt" in persist
    assert persist.index("if path.exists()") < persist.index("crate::atomic_write_file")


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
