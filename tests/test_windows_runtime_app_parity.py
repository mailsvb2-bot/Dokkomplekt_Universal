from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_windows_runtime_app_parity.py"
OFFLINE_CONFIG = ROOT / "src-tauri" / "tauri.offline.conf.json"
SEMANTIC_RUNTIME = ROOT / "src-tauri" / "src" / "semantic_runtime.rs"
BUILD_WINDOWS = ROOT / "BUILD_WINDOWS_INSTALLER.bat"
PREPARE_WINDOWS = ROOT / "scripts" / "prepare_windows_production_runtime.ps1"


def load_module():
    spec = importlib.util.spec_from_file_location("verify_windows_runtime_app_parity", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def valid_status() -> dict:
    paths = {
        "tesseract": ["tesseract/tesseract.exe"],
        "poppler": ["poppler/bin/pdftotext.exe", "poppler/bin/pdftoppm.exe"],
        "libreoffice": ["libreoffice/program/soffice.exe"],
        "sumatrapdf": ["sumatrapdf/SumatraPDF.exe"],
        "7zip": ["7zip/7z.exe"],
        "msgconvert": ["msgconvert/msgconvert.exe"],
        "llama_cpp": ["llama_cpp/llama-server.exe"],
        "semantic_model": ["semantic_model/dokkomplekt-instruct.gguf"],
    }
    return {
        "schema": 1,
        "target": "windows-x86_64",
        "files": [
            {"tool": tool, "path": path}
            for tool, tool_paths in paths.items()
            for path in tool_paths
        ],
    }


def test_valid_production_layout_matches_application_contract() -> None:
    module = load_module()
    tools = module.paths_by_tool(valid_status())
    module.verify_entry_points(tools)


def test_perl_only_msgconvert_is_not_marked_production_ready() -> None:
    module = load_module()
    status = valid_status()
    status["files"] = [
        item for item in status["files"] if item["tool"] != "msgconvert"
    ] + [
        {"tool": "msgconvert", "path": "msgconvert/msgconvert.pl"},
        {"tool": "msgconvert", "path": "msgconvert/bin/perl.exe"},
    ]
    with pytest.raises(ValueError, match="msgconvert/msgconvert.exe"):
        module.verify_entry_points(module.paths_by_tool(status))


def test_noncanonical_target_root_is_rejected() -> None:
    module = load_module()
    status = valid_status()
    for item in status["files"]:
        if item["tool"] == "msgconvert":
            item["path"] = "custom/msgconvert.exe"
            break
    with pytest.raises(ValueError, match="application-resolvable root"):
        module.paths_by_tool(status)


def test_7zz_only_runtime_is_not_claimed_app_launchable() -> None:
    module = load_module()
    status = valid_status()
    for item in status["files"]:
        if item["tool"] == "7zip":
            item["path"] = "7zip/7zz.exe"
            break
    with pytest.raises(ValueError, match="7zip/7z.exe"):
        module.verify_entry_points(module.paths_by_tool(status))


def test_semantic_model_must_be_deterministic_single_gguf() -> None:
    module = load_module()
    status = valid_status()
    status["files"].append(
        {"tool": "semantic_model", "path": "semantic_model/second.gguf"}
    )
    with pytest.raises(ValueError, match="exactly one"):
        module.verify_entry_points(module.paths_by_tool(status))


def test_offline_tauri_config_flattens_staging_target_prefix() -> None:
    module = load_module()
    module.verify_offline_resource_mapping(OFFLINE_CONFIG)
    config = json.loads(OFFLINE_CONFIG.read_text(encoding="utf-8"))
    assert config["bundle"]["resources"] == {
        "resources/tools/windows-x86_64/": "resources/tools/"
    }


def test_semantic_runtime_has_canonical_bundled_fallbacks() -> None:
    source = SEMANTIC_RUNTIME.read_text(encoding="utf-8")
    assert 'root.join("llama_cpp").join(name)' in source
    assert '["llama-server.exe", "server.exe"]' in source
    assert 'root.join("semantic_model")' in source
    assert 'eq_ignore_ascii_case("gguf")' in source
    assert "candidates.len() == 1" in source


def test_all_windows_production_entrypoints_run_parity_gate() -> None:
    required = "scripts\\verify_windows_runtime_app_parity.py"
    assert required in BUILD_WINDOWS.read_text(encoding="utf-8")
    powershell = PREPARE_WINDOWS.read_text(encoding="utf-8")
    assert "scripts/verify_windows_runtime_app_parity.py" in powershell


def test_parity_config_validator_fails_closed_on_legacy_glob_layout() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "tauri.json"
        path.write_text(
            json.dumps({"bundle": {"resources": ["resources/tools/**"]}}),
            encoding="utf-8",
        )
        with pytest.raises(ValueError, match="source->target map"):
            module.verify_offline_resource_mapping(path)
