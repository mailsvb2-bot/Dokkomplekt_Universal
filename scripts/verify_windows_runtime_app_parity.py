#!/usr/bin/env python3
"""Prove that a staged production Windows runtime is launchable by the desktop app.

Supply-chain verification proves that staged files are approved and intact. This
additional gate proves a different invariant: the staged directory layout and
entry-point names must match the paths the installed Rust application resolves.
It also verifies that the offline Tauri config strips the staging target prefix
(`windows-x86_64`) when embedding resources into the installed application.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

try:
    from scripts._release_policy import validate_relative_runtime_path
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
OFFLINE_TAURI_CONFIG = ROOT / "src-tauri" / "tauri.offline.conf.json"
WINDOWS_TARGET = "windows-x86_64"
RESOURCE_SOURCE = f"resources/tools/{WINDOWS_TARGET}/"
RESOURCE_TARGET = "resources/tools/"

# These are installed-resource paths relative to `$RESOURCE/resources/tools/`.
# They intentionally mirror the Rust resolver's canonical tool directories.
REQUIRED_EXACT_PATHS: dict[str, tuple[str, ...]] = {
    "tesseract": ("tesseract/tesseract.exe",),
    "poppler": ("poppler/bin/pdftotext.exe", "poppler/bin/pdftoppm.exe"),
    "libreoffice": ("libreoffice/program/soffice.exe",),
    "sumatrapdf": ("sumatrapdf/SumatraPDF.exe",),
    "7zip": ("7zip/7z.exe",),
    # The desktop MSG intake executes a Windows executable. A bare .pl + Perl
    # tree may still be useful for development/probing, but it is not a
    # production-ready application entry point until wrapped by msgconvert.exe.
    "msgconvert": ("msgconvert/msgconvert.exe",),
}
SEMANTIC_SERVER_CHOICES = (
    "llama_cpp/llama-server.exe",
    "llama_cpp/server.exe",
)
SEMANTIC_MODEL_PREFIX = "semantic_model/"


def safe_relative(value: object) -> str:
    return validate_relative_runtime_path(value, "staged runtime path")


def load_status(target: str = WINDOWS_TARGET) -> dict[str, Any]:
    if target != WINDOWS_TARGET:
        raise ValueError(f"Windows application parity only supports {WINDOWS_TARGET}")
    status_path = TOOLS_ROOT / target / "sidecar-status.json"
    if not status_path.is_file():
        raise FileNotFoundError(f"missing staged runtime status: {status_path}")
    data = json.loads(status_path.read_text("utf-8"))
    if data.get("schema") != 1 or data.get("target") != target:
        raise ValueError("sidecar status schema/target mismatch")
    if not isinstance(data.get("files"), list) or not data["files"]:
        raise ValueError("sidecar status has no staged files")
    return data


def paths_by_tool(status: dict[str, Any]) -> dict[str, set[str]]:
    tools: dict[str, set[str]] = {}
    for index, raw in enumerate(status.get("files", [])):
        if not isinstance(raw, dict):
            raise ValueError(f"files[{index}] must be an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if not tool:
            raise ValueError(f"files[{index}].tool is empty")
        path = safe_relative(raw.get("path", ""))
        canonical_prefix = f"{tool}/"
        if not path.lower().startswith(canonical_prefix.lower()):
            raise ValueError(
                f"{tool} is staged outside its application-resolvable root: {path}; "
                f"expected prefix {canonical_prefix}"
            )
        tools.setdefault(tool, set()).add(path)
    return tools


def verify_entry_points(tools: dict[str, set[str]]) -> None:
    lower_tools = {
        tool: {path.lower() for path in paths}
        for tool, paths in tools.items()
    }
    for tool, required in REQUIRED_EXACT_PATHS.items():
        available = lower_tools.get(tool, set())
        for path in required:
            if path.lower() not in available:
                raise ValueError(
                    f"production runtime is not launchable by the app: missing {path}"
                )

    llama = lower_tools.get("llama_cpp", set())
    if not any(path.lower() in llama for path in SEMANTIC_SERVER_CHOICES):
        raise ValueError(
            "production semantic runtime is missing an application-resolvable "
            f"llama.cpp server: expected one of {list(SEMANTIC_SERVER_CHOICES)}"
        )

    models = sorted(
        path
        for path in tools.get("semantic_model", set())
        if path.lower().startswith(SEMANTIC_MODEL_PREFIX)
        and path.lower().endswith(".gguf")
    )
    if len(models) != 1:
        raise ValueError(
            "production semantic runtime must contain exactly one deterministic "
            f"semantic_model/*.gguf; found {len(models)}"
        )


def verify_offline_resource_mapping(config_path: Path = OFFLINE_TAURI_CONFIG) -> None:
    data = json.loads(config_path.read_text("utf-8"))
    resources = data.get("bundle", {}).get("resources")
    if not isinstance(resources, dict):
        raise ValueError(
            "offline Tauri bundle.resources must be a source->target map so the "
            "staging target prefix is not installed into the runtime search path"
        )
    if resources.get(RESOURCE_SOURCE) != RESOURCE_TARGET:
        raise ValueError(
            "offline Tauri resource mapping must install "
            f"{RESOURCE_SOURCE!r} at {RESOURCE_TARGET!r}"
        )


def verify_status(status: dict[str, Any]) -> None:
    tools = paths_by_tool(status)
    verify_entry_points(tools)
    verify_offline_resource_mapping()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default=WINDOWS_TARGET)
    args = parser.parse_args()
    status = load_status(args.target)
    verify_status(status)
    print(
        "WINDOWS RUNTIME APP PARITY PASSED: "
        f"target={args.target}; tools={len(paths_by_tool(status))}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}")
        raise SystemExit(1)
