from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_daily_ui_uses_product_dialogs_not_browser_prompts() -> None:
    combined = "\n".join(
        text(path)
        for path in [
            "src/App.tsx",
            "src/components/AutomationControlCenter.tsx",
            "src/components/LearningGovernancePanel.tsx",
        ]
    )
    assert "globalThis.prompt" not in combined
    assert "globalThis.confirm" not in combined
    assert "useAppDialog" in combined
    provider = text("src/components/AppDialogProvider.tsx")
    assert 'role="dialog"' in provider
    assert "aria-modal=\"true\"" in provider
    assert "event.key === 'Escape'" in provider


def test_folder_picker_is_wired_end_to_end_and_hidden_on_windows() -> None:
    app = text("src/App.tsx")
    api = text("src/lib/api.ts")
    runtime = text("src-tauri/src/subsystems/automation_runtime.rs")
    main = text("src-tauri/src/main.rs")
    workspace = text("src/components/Workspace.tsx")
    settings = text("src/components/UtilityPanel.tsx")
    assert "pickFolder" in app
    assert "callRust<{ selected_path: string | null }>('pick_folder'" in api
    assert "async fn pick_folder" in runtime
    assert "CREATE_NO_WINDOW" in runtime
    assert "FolderBrowserDialog" in runtime
    assert "zenity" in runtime and "kdialog" in runtime and "osascript" in runtime
    assert "pick_folder," in main
    assert "onPickWatchFolder" in workspace
    assert "onPickOutputFolder" in settings
