from __future__ import annotations

import ast
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_trial_allows_one_complete_monthly_kit() -> None:
    source = (ROOT / "src-tauri/src/main.rs").read_text("utf-8")
    assert "const TRIAL_MAX_DOCUMENTS_PER_RUN: u32 = TRIAL_DOCUMENT_LIMIT_MONTH;" in source
    assert "запрошено {}" in source
    assert "лимит за запуск {}" in source


def test_thin_and_offline_installers_have_distinct_payloads() -> None:
    thin = json.loads((ROOT / "src-tauri/tauri.thin.conf.json").read_text("utf-8"))
    offline = json.loads((ROOT / "src-tauri/tauri.offline.conf.json").read_text("utf-8"))
    assert thin["bundle"]["windows"]["webviewInstallMode"]["type"] == "downloadBootstrapper"
    assert thin["bundle"]["resources"] == []
    assert offline["bundle"]["windows"]["webviewInstallMode"]["type"] == "offlineInstaller"
    assert offline["bundle"]["resources"] == {
        "resources/tools/windows-x86_64/": "resources/tools/"
    }


def test_local_windows_release_signs_binary_and_installer_with_hsm_only() -> None:
    script = (ROOT / "BUILD_WINDOWS_INSTALLER.bat").read_text("utf-8")
    assert 'set "DOKKOMPLEKT_RELEASE_MODE=production"' in script
    assert 'if /I not "%DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND%"=="certificate-store"' in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT is required" in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER is required" in script
    assert "DOKKOMPLEKT_TIMESTAMP_SERVER is required for production signing" in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is forbidden in production" in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is forbidden in production" in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is required" not in script
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is required" not in script
    assert "sign_windows_release.ps1 -ArtifactRoot target\\release\\dokkomplekt-tauri.exe" in script
    assert "sign_windows_release.ps1 -ArtifactRoot target\\release\\bundle\\nsis" in script
    assert "DOKKOMPLEKT_REQUIRE_AUTHENTICODE=1" in script
    assert "tauri.offline.conf.json" in script


def test_release_assets_wait_for_hardware_e2e() -> None:
    workflow = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")
    assert "types: [published]" in workflow
    assert "needs: [windows-hardware-e2e, linux-bundles]" in workflow
    assert "Publish only verified signed release assets" in workflow


def test_source_archive_excludes_virtual_environments_and_ascii_launchers_exist() -> None:
    module_path = ROOT / "scripts/build_source_archive.py"
    spec = importlib.util.spec_from_file_location("source_archive", module_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assert {".venv", "venv", ".tox", ".nox"} <= module.EXCLUDED_DIRS
    assert (ROOT / "CHECK_PROJECT.bat").is_file()
    assert (ROOT / "BUILD_EXE.bat").is_file()
    assert not (ROOT / "ПРОВЕРИТЬ_ПРОЕКТ.bat").exists()
    assert not (ROOT / "СОБРАТЬ_EXE.bat").exists()


def test_rustsec_evidence_requires_a_real_database_commit() -> None:
    source = (ROOT / "scripts/write_rustsec_evidence.py").read_text("utf-8")
    tree = ast.parse(source)
    assert "advisory_database_commit" in source
    assert "len(head) != 40" in source
    assert any(isinstance(node, ast.Raise) for node in ast.walk(tree))


def test_background_watcher_install_is_durable_and_error_events_match_frontend_contract() -> None:
    source = (ROOT / "src-tauri/src/subsystems/watcher_commands.rs").read_text("utf-8")
    assert "fn write_autostart_entries(exe: &Path) -> Result<(Vec<PathBuf>, Vec<String>), String>" in source
    assert "installed=true" in source
    assert "match write_autostart_entries(&exe)" in source
    assert "status: \"error\".into()" not in source
    assert source.count("status: \"attention\".into()") >= 2

    frontend = (ROOT / "src/lib/runtimeValidation.ts").read_text("utf-8")
    assert "'processed', 'attention', 'setup_needed', 'ignored'" in frontend
