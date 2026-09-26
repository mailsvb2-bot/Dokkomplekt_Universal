from __future__ import annotations

import codecs
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
QUALITY_GATE_BAT = ROOT / "scripts" / "run_quality_gate.bat"
UNSIGNED_PREVIEW = ROOT / ".github" / "workflows" / "unsigned-preview.yml"
E1_DOMAIN_MATRIX = ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1"
QUALITY_GATE = ROOT / ".github" / "workflows" / "quality-gate.yml"


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


def test_e1_installed_domain_matrix_keeps_all_canon_domains_and_physical_proof() -> None:
    source = E1_DOMAIN_MATRIX.read_text(encoding="utf-8-sig")
    for marker in (
        "E1 INSTALLED PASS: Accounting",
        "E1 Юридический договор",
        "E1 Трудовой договор",
        "E1 Справка об обучении",
        "E1 Архитектурное заключение",
        "Юридическая работа",
        "Кадровая работа",
        "Бухгалтерия",
        "Образование",
        "Своя профессия / профиль",
        "E1 FPR-21 PASS: installed Medical/Legal/HR/Accounting/Education/Custom",
        "physical DOCX -> committed receipt",
    ):
        assert marker in source

    for required_field in (
        "contract.party_a",
        "contract.party_b",
        "employee.position",
        "employee.hire_date",
        "employee.contract_number",
        "education.institution",
    ):
        assert required_field in source


def test_quality_gate_names_and_uploads_cross_domain_installed_evidence() -> None:
    source = QUALITY_GATE.read_text(encoding="utf-8")
    assert "Windows E1 cross-domain installed path" in source
    assert "installer-e1-domain-matrix-${{ runner.os }}.log" in source
    assert "DOKKOMPLEKT_ADVERSARIAL: '1'" in source
    assert source.index("Windows installer smoke") < source.index("Windows E1 cross-domain installed path")


def test_e1_domain_selector_uses_physical_radio_and_behavioral_proof() -> None:
    source = E1_DOMAIN_MATRIX.read_text(encoding="utf-8-sig")
    for marker in (
        '$radioName = "$OptionName для $FileName"',
        'Invoke-UiElementPhysically -Element $radio',
        'IsSelectionItemPatternAvailable',
        'IsTogglePatternAvailable',
        'downstream domain-specific installed behavior remains authoritative',
        "foreach ($fieldId in $PluginRequiredFields)",
        "did not expose canonical domain-required field",
    ):
        assert marker in source
    assert "$domainOffsets = @{" not in source
    assert "SendWait('{HOME}')" not in source
    assert "SendWait('{DOWN}')" not in source



def test_e1_custom_domain_value_commits_through_react_input_event() -> None:
    source = E1_DOMAIN_MATRIX.read_text(encoding="utf-8-sig")
    for marker in (
        "React controls this input",
        "Set-UiValue -Element $custom -Value $CustomProfile",
        "SendWait(' ')",
        "SendWait('{BACKSPACE}')",
        "persisted custom domain value for $FileName",
        "E1 custom domain value commit:",
        "custom domain override did not persist",
    ):
        assert marker in source
    custom_start = source.index("Set-UiValue -Element $custom -Value $CustomProfile")
    custom_space = source.index("SendWait(' ')", custom_start)
    custom_backspace = source.index("SendWait('{BACKSPACE}')", custom_space)
    custom_persist = source.index("persisted custom domain value for $FileName", custom_backspace)
    assert custom_start < custom_space < custom_backspace < custom_persist



def test_e1_cross_domain_scenarios_reset_case_through_real_ui() -> None:
    source = E1_DOMAIN_MATRIX.read_text(encoding="utf-8-sig")
    for marker in (
        'reset case before $($scenario.Label)',
        "'Новый комплект', 'Новый пациент / дело'",
        'empty case before $($scenario.Label)',
        "'Выбрать исходный файл'",
        'Set-E1DomainSource -SourcePath $crossDomainSource',
    ):
        assert marker in source
    assert source.index('reset case before $($scenario.Label)') < source.index('Set-E1DomainSource -SourcePath $crossDomainSource')


def test_e1_physical_retry_restores_installed_app_foreground() -> None:
    source = E1_DOMAIN_MATRIX.read_text(encoding="utf-8-sig")
    for marker in (
        "public static extern bool ShowWindow",
        "public static extern bool SetForegroundWindow",
        "$process.Refresh()",
        "$windowHandle = [IntPtr]$process.MainWindowHandle",
        "[DokkomplektE1NativeMouse]::ShowWindow($windowHandle, 5)",
        "[DokkomplektE1NativeMouse]::SetForegroundWindow($windowHandle)",
        "$Element.SetFocus()",
    ):
        assert marker in source
