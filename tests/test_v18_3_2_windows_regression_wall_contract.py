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


def test_hardware_gate_requires_visible_gui_and_no_console_shell_descendants() -> None:
    script = text("tests/windows/windows_hardware_e2e.ps1")
    assert "Wait-VisibleApplicationWindow" in script
    assert "Get-NewVisibleConsoleWindows" in script
    assert "ConsoleWindowClass" in script
    assert "GUI_AND_CONSOLE_EVIDENCE.json" in script
    assert "dokkomplekt.gui-console-evidence.v1" in script
    assert "application_sha256" in script
    assert "No visible titled GUI window appeared" in script
    assert "Unexpected visible console or script-host window" in script
    assert "console_observation_milliseconds" in script
    assert "gui_window_observed = $true" in script
    assert "unexpected_console_windows_observed = $false" in script
    assert "gui_console_evidence_sha256" in script
    assert "dokkomplekt.windows-hardware-e2e.v3" in script
