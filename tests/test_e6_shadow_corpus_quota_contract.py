from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_zero_touch_shadow_corpus_is_bounded_without_deleting_confirmed_corpus() -> None:
    storage = read("crates/dokkomplekt-storage/src/lib.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")

    assert "ZERO_TOUCH_SHADOW_CORPUS_QUOTA_BYTES: u64 = 100 * 1024 * 1024" in storage
    assert "ZERO_TOUCH_SHADOW_CORPUS_MAX_ENTRIES: u64 = 50_000" in storage
    assert "ZERO_TOUCH_SHADOW_CORPUS_RETENTION_DAYS: i64 = 90" in storage
    assert 'CORPUS_CLASS_ZERO_TOUCH_SHADOW: &str = "zero_touch_shadow"' in storage
    assert 'CORPUS_CLASS_SPECIALIST_CONFIRMED: &str = "specialist_confirmed"' in storage
    assert 'CORPUS_CLASS_LEGACY_UNVERIFIED: &str = "legacy_unverified"' in storage

    classification = storage[
        storage.index("fn corpus_retention_class"):
        storage.index("#[derive(Debug, Error)]")
    ]
    assert classification.index("SpecialistConfirmed") < classification.index("ZeroTouchShadow")

    append_block = storage[
        storage.index("fn append_corpus_entry_with_shadow_policy"):
        storage.index("pub fn zero_touch_shadow_corpus_status")
    ]
    assert append_block.index("enforce_zero_touch_shadow_policy_in_transaction") < append_block.index(
        'transaction.execute(\n            "INSERT INTO corpus_entries'
    )
    assert "TransactionBehavior::Immediate" in append_block
    assert "let now = chrono::Utc::now();" in append_block
    assert "transaction.commit()?" in append_block

    policy_block = storage[
        storage.index("fn enforce_zero_touch_shadow_policy_in_transaction"):
        storage.index("pub fn list_corpus_entries")
    ]
    assert "WHERE retention_class=?1" in policy_block
    assert "CORPUS_CLASS_ZERO_TOUCH_SHADOW" in policy_block
    assert "without deleting protected corpus data" in policy_block
    assert "CORPUS_CLASS_SPECIALIST_CONFIRMED" not in policy_block

    backfill = storage[
        storage.index("fn backfill_corpus_retention_metadata"):
        storage.index("fn encoded_workspace_profile")
    ]
    assert "CORPUS_CLASS_LEGACY_UNVERIFIED" in backfill
    assert ".decode_sensitive(&stored)" in backfill

    for regression in [
        "zero_touch_shadow_quota_evicts_oldest_without_touching_specialist_corpus",
        "zero_touch_shadow_retention_removes_expired_shadow_only",
        "zero_touch_shadow_byte_quota_evicts_shadow_instead_of_protected_corpus",
        "legacy_corpus_rows_are_backfilled_into_safe_retention_classes",
        "encrypted_legacy_corpus_rows_are_backfilled_without_losing_acceptance_source",
        "corrupt_legacy_corpus_row_is_preserved_as_unverified_instead_of_blocking_open",
    ]:
        assert regression in storage

    assert '"zero-touch-shadow-corpus"' in privacy
    assert "до 50 000 записей" in privacy
    assert "ZERO_TOUCH_SHADOW_CORPUS_QUOTA_BYTES" in privacy
    assert "ZERO_TOUCH_SHADOW_CORPUS_RETENTION_DAYS" in privacy
    assert ".zero_touch_shadow_corpus_status()" in privacy

    cleanup = privacy[
        privacy.index("pub(crate) fn cleanup_intake_workspace"):
        privacy.index("pub(crate) fn start_periodic_intake_cleanup")
    ]
    assert ".enforce_zero_touch_shadow_corpus_policy()" in cleanup
    assert ".dokkomplekt-backups" not in cleanup
    assert "update-backups" not in cleanup
