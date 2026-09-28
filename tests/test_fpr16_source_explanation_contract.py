from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_fpr16_source_explanation_is_wired_to_real_installed_ui():
    workspace = (ROOT / "src" / "components" / "Workspace.tsx").read_text(encoding="utf-8")
    e1 = (ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1").read_text(encoding="utf-8")

    assert "Сверить источник для ${field.field_id}" in workspace
    assert "Сверка источника для ${reviewField.field_id}" in workspace
    assert "Фрагмент источника для ${reviewField.field_id}" in workspace
    assert "Распознанное значение для ${reviewField.field_id}: ${reviewField.value}" in workspace

    assert "FPR-16: source explanation is a real installed user path" in e1
    assert "Сверить источник для document.number" in e1
    assert "Сверка источника для document.number" in e1
    assert "Фрагмент источника для document.number" in e1
    assert "Распознанное значение для document.number: E1-17" in e1
    assert "FPR-16 INSTALLED PASS: current-case source explanation -> document.number -> E1-17" in e1
