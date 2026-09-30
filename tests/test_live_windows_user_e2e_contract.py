from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "verification" / "e2e" / "LIVE_USER_SCENARIOS.json"
HARNESS = ROOT / "tests" / "windows" / "windows_live_user_e2e.ps1"
PRIVATE_WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
HOST_PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"
LIVE_UPDATE = ROOT / "tests" / "windows" / "windows_live_update_e2e.ps1"
QUALITY = ROOT / ".github" / "workflows" / "quality-gate.yml"


def test_live_registry_is_exhaustive_for_every_fpr() -> None:
    payload = json.loads(REGISTRY.read_text(encoding="utf-8"))
    assert payload["schema"] == "dokkomplekt.live-user-scenarios.v1"
    scenarios = payload["scenarios"]
    ids = [item["id"] for item in scenarios]
    assert len(ids) == len(set(ids))
    for number in range(1, 24):
        assert f"FPR-{number:02d}" in ids
    for item in scenarios:
        assert item["required"] is True
        assert item["lane"] in {
            "installed-baseline",
            "installed-e1",
            "hardware-live",
            "reboot-live",
            "update-live",
        }
        assert item["executor"]
        assert item["evidence_marker"]


def test_live_harness_requires_real_windows11_x64_interactive_signed_install() -> None:
    source = HARNESS.read_text(encoding="utf-8-sig")
    for marker in (
        "Live E2E requires Windows 11",
        "[Environment]::Is64BitOperatingSystem",
        "real interactive user session",
        "Get-Service -Name 'actions.runner.*'",
        "Get-AuthenticodeSignature",
        "Live E2E refuses unsigned/invalid installer",
        "windows_installer_contract.ps1",
        "windows_e1_accounting_contract.ps1",
        "src-tauri/tauri.offline.conf.json",
        "DOKKOMPLEKT_ADVERSARIAL = '1'",
        "LIVE WINDOWS USER E2E INSTALLED LANES PASSED",
    ):
        assert marker in source


def test_private_workflow_runs_live_suite_after_signed_handoff_verification() -> None:
    workflow = PRIVATE_WORKFLOW.read_text(encoding="utf-8")
    hardware = workflow[workflow.index("  hardware-evidence:") :]
    assert "windows_signed_handoff.py verify" in hardware
    assert "Execute full installed live user scenario suite" in hardware
    assert "windows_live_user_e2e.ps1" in hardware
    assert "LIVE_USER_E2E.json" in hardware
    assert hardware.index("windows_signed_handoff.py verify") < hardware.index(
        "Execute full installed live user scenario suite"
    )
    assert hardware.index("Execute full installed live user scenario suite") < hardware.index(
        "Prepare real reboot E2E state"
    )


def test_hardware_host_is_pinned_to_windows11_x64() -> None:
    source = HOST_PREFLIGHT.read_text(encoding="utf-8")
    assert "Get-CimInstance Win32_OperatingSystem" in source
    assert "Add-Check -Name 'windows-11'" in source
    assert "Add-Check -Name 'windows-x64'" in source
    assert "Windows 11" in source


def test_update_live_lane_is_real_previous_signed_gui_update() -> None:
    source = LIVE_UPDATE.read_text(encoding="utf-8-sig")
    for marker in (
        "DOKKOMPLEKT_PREVIOUS_SIGNED_INSTALLER",
        "Get-AuthenticodeSignature",
        "Проверить обновления",
        "Установить и перезапустить",
        "update-recovery.json",
        "recovery.status -ne 'verified'",
        "FPR-19 LIVE UPDATE PASS:",
    ):
        assert marker in source
    preflight = HOST_PREFLIGHT.read_text(encoding="utf-8")
    workflow = PRIVATE_WORKFLOW.read_text(encoding="utf-8")
    assert "previous-signed-installer-configured" in preflight
    assert "windows_live_update_e2e.ps1" in workflow
    assert workflow.index("Execute full installed live user scenario suite") < workflow.index(
        "Execute previous-version signed update/recovery live scenario"
    )


def test_mocked_browser_lane_is_not_the_live_evidence_lane() -> None:
    quality = QUALITY.read_text(encoding="utf-8")
    registry = REGISTRY.read_text(encoding="utf-8")
    assert "Browser UI e2e (mocked Tauri IPC)" in quality
    assert "mocked IPC is not accepted" in registry
