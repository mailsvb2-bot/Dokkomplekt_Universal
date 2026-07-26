from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_hardware_e2e_installs_watcher_through_real_application_command() -> None:
    main = text("src-tauri/src/main.rs")
    script = text("tests/windows/windows_hardware_e2e.ps1")
    assert '--e2e-install-watcher=' in main
    assert '--e2e-uninstall-watcher' in main
    assert 'DOKKOMPLEKT_RUN_HARDWARE_E2E' in main
    assert '--e2e-install-watcher=$watchFolder' in script
    assert 'WATCHER_INSTALL.json' in script
    assert 'created by this scenario' in script


def test_hardware_gate_requires_real_reboot_and_post_reboot_case() -> None:
    script = text("tests/windows/windows_hardware_e2e.ps1")
    prepare = text("tests/windows/prepare_reboot_evidence.ps1")
    verifier = text("tests/windows/verify_reboot_evidence.ps1")
    assert 'operating_system_reboot_tested = $true' in script
    assert 'operating_system_reboot_tested = $false' not in script
    assert 'verify_reboot_evidence.ps1' in script
    assert 'DOKKOMPLEKT_REBOOT_EVIDENCE' in script
    assert 'New-ScheduledTaskTrigger -AtLogOn' in prepare
    assert "watcher_started_after_reboot = `$watcherStarted" in prepare
    assert "post_reboot_case_completed = `$completed" in prepare
    assert "No operating-system reboot was demonstrated" in verifier
