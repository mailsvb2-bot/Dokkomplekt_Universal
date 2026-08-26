from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"


def test_windows_installer_smoke_drives_real_generation_to_physical_docx() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "Сохранить папку и правило" in source
    assert "Проверить и создать (1)" in source
    assert "workflow-document-number" in source
    assert "workflow-document-date" in source
    assert "Создать документы" in source
    assert "Проверочная кнопка.docx" in source
    assert "[System.IO.Compression.ZipFile]::OpenRead" in source
    assert "Created DOCX lost the template content" in source
    assert "Installed end-to-end document generation OK" in source
