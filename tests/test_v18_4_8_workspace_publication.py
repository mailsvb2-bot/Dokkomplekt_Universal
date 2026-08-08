from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HYGIENE = ROOT / "src-tauri" / "src" / "workspace_hygiene.rs"


def _section(text: str, start: str, end: str) -> str:
    return text.split(start, 1)[1].split(end, 1)[0]


def test_service_file_move_reuses_identity_safe_claim_and_atomic_archive_publish():
    text = HYGIENE.read_text(encoding="utf-8")
    move = _section(text, "fn move_to_unique_folder(", "fn write_receipt(")
    assert "claim_matching_source(source, &hash)" in move
    assert "copy_claim_to_unique_archive(&claim, folder, source)" in move
    assert "fn move_file_safely" not in text
    assert "fs::copy(source, destination)" not in text
    assert "fs::rename(source, destination)" not in text


def test_receipt_is_hidden_until_complete_and_never_overwrites_final_name():
    text = HYGIENE.read_text(encoding="utf-8")
    publisher = _section(text, "fn publish_bytes_create_new(", "fn staging_path(")
    assert "RECEIPT_STAGE_PREFIX" in text
    assert ".create_new(true)" in publisher
    assert "file.sync_all()" in publisher
    assert "staged != bytes" in publisher
    assert "fs::hard_link(&staging, destination)" in publisher
    assert publisher.index("staged != bytes") < publisher.index("fs::hard_link(&staging, destination)")
    assert "published != bytes" in publisher


def test_stale_hidden_staging_is_cleaned_independently_of_archive_retention():
    text = HYGIENE.read_text(encoding="utf-8")
    assert "removed_stale_staging_files" in text
    assert "is_workspace_staging_name(name)" in text
    assert "let retention = (policy.archived_source_retention_days > 0).then" in text
    recursive = _section(text, "fn cleanup_expired_archive_files(", "fn sha256_file(")
    assert "retention: Option<Duration>" in recursive
    assert "FINALIZING_CLAIM_GRACE" in recursive
    assert "removed_stale_staging_files" in recursive
