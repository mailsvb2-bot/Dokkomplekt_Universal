from __future__ import annotations

import codecs
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
IMPLEMENTATION = ROOT / "tests" / "installer" / "windows_installer_contract.impl.ps1"
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


def test_bom_launcher_keeps_large_contract_utf8_and_fail_closed() -> None:
    launcher = CONTRACT.read_text(encoding="utf-8-sig")
    implementation = IMPLEMENTATION.read_text(encoding="utf-8")

    assert implementation.startswith("param(")
    assert any(ord(character) > 127 for character in implementation)
    assert "Get-Content -LiteralPath $implementationPath -Raw -Encoding UTF8" in launcher
    assert "[System.Text.UTF8Encoding]::new($true)" in launcher
    assert "expected exactly one commit-boundary anchor" in launcher
    assert "windows_installer_contract.impl.ps1" in launcher
    assert "Windows PowerShell 5.1 is unavailable" in launcher


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
