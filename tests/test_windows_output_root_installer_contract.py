from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"


def test_installed_windows_app_must_create_canonical_desktop_output_root() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "[Environment]::GetFolderPath('ApplicationData')" in source
    assert "$appDataRoot = Join-Path $roamingAppData $bundleIdentifier" in source
    assert "Remove-Item -LiteralPath $appDataRoot -Recurse -Force" in source
    assert "[Environment]::GetFolderPath('Desktop')" in source
    assert "Join-Path $desktopPath 'Выписанные пациенты'" in source
    assert "Remove-Item -LiteralPath $defaultOutputRoot" in source
    assert "Installed application did not create the canonical Desktop output root" in source
    assert "Desktop output root created by installed application" in source


def test_native_startup_prepares_output_root_before_webview_creation() -> None:
    native = (ROOT / "src-tauri" / "src" / "main.rs").read_text(encoding="utf-8")
    commands = (ROOT / "src-tauri" / "src" / "subsystems" / "output_root_commands.rs").read_text(encoding="utf-8")

    assert "fn ensure_startup_output_root" in commands
    assert native.index("ensure_startup_output_root(&handle)") < native.index("tauri::WebviewWindowBuilder::from_config")


def test_output_root_collision_requires_the_real_recovery_alert() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "Не удалось восстановить проверенную папку результата:" in source
    assert "Application silently replaced the deliberate output-root collision file." in source
    assert "Output-root path collision crashed the application." in source
