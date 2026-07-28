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
    assert offline["bundle"]["resources"] == ["resources/tools/**"]


def test_local_windows_release_signs_binary_and_installer() -> None:
    script = (ROOT / "BUILD_WINDOWS_INSTALLER.bat").read_text("utf-8")
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
