from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "src-tauri" / "src" / "subsystems" / "automation_runtime.rs"
HYGIENE = ROOT / "src-tauri" / "src" / "workspace_hygiene.rs"
INTAKE = ROOT / "src-tauri" / "src" / "universal_intake.rs"
DOCS = ROOT / "docs" / "CRASH_CONSISTENCY.md"


def _finalizer() -> str:
    text = RUNTIME.read_text(encoding="utf-8")
    return text.split("fn finalize_processed_source(", 1)[1].split(
        "fn automation_plan_fingerprint", 1
    )[0]


def test_finalizer_has_no_path_based_delete_after_hash_precheck() -> None:
    body = _finalizer()
    assert "std::fs::remove_file(source)" not in body
    assert "delete_processed_source_if_matches(source, source_sha256)" in body
    assert "archive_processed_source(" in body


def test_hygiene_claims_path_before_post_claim_hash_and_destructive_actions() -> None:
    text = HYGIENE.read_text(encoding="utf-8")
    claim = text.split("fn claim_matching_source(", 1)[1].split(
        "fn copy_claim_to_unique_archive", 1
    )[0]
    assert "fs::rename(source, &claim_path)" in claim
    assert claim.index("fs::rename(source, &claim_path)") < claim.index("sha256_file(&claim_path)")
    assert "recover_finalizing_claim(&claim_path)" in claim
    delete = text.split("pub fn delete_processed_source_if_matches(", 1)[1].split(
        "pub fn cleanup_workspace_folder", 1
    )[0]
    assert "claim_matching_source(source, source_sha256)" in delete
    assert "fs::remove_file(&claim.path)" in delete


def test_archive_receipt_uses_verified_archived_bytes_not_caller_metadata() -> None:
    text = HYGIENE.read_text(encoding="utf-8")
    archive = text.split("pub fn archive_processed_source(", 1)[1].split(
        "pub fn delete_processed_source_if_matches", 1
    )[0]
    assert "let archived_sha256 = sha256_file(&destination)?;" in archive
    assert "write_receipt(&receipt, source, &destination, &archived_sha256)" in archive
    assert "create_new(true)" in text
    assert ".dokkomplekt-archive-stage-" in text
    assert "fs::hard_link(&staging, &destination)" in text
    assert text.index("staged_sha256") < text.index("fs::hard_link(&staging, &destination)")


def test_pending_claim_is_not_intakeable_and_stale_claims_are_recovered() -> None:
    hygiene = HYGIENE.read_text(encoding="utf-8")
    intake = INTAKE.read_text(encoding="utf-8")
    assert 'const FINALIZING_SUFFIX: &str = ".pending";' in hygiene
    assert 'name.contains(".dokkomplekt-finalizing-")' in intake
    assert "recovered_finalizing_sources" in hygiene
    assert "finalizing_claim_is_stale" in hygiene
    assert "recover_finalizing_claim(&path)" in hygiene
    assert ".dokkomplekt-recovery-stage-" in hygiene


def test_crash_consistency_contract_documents_identity_claim() -> None:
    docs = DOCS.read_text(encoding="utf-8")
    assert "atomically renames the live pathname" in docs
    assert "verified claim rather than the reusable live pathname" in docs
    assert "stale `.pending` claims" in docs
