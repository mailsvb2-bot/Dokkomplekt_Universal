from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_rust_backend_accepts_plain_word_templates_as_static_buttons() -> None:
    runtime = text("src-tauri/src/subsystems/document_commands.rs")
    storage = text("crates/dokkomplekt-storage/src/lib.rs")
    start = runtime.index("fn confirm_template_setup(")
    end = runtime.index("struct RenameDocumentButtonRequest", start)
    command = runtime[start:end]

    assert "req.rows.iter().any(|row| row.is_static_copy)" not in command
    assert "Шаблон не содержит размеченных полей" not in command
    assert 'return Err("Выберите хотя бы один шаблон Word."' in command
    assert "create_pack_from_confirmations" in command
    assert "publish_pack_with_template_versions" in command
    assert "save_desktop_snapshot_with_template_versions" in storage


def test_windows_installer_exercises_real_plain_docx_button_creation_and_restart() -> None:
    smoke = text("tests/installer/windows_installer_contract.ps1")

    assert "New-PlainDocxFixture" in smoke
    assert "function Set-UiValue" in smoke
    assert "SendMessage" in smoke
    assert "0x000C" in smoke
    assert "function Submit-OpenFileDialog" in smoke
    assert "SendMessagePtr" in smoke
    assert "0x0111" in smoke
    assert "AutomationId=1" in smoke
    assert "IsValuePatternAvailableProperty" in smoke
    assert "button-smoke.docx" in smoke
    assert "Проверочная кнопка" in smoke
    assert "Create button from a real unmarked DOCX" in smoke
    assert "$application.FullName" not in smoke
    # Baseline install/restart plus adversarial single-instance and fail-closed
    # output-root collision launches must all exercise the installed binary.
    assert smoke.count("Start-Process -FilePath $app.FullName -PassThru") >= 4
    assert "ADVERSARIAL OK: second launch exited and primary UI stayed alive" in smoke
    assert "ADVERSARIAL OK: output-root collision stayed fail-closed and visible" in smoke
    assert "ADVERSARIAL OK: Desktop output root recovered on clean restart" in smoke
    assert "Persisted template button survived application restart" in smoke
