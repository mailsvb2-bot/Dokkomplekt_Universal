from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_first_run_button_uses_native_template_picker() -> None:
    app = text("src/App.tsx")
    rail = text("src/components/DocumentRail.tsx")
    api = text("src/lib/api.ts")
    runtime = text("src-tauri/src/subsystems/template_picker.rs")
    main = text("src-tauri/src/main.rs")

    assert "pickTemplateFiles" in app
    assert "run('pick_template_files'" in app
    assert "setSetupOpen(false)" in app
    assert "setSetupOpen(true)" in app
    assert "Создать свои кнопки" in rail
    assert "disabled={props.busy}" in rail
    assert "callRust<{ files: PickedTemplateFile[] }>('pick_template_files'" in api
    assert "async fn pick_template_files" in runtime
    assert "System.Windows.Forms.OpenFileDialog" in runtime
    assert "$dialog.Multiselect = $true" in runtime
    assert "CREATE_NO_WINDOW" in runtime
    assert "validate_safe_template_file" in runtime
    assert "extract_docx_text" in runtime
    assert "pick_template_files," in main


def test_windows_installer_smoke_clicks_real_button_and_requires_native_dialog() -> None:
    smoke = text("tests/installer/windows_installer_contract.ps1")
    assert "UIAutomationClient" in smoke
    assert "Создать свои кнопки" in smoke
    assert "Выберите шаблоны Word" in smoke
    assert "Native first-run template picker OK" in smoke
