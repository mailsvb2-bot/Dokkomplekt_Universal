from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_trust_report_is_opt_in_for_new_and_legacy_installations() -> None:
    privacy = (ROOT / "src-tauri/src/privacy_runtime.rs").read_text(encoding="utf-8")
    assert "write_trust_report: false" in privacy
    assert "trust_report_explicit: false" in privacy
    assert "if !preferences.trust_report_explicit" in privacy
    assert "preferences.write_trust_report = false" in privacy
    assert "persisted.trust_report_explicit = true" in privacy


def test_manual_docx_generation_cannot_enter_trust_report_path_unless_opted_in() -> None:
    document_commands = (
        ROOT / "src-tauri/src/subsystems/document_commands.rs"
    ).read_text(encoding="utf-8")
    batch_start = document_commands.index("fn render_docx_batch(")
    batch_end = document_commands.index("struct ScannerRequest", batch_start)
    batch = document_commands[batch_start:batch_end]
    assert "if privacy.write_trust_report" in batch


def test_migration_keeps_future_explicit_opt_in_available() -> None:
    privacy = (ROOT / "src-tauri/src/privacy_runtime.rs").read_text(encoding="utf-8")
    assert "explicit_trust_report_choice_is_preserved" in privacy
    assert "legacy_implicit_trust_report_is_migrated_off" in privacy
