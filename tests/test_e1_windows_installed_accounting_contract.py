from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1"
WORKFLOW = ROOT / ".github" / "workflows" / "quality-gate.yml"


def test_e1_accounting_installed_lane_is_real_and_fail_closed() -> None:
    assert SCRIPT.read_bytes().startswith(b"\xef\xbb\xbf"), (
        "Windows PowerShell 5.1 requires UTF-8 BOM for Cyrillic literals in the E1 installed contract"
    )
    source = SCRIPT.read_text(encoding="utf-8-sig")
    workflow = WORKFLOW.read_text(encoding="utf-8")
    app_source = (ROOT / "src" / "App.tsx").read_text(encoding="utf-8")
    assert app_source.count("semanticExtract(") == 1, "automatic source intake must not run a second state-owning semantic extraction"
    assert app_source.count("semanticPreviewFromParsedSource(res)") == 3

    assert "preceding installed baseline smoke" in source
    assert "content-packs\\tier1-accounting-ru\\templates\\service_act.docx" in source
    assert "Название документа для service_act.docx" in source
    assert "workflow-amount-currency" in source
    assert "workflow-amount-vat" in source
    assert "$sourceOwnedValues = [ordered]@{" in source
    assert "E1 source-owned prompt drift" in source
    assert "$manualPromptValues = [ordered]@{" in source
    manual_block = source[source.index("$manualPromptValues = [ordered]@{"):source.index("foreach ($fieldId in $manualPromptValues.Keys)")]
    assert "amount.currency" in manual_block and "amount.vat" in manual_block
    for forbidden in ("document.number", "document.date", "org.name", "counterparty.name", "contract.number", "contract.date", "contract.subject", "amount.total"):
        assert forbidden not in manual_block
    assert "stale blocked-draft error did not clear" in source
    assert "faulty Accounting draft produced a physical DOCX" in source
    assert "faulty Accounting draft produced a committed receipt" in source
    assert "word/document.xml" in source
    assert "Get-FileHash" in source
    assert "generation-completion-receipts" in source
    assert "physical-output-sha256-v1" in source
    assert "published-readback-v1" in source
    assert "E1 INSTALLED PASS: Accounting source -> UI -> physical DOCX -> committed receipt" in source
    assert "Windows installer smoke" in workflow
    assert "Windows E1 cross-domain installed path" in workflow
    assert workflow.index("Windows installer smoke") < workflow.index("Windows E1 Accounting installed path")
    assert "tests/installer/windows_e1_accounting_contract.ps1" in workflow
    assert "public static extern IntPtr SendMessage" in source
    assert "public static extern IntPtr SendMessagePtr" in source
    assert "AutomationIdProperty" in source
    assert "'1'" in source
    assert "0x0111" in source
    assert "[IntPtr]1" in source
    assert "OpenFileDialog exposes neither AutomationId=1 nor a native HWND." in source
    assert "Find-ReadyButtonByNames -Root $Dialog -Names @('Открыть', 'Open')" not in source
    assert "IsValuePatternAvailableProperty" in source
    assert "NativeWindowHandle" in source
    assert "0x000C" in source
    assert "UI value control exposes neither ValuePattern nor a native HWND." in source
    assert "IsLegacyIAccessiblePatternAvailableProperty" in source
    assert "LegacyIAccessiblePattern" in source
    assert "GetWindowTextLength" in source
    assert "GetWindowText(IntPtr hWnd" in source
    assert "neither ValuePattern, LegacyIAccessible value, nor a native HWND" in source
    assert "function Invoke-UiActionWithObservedTransition" in source
    assert "produced no observable transition and remains actionable; retrying once with physical input" in source
    assert "$templateDialog = Invoke-UiActionWithObservedTransition" in source
    assert "$sourceDialog = Invoke-UiActionWithObservedTransition" in source
    assert "-TransitionProbe { Find-FileDialog }" in source
