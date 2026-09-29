from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src-tauri" / "src" / "main.rs"
COMMANDS = ROOT / "src-tauri" / "src" / "subsystems" / "document_commands.rs"
INSTALLER = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
WORKFLOW = ROOT / ".github" / "workflows" / "quality-gate.yml"
ISSUER = ROOT / "crates" / "dokkomplekt-license-server" / "src" / "bin" / "issue_e2e_license.rs"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr18_installed_proof_reuses_canonical_license_path() -> None:
    main = MAIN.read_text(encoding="utf-8")
    commands = COMMANDS.read_text(encoding="utf-8")
    assert "--e2e-license-proof=" in main
    assert "--e2e-license-access-proof" in main
    assert "run_fpr18_license_e2e(&handle, &state" in main
    assert "fn persist_verified_license_text(" in commands
    assert "verify_license_document_now(&document, &public_key)" in commands
    assert "transact_default_state(app, state" in commands
    assert "run_fpr18_license_e2e(" in commands
    assert 'decision.mode != "paid"' in commands


def test_fpr18_fixture_uses_existing_rust_issuer_and_ephemeral_key_cleanup() -> None:
    issuer = ISSUER.read_text(encoding="utf-8")
    workflow = WORKFLOW.read_text(encoding="utf-8")
    assert '#[path = "../issuer.rs"]' in issuer
    assert "issue_license(" in issuer
    assert 'plan: PlanId::DoctorPro' in issuer
    assert "allowed_machines: vec![]" in issuer
    assert "python -m pip install --disable-pip-version-check -r requirements-dev.txt" in workflow
    assert "generate_license_keypair.py --json-output" in workflow
    assert "--manifest-path crates/dokkomplekt-license-server/Cargo.toml --bin issue_e2e_license" in workflow
    assert "FPR-18 ephemeral private key cleanup failed." in workflow
    assert "Remove-Item -LiteralPath $privateKey, $keypair" in workflow


def test_fpr18_installed_positive_negative_and_restart_are_fail_closed() -> None:
    script = INSTALLER.read_text(encoding="utf-8-sig")
    for marker in (
        "FPR-18 POSITIVE INSTALLED PASS:",
        "FPR-18 NEGATIVE INSTALLED PASS:",
        "FPR-18 RESTART INSTALLED PASS:",
        "tampered license was accepted",
        "tampered license produced success evidence",
        "--e2e-license-access-proof",
    ):
        assert marker in script
    assert "$tamperedProcess.ExitCode -eq 0" in script
    assert "$restartRecord.mode -ne 'paid'" in script
    assert "$fpr18PreviousHardwareE2E" in script
    assert "$env:DOKKOMPLEKT_RUN_HARDWARE_E2E = '1'" in script
    assert "Remove-Item Env:DOKKOMPLEKT_RUN_HARDWARE_E2E" in script


def test_fpr18_register_stays_open_until_installed_run_is_green() -> None:
    register = REGISTER.read_text(encoding="utf-8")
    block = register[register.index('"id": "FPR-18"'):register.index('"id": "FPR-19"')]
    assert '"status": "needs-runtime-proof"' in block
