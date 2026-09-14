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
    assert "publication-digest-v1" in source
    assert "published-readback-v1" in source
    assert "E1 INSTALLED PASS: Accounting source -> UI -> physical DOCX -> committed receipt" in source
    assert "Windows installer smoke" in workflow
    assert "Windows E1 Accounting installed path" in workflow
    assert workflow.index("Windows installer smoke") < workflow.index("Windows E1 Accounting installed path")
    assert "tests/installer/windows_e1_accounting_contract.ps1" in workflow
