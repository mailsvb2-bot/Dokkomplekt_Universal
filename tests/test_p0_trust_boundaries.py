from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_word_scanner_and_printer_reject_active_ooxml_before_opening() -> None:
    commands = text("src-tauri/src/subsystems/document_commands.rs")
    scanner = commands[commands.index("fn start_word_scanner(") : commands.index("fn activate_word_scanner(")]
    assert scanner.index("validate_safe_template_file(&original)") < scanner.index("std::fs::copy(&original")
    assert scanner.index("validate_safe_template_file(&original)") < scanner.index("shell_execute_path(&opened")

    desktop = text("src-tauri/src/subsystems/desktop_io.rs")
    assert "validate_safe_template_file(path)" in desktop
    assert "$word.AutomationSecurity = 3" in desktop
    assert "$word.AutomationSecurity = $previousAutomationSecurity" in desktop


def test_desktop_state_is_loaded_then_applied_and_saved_as_one_snapshot() -> None:
    commands = text("src-tauri/src/subsystems/document_commands.rs")
    loader = commands[commands.index("fn load_state_from(") : commands.index("#[tauri::command]\nfn load_state(")]
    assert "quick_integrity_check" in loader
    assert loader.index("let loaded_case") < loader.index("let mut case_guard")
    assert loader.index("let loaded_pack") < loader.index("let mut pack_guard")
    assert "persistence_blocked.store(false" in loader

    main = text("src-tauri/src/main.rs")
    assert "save_case_and_pack_atomic" in main
    assert "save_desktop_snapshot" in main
    assert "if let Err(error) = load_state_from" in main
    assert "persistence_blocked.store(true" in main


def test_archive_cleanup_and_xlsx_parser_have_resource_boundaries() -> None:
    hygiene = text("src-tauri/src/workspace_hygiene.rs")
    assert "symlink_metadata" in hygiene
    assert "FILE_ATTRIBUTE_REPARSE_POINT" in hygiene
    assert "create_real_directory_below" in hygiene
    assert "value.starts_with(archive_root_canonical)" in hygiene

    intake = text("src-tauri/src/universal_intake.rs")
    for marker in (
        "MAX_XLSX_UNPACKED_BYTES",
        "MAX_XLSX_COLUMNS",
        "MAX_XLSX_CELLS",
        "validate_xlsx_archive",
        "read_text_limited",
        "validate_source_file_size(path)?",
    ):
        assert marker in intake
    assert "letters > 3" in intake
    assert "value > MAX_XLSX_COLUMNS" in intake

    watcher = text("src-tauri/src/subsystems/watcher_commands.rs")
    preflight = watcher.index("universal_intake::validate_source_file_size(&path)")
    hashing = watcher.index("observe_file_stability(&path", preflight)
    assert preflight < hashing


def test_yookassa_callback_is_authenticated_by_provider_api_not_custom_header() -> None:
    provider = text("crates/dokkomplekt-license-server/src/provider_yookassa.rs")
    assert "pub fn verify_callback" in provider
    assert ".get(endpoint)" in provider
    assert ".basic_auth(self.shop_id.trim()" in provider
    assert "webhook data does not match the authenticated YooKassa payment" in provider

    webhooks = text("crates/dokkomplekt-license-server/src/http/webhooks.rs")
    callback = webhooks[webhooks.index("async fn yookassa_callback(") : webhooks.index("async fn record_verified_event(")]
    assert "verify_callback" in callback
    assert "x-dokkomplekt-callback-secret" not in callback
    assert "PAYLOAD_TOO_LARGE" in callback


def test_release_workflows_pin_actions_and_scope_private_keys_to_steps() -> None:
    workflow_paths = sorted((ROOT / ".github/workflows").glob("*.yml"))
    action_ref = re.compile(r"uses:\s+[^\s@]+@([^\s#]+)")
    for workflow_path in workflow_paths:
        workflow = workflow_path.read_text(encoding="utf-8")
        refs = action_ref.findall(workflow)
        assert refs, workflow_path
        assert all(re.fullmatch(r"[0-9a-f]{40}", ref) for ref in refs), (workflow_path, refs)

    release = text(".github/workflows/build-installers.yml")
    for required in (
        "DOKKOMPLEKT_LICENSE_PUBKEY_B64",
        "DOKKOMPLEKT_UPDATE_MANIFEST_URL",
        "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
        "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64",
        "DOKKOMPLEKT_REFDATA_PUBKEY_B64",
        "--mode production-build",
        "verify_windows_hosted_signing_runner.py",
        "fetch_hosted_runtime_bundle.py",
        "stage_signed_runtime_bundle.py",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL",
    ):
        assert required in release
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-runtime]" not in release
    assert "runs-on: windows-latest" in release
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" not in release

    # No private signing value may live in a job-level env block. It must appear
    # beneath a concrete step, after pinned setup actions have completed.
    for secret in (
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert not re.search(rf"^      {re.escape(secret)}:", release, re.MULTILINE)
        assert re.search(rf"^          {re.escape(secret)}:", release, re.MULTILINE)
