from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
from pathlib import Path
from unittest import mock

import pytest


ROOT = Path(__file__).resolve().parents[1]
BUILDER = ROOT / "scripts" / "build_windows_runtime_kit.py"
LOCKER = ROOT / "scripts" / "create_runtime_lock.py"
STAGER = ROOT / "scripts" / "prepare_sidecars.py"
VERIFIER = ROOT / "scripts" / "assert_offline_runtime_ready.py"
WRAPPER = ROOT / "scripts" / "prepare_windows_production_runtime.ps1"
EXPECTED_PRODUCTION_COMPONENTS = {
    "tesseract",
    "poppler",
    "libreoffice",
    "sumatrapdf",
    "7zip",
    "llama_cpp",
    "semantic_model",
}


def load_module(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write(path: Path, data: bytes) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def component_tree(root: Path, tool: str) -> Path:
    tree = root / tool
    fixtures: dict[str, dict[str, bytes]] = {
        "tesseract": {
            "tesseract.exe": b"MZ" + b"t" * 64,
            "tessdata/rus.traineddata": b"rus",
            "tessdata/eng.traineddata": b"eng",
        },
        "poppler": {
            "bin/pdftotext.exe": b"MZ" + b"p" * 64,
            "bin/pdftoppm.exe": b"MZ" + b"q" * 64,
            "bin/poppler.dll": b"MZ" + b"d" * 64,
        },
        "libreoffice": {
            "program/soffice.exe": b"MZ" + b"l" * 64,
            "program/soffice.bin": b"bin",
            "program/fundamental.ini": b"[Bootstrap]\n",
        },
        "sumatrapdf": {"SumatraPDF.exe": b"MZ" + b"s" * 64},
        "7zip": {
            "7z.exe": b"MZ" + b"7" * 64,
            "7z.dll": b"MZ" + b"z" * 64,
        },
        "llama_cpp": {"llama-server.exe": b"MZ" + b"a" * 64},
        "semantic_model": {"dokkomplekt-instruct.gguf": b"GGUF-test-model"},
    }
    for relative, payload in fixtures[tool].items():
        write(tree / relative, payload)
    return tree


def make_spec(root: Path, *, omit: str | None = None) -> Path:
    license_file = write(root / "licenses" / "RUNTIME-LICENSE.txt", b"fixture license\n")
    tools = [
        "tesseract",
        "poppler",
        "libreoffice",
        "sumatrapdf",
        "7zip",
        "llama_cpp",
        "semantic_model",
    ]
    components = []
    for tool in tools:
        if tool == omit:
            continue
        tree = component_tree(root / "components", tool)
        components.append(
            {
                "tool": tool,
                "root": str(tree),
                "target_root": tool,
                "version": "1.0.0-test",
                "source_url": f"https://github.com/dokkomplekt-fixtures/{tool}/releases/tag/v1.0.0",
                "license": "TEST-ONLY",
                "license_file": str(license_file),
            }
        )
    spec = {
        "schema": 1,
        "target": "windows-x86_64",
        "review": {
            "reviewer": "runtime-test",
            "reviewed_at": "2026-01-01",
            "scope": "complete synthetic portable trees",
        },
        "components": components,
    }
    path = root / "runtime-kit.json"
    path.write_text(json.dumps(spec), encoding="utf-8")
    return path


def test_builder_creates_lock_that_stages_and_verifies_end_to_end() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit")
    stager = load_module(STAGER, "prepare_sidecars_runtime_kit_test")
    verifier = load_module(VERIFIER, "assert_offline_runtime_ready_runtime_kit_test")

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec = make_spec(root)
        output = root / "output"
        output.mkdir()

        catalog, report = builder.build_catalog(spec, output)
        catalog_path = output / "runtime-catalog.json"
        builder.atomic_json(catalog_path, catalog)
        lock = builder.build_lock(catalog_path)
        lock_path = output / "windows-x86_64-manifest.json"
        builder.atomic_json(lock_path, lock)

        assert lock["supply_chain_locked"] is True
        assert {entry["tool"] for entry in lock["files"]} == builder.PRODUCTION_REQUIRED_TOOLS
        assert report["component_count"] == 7
        assert report["file_count"] == len(lock["files"])
        assert "msgconvert" not in {entry["tool"] for entry in lock["files"]}

        staged_root = root / "staged"
        with mock.patch.object(stager, "DEST_ROOT", staged_root), mock.patch.object(
            sys, "argv", ["prepare_sidecars.py", str(lock_path), "--clean"]
        ):
            assert stager.main() == 0

        verifier.TOOLS_ROOT = staged_root
        target_dir, status = verifier.load_status("windows-x86_64")
        tools = verifier.verify_entries(target_dir, status)
        verifier.verify_supply_chain(target_dir, status)
        verifier.verify_required_runtime(tools, True)
        verifier.verify_distribution_review(target_dir, status, tools)
        assert set(tools) == EXPECTED_PRODUCTION_COMPONENTS


def test_builder_fails_closed_when_required_component_is_missing() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit_missing_component")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec = make_spec(root, omit="llama_cpp")
        with pytest.raises(ValueError, match="llama_cpp"):
            builder.build_catalog(spec, root / "output")


def test_builder_rejects_target_root_not_discoverable_by_desktop_resolver() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit_layout_parity")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec_path = make_spec(root)
        data = json.loads(spec_path.read_text(encoding="utf-8"))
        tesseract = next(item for item in data["components"] if item["tool"] == "tesseract")
        tesseract["target_root"] = "custom/tesseract"
        spec_path.write_text(json.dumps(data), encoding="utf-8")
        with pytest.raises(ValueError, match="desktop resolver"):
            builder.build_catalog(spec_path, root / "output")


def test_builder_rejects_placeholder_provenance() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit_placeholder")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec_path = make_spec(root)
        data = json.loads(spec_path.read_text(encoding="utf-8"))
        data["components"][0]["version"] = "REPLACE_VERSION"
        spec_path.write_text(json.dumps(data), encoding="utf-8")
        with pytest.raises(ValueError, match="placeholder"):
            builder.build_catalog(spec_path, root / "output")


def test_builder_rejects_linklike_component_content() -> None:
    if os.name == "nt":
        pytest.skip("Creating Windows junction/symlink fixtures requires runner privileges")
    builder = load_module(BUILDER, "build_windows_runtime_kit_symlink")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec_path = make_spec(root)
        external = write(root / "external.bin", b"outside reviewed tree")
        link = root / "components" / "tesseract" / "escape.bin"
        link.symlink_to(external)
        with pytest.raises(ValueError, match="symlink or junction"):
            builder.build_catalog(spec_path, root / "output")


def test_production_runtime_surface_is_exactly_seven_components_without_msgconvert() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit_required_set")
    locker = load_module(LOCKER, "create_runtime_lock_required_set")
    assert builder.PRODUCTION_REQUIRED_TOOLS == EXPECTED_PRODUCTION_COMPONENTS
    assert locker.REQUIRED_TOOLS == EXPECTED_PRODUCTION_COMPONENTS
    assert locker.SUPPORTED_TOOLS == EXPECTED_PRODUCTION_COMPONENTS
    assert "msgconvert" not in locker.SUPPORTED_TOOLS



def test_core_profile_filters_semantic_trees_and_stages_exact_document_runtime() -> None:
    builder = load_module(BUILDER, "build_windows_runtime_kit_core")
    stager = load_module(STAGER, "prepare_sidecars_runtime_kit_core")
    verifier = load_module(VERIFIER, "assert_offline_runtime_ready_runtime_kit_core")

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        spec = make_spec(root)
        output = root / "output"
        output.mkdir()
        catalog, report = builder.build_catalog(spec, output, "core")
        catalog_path = output / "runtime-catalog.json"
        builder.atomic_json(catalog_path, catalog)
        lock = builder.build_lock(catalog_path, "core")
        lock_path = output / "windows-x86_64-manifest.json"
        builder.atomic_json(lock_path, lock)

        assert catalog["runtime_profile"] == "core"
        assert lock["runtime_profile"] == "core"
        assert lock["semantic_model_required"] is False
        assert report["runtime_profile"] == "core"
        assert report["component_count"] == 5
        assert {entry["tool"] for entry in lock["files"]} == {
            "tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip"
        }

        staged_root = root / "staged"
        with mock.patch.object(stager, "DEST_ROOT", staged_root), mock.patch.object(
            sys, "argv", ["prepare_sidecars.py", str(lock_path), "--clean"]
        ):
            assert stager.main() == 0
        status = json.loads(
            (staged_root / "windows-x86_64" / "sidecar-status.json").read_text(encoding="utf-8")
        )
        assert status["runtime_profile"] == "core"
        assert status["semantic_model_required"] is False
        with mock.patch.object(verifier, "TOOLS_ROOT", staged_root):
            target_dir, loaded = verifier.load_status("windows-x86_64")
            tools = verifier.verify_entries(target_dir, loaded)
            verifier.verify_required_runtime(tools, False)
            assert set(tools) == {"tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip"}

def test_one_command_wrapper_is_fail_closed_and_network_free() -> None:
    text = WRAPPER.read_text(encoding="utf-8")
    for required in (
        "scripts/build_windows_runtime_kit.py",
        "scripts/prepare_sidecars.py",
        "scripts/assert_offline_runtime_ready.py",
        "--require-semantic-model",
        "--require-supply-chain",
        "--production",
        "windows-x86_64-manifest.json",
    ):
        assert required in text
    for forbidden in ("Invoke-WebRequest", "Invoke-RestMethod", "curl.exe", "Start-BitsTransfer"):
        assert forbidden not in text
