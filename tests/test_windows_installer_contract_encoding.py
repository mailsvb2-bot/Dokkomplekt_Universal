from __future__ import annotations

import codecs
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
QUALITY_GATE_BAT = ROOT / "scripts" / "run_quality_gate.bat"
UNSIGNED_PREVIEW = ROOT / ".github" / "workflows" / "unsigned-preview.yml"


def test_legacy_windows_powershell_contract_is_utf8_bom_marked() -> None:
    payload = CONTRACT.read_bytes()
    assert payload.startswith(codecs.BOM_UTF8), (
        "windows_installer_contract.ps1 contains Cyrillic fixtures and is invoked "
        "by Windows PowerShell 5.1; without a UTF-8 BOM powershell.exe decodes it "
        "as the legacy ANSI code page and can fail before the contract executes"
    )
    source = payload.decode("utf-8-sig")
    assert source.startswith("param(")
    assert any(ord(character) > 127 for character in source)


def test_direct_bom_contract_exercises_refreshed_output_identity_prompts() -> None:
    source = CONTRACT.read_text(encoding="utf-8-sig")
    assert "E2 refreshed document number" in source
    assert "E2 refreshed document date" in source
    assert "workflow-document-number" in source
    assert "workflow-document-date" in source
    assert "E2-7708004767" in source
    assert "18.09.2026" in source
    assert "E2 Создать документы after refreshed preflight" in source
    assert "confirm a second time instead of bypassing the naming contract" in source


def test_quality_gate_still_executes_contract_with_legacy_powershell() -> None:
    source = QUALITY_GATE_BAT.read_text(encoding="utf-8")
    assert (
        "powershell -ExecutionPolicy Bypass -File "
        "tests\\installer\\windows_installer_contract.ps1"
    ) in source


def test_unsigned_windows_preview_pins_python_for_sqlite_installer_evidence() -> None:
    source = UNSIGNED_PREVIEW.read_text(encoding="utf-8")
    contract_call = "run: tests/installer/windows_installer_contract.ps1"
    setup = "actions/setup-python@a26af69be951a213d495a4c3e4e4022e16d87065"
    assert setup in source
    assert "python-version: '3.12'" in source
    assert source.index(setup) < source.index(contract_call)
