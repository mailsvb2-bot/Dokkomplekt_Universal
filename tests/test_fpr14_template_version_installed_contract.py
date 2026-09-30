from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1"
MAIN_RS = ROOT / "src-tauri" / "src" / "main.rs"
FIXTURE_RS = ROOT / "src-tauri" / "src" / "subsystems" / "template_version_e2e.rs"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr14_installed_flow_selects_superseded_version_and_reads_physical_docx() -> None:
    script = INSTALLER.read_text(encoding="utf-8-sig")
    for marker in (
        "FPR-14 INSTALLED PASS:",
        "FPR14-V1-SNAPSHOT",
        "FPR14-V2-SNAPSHOT",
        "Версии шаблона",
        "Использовать версию 1?",
        "DOKKOMPLEKT_E2E_TEMPLATE_VERSION_REQUEST",
        "rollback output did not contain the archived v1 marker",
        "rollback output incorrectly rendered the superseded-current v2 marker",
    ):
        assert marker in script, marker


def test_fpr14_fixture_is_guarded_and_uses_installed_application_state() -> None:
    main = MAIN_RS.read_text(encoding="utf-8")
    source = FIXTURE_RS.read_text(encoding="utf-8")
    assert 'include!("subsystems/template_version_e2e.rs");' in main
    assert "run_template_version_installer_e2e(&handle, &state)" in main
    assert "DOKKOMPLEKT_RUN_INSTALLER_E2E" in source
    assert "DOKKOMPLEKT_E2E_TEMPLATE_VERSION_REQUEST" in source
    assert "DOKKOMPLEKT_E2E_EVIDENCE_PATH" in source
    assert "update_document_template(" in source
    assert "template-version fixture document not found for button" in source
    assert "dokkomplekt.template-version-fixture.v1" in source


def test_fpr14_register_waits_for_exact_green_installed_evidence() -> None:
    register = json.loads(REGISTER.read_text(encoding="utf-8"))
    feature = next(item for item in register["features"] if item["id"] == "FPR-14")
    assert feature["status"] == "needs-runtime-proof"
    assert "FPR-14 INSTALLED PASS" in feature["runtime_gap"]
    assert any(
        "real Versions UI" in evidence and "physical DOCX marker proof" in evidence
        for evidence in feature["runtime_evidence"]
    )
