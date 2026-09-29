from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr12_installed_restart_and_installer_preservation_are_locked() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "FPR-12 RESTART STORAGE PASS" in source
    assert "FPR-12 RESTART SEMANTIC PASS" in source
    assert "$expectedFpr12RestartFolder = Join-Path $defaultOutputRoot '3333 27.08.2026'" in source
    assert "$restartCreated.Directory.FullName" in source
    assert "FPR-12 INSTALLER PRESERVATION PASS" in source
    assert "$fpr12Replacement = Start-Process -FilePath $installer.FullName" in source
    assert "$fpr12AfterReplacementStorage = Wait-AppStateCipherFingerprint" in source
    assert "FPR-12 installer replacement lost the persisted workspace/button state." in source


def test_fpr12_register_does_not_overclaim_cross_version_upgrade() -> None:
    register = json.loads(REGISTER.read_text(encoding="utf-8"))
    entry = next(item for item in register["features"] if item["id"] == "FPR-12")

    assert entry["status"] == "needs-runtime-proof"
    assert any("clean-restart" in item for item in entry["runtime_evidence"])
    assert any("NSIS replacement" in item for item in entry["runtime_evidence"])
    assert "previous-version -> current-version" in entry["runtime_gap"]
