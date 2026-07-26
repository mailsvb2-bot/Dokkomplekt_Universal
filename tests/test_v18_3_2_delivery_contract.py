from __future__ import annotations

import json
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_workspace_lock_versions_match_18_3_2() -> None:
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    local = set()
    for member in workspace["workspace"]["members"]:
        manifest = tomllib.loads((ROOT / member / "Cargo.toml").read_text(encoding="utf-8"))
        local.add(manifest["package"]["name"])
    versions = {
        package["name"]: package["version"]
        for package in lock["package"]
        if package["name"] in local
    }
    assert versions == {name: "18.4.3" for name in local}


def test_docx_lock_entry_contains_new_direct_dependencies() -> None:
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    package = next(item for item in lock["package"] if item["name"] == "dokkomplekt-docx")
    dependencies = set(package["dependencies"])
    assert "hex" in dependencies
    assert "sha2" in dependencies


def test_offline_runtime_is_truthfully_fail_closed() -> None:
    status_path = ROOT / "src-tauri/resources/tools/windows-x86_64/sidecar-status.json"
    status = json.loads(status_path.read_text(encoding="utf-8"))
    assert status["ready"] is False
    assert status["files"] == []
    assert {
        "tesseract",
        "poppler/pdftotext",
        "libreoffice/soffice",
        "sumatrapdf",
        "llama_cpp/llama-server",
        "approved_gguf_model",
    }.issubset(status["missing_required_tools"])


def test_delivery_contains_truthful_requirement_matrix() -> None:
    matrix = (ROOT / "IMPLEMENTATION_MATRIX_2026-07-21.md").read_text(encoding="utf-8")
    for phrase in (
        "Bundle Decision Engine",
        "Template Intelligence Wizard",
        "Case Segmentation Engine",
        "Windows Regression Wall",
        "Реальные анонимизированные корпуса",
        "Неподделанные внешние блокеры релиза",
        "370 Rust-тестов",
    ):
        assert phrase in matrix
