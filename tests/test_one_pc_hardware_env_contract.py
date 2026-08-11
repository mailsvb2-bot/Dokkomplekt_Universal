from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"
PRIVATE_WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"


def read(path: Path) -> str:
    assert path.is_file(), path
    return path.read_text(encoding="utf-8")


def test_hardware_preflight_imports_only_nonsecret_local_runner_configuration() -> None:
    text = read(PREFLIGHT)
    assert "DokkomplektHardwareRunner" in text
    assert "hardware-config.cmd" in text
    assert "GetFolderPath('Startup')" in text
    assert "DokkomplektHardwareRunner.cmd" in text
    assert "GITHUB_ENV" in text
    for name in (
        "DOKKOMPLEKT_TEST_PRINTER",
        "DOKKOMPLEKT_TEST_DUPLEX",
        "DOKKOMPLEKT_TEST_TRAY",
        "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH",
        "DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT",
        "DOKKOMPLEKT_WORD_PATH",
    ):
        assert name in text
    for forbidden in (
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert forbidden in text  # explicitly checked as forbidden exposure
        assert f"'{forbidden}',\n        'DOKKOMPLEKT_TEST" not in text


def test_one_pc_preflight_matches_interactive_startup_bootstrap_contract() -> None:
    text = read(PREFLIGHT)
    assert "interactive-runner-logon-autostart" in text
    assert "runner-listener-interactive" in text
    assert "actions-runner-not-service" in text
    assert "not-local-system" in text
    assert "runner-config-present" in text
    assert "hardware-local-config-present" in text
    assert "reboot-source-docx-configured" in text
    assert "[Parameter(Mandatory = $true)] [string] $PrinterName" not in text
    assert "RunnerTaskName" not in text
    assert "administrator'" not in text


def test_private_workflow_keeps_trust_key_remote_but_can_receive_local_hardware_values() -> None:
    workflow = read(PRIVATE_WORKFLOW)
    hardware = workflow[workflow.index("  hardware-evidence:"):]
    assert "verify_windows_hardware_evidence_host.ps1" in hardware
    assert "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64: ${{ vars.DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64 }}" in hardware
    assert hardware.index("verify_windows_hardware_evidence_host.ps1") < hardware.index("windows_signed_handoff.py verify")
