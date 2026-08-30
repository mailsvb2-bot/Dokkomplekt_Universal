from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
WORKSPACE = ROOT / "src" / "components" / "Workspace.tsx"


def test_windows_installer_smoke_drives_real_generation_to_physical_docx() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")
    workspace = WORKSPACE.read_text(encoding="utf-8")

    assert 'aria-label="Выбрать исходный файл"' in workspace
    assert "Сохранить папку и правило" in source
    assert "Выбрать исходный файл" in source
    assert "native source file picker" in source
    assert "Real source DOCX accepted by installed application" in source
    assert "@('Проверить и создать (1)', 'Создать документы (1)')" in source
    assert "workflow-document-number" in source
    assert "workflow-document-date" in source
    assert "Создать документы" in source
    assert '$expectedGeneratedFileName = "$expectedTemplateButtonName.docx"' in source
    assert "-Filter $expectedGeneratedFileName" in source
    assert "Проверочная кнопка.docx" not in source
    assert "[System.IO.Compression.ZipFile]::OpenRead" in source
    assert "Created DOCX lost the template content" in source
    assert "Installed end-to-end document generation OK" in source
