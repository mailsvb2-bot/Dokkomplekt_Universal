#!/usr/bin/env python3
"""Prove a staged production Windows runtime is launchable by the desktop app.

The gate is profile-aware: ``core`` verifies the exact document-processing
surface embedded by the stock installer, while ``full`` additionally requires
one llama.cpp server and exactly one GGUF model. Outlook MSG is parsed inside
the Rust core and therefore never appears as an external sidecar.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

try:
    from scripts._release_policy import validate_relative_runtime_path
    from scripts._runtime_profile import (
        CORE_PROFILE,
        FULL_PROFILE,
        PROFILES,
        normalize_profile,
        profile_tools,
    )
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path
    from _runtime_profile import CORE_PROFILE, FULL_PROFILE, PROFILES, normalize_profile, profile_tools

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
OFFLINE_TAURI_CONFIG = ROOT / "src-tauri" / "tauri.offline.conf.json"
WINDOWS_TARGET = "windows-x86_64"
RESOURCE_SOURCE = f"resources/tools/{WINDOWS_TARGET}/"
RESOURCE_TARGET = "resources/tools/"

EXPECTED_RUNTIME_TOOLS = set(profile_tools(FULL_PROFILE))
CORE_RUNTIME_TOOLS = set(profile_tools(CORE_PROFILE))

REQUIRED_EXACT_PATHS: dict[str, tuple[str, ...]] = {
    "tesseract": ("tesseract/tesseract.exe",),
    "poppler": ("poppler/bin/pdftotext.exe", "poppler/bin/pdftoppm.exe"),
    "libreoffice": ("libreoffice/program/soffice.exe",),
    "sumatrapdf": ("sumatrapdf/SumatraPDF.exe",),
    "7zip": ("7zip/7z.exe",),
}
SEMANTIC_SERVER_CHOICES = ("llama_cpp/llama-server.exe", "llama_cpp/server.exe")
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


def selected_profile(status: dict[str, Any], requested: str | None) -> str:
    if requested:
        profile = normalize_profile(requested, semantic_model_required=(requested == FULL_PROFILE))
    else:
        declared = status.get("runtime_profile")
        if declared:
            profile = normalize_profile(
                declared,
                semantic_model_required=status.get("semantic_model_required"),
            )
        else:
            # Compatibility for older callers/tests whose runtime surface was full.
            profile = FULL_PROFILE
    declared = status.get("runtime_profile")
    if declared:
        actual = normalize_profile(
            declared,
            semantic_model_required=status.get("semantic_model_required"),
        )
        if actual != profile:
            raise ValueError(f"runtime profile mismatch: expected {profile!r}, staged {actual!r}")
    return profile


def paths_by_tool(status: dict[str, Any]) -> dict[str, set[str]]:
    tools: dict[str, set[str]] = {}
    for index, raw in enumerate(status.get("files", [])):
        if not isinstance(raw, dict):
            raise ValueError(f"files[{index}] must be an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if not tool:
            raise ValueError(f"files[{index}].tool is empty")
        if tool not in EXPECTED_RUNTIME_TOOLS:
            raise ValueError(f"unsupported external Windows runtime component: {tool}")
        path = safe_relative(raw.get("path", ""))
        canonical_prefix = f"{tool}/"
        if not path.lower().startswith(canonical_prefix.lower()):
            raise ValueError(
                f"{tool} is staged outside its application-resolvable root: {path}; "
                f"expected prefix {canonical_prefix}"
            )
        tools.setdefault(tool, set()).add(path)
    return tools


def verify_entry_points(tools: dict[str, set[str]], profile: str = FULL_PROFILE) -> None:
    expected_tools = set(profile_tools(profile))
    actual_tools = set(tools)
    if actual_tools != expected_tools:
        missing = sorted(expected_tools - actual_tools)
        extra = sorted(actual_tools - expected_tools)
        raise ValueError(
            "production Windows runtime component set does not match the application contract: "
            f"profile={profile}; missing={missing}; extra={extra}"
        )

    lower_tools = {tool: {path.lower() for path in paths} for tool, paths in tools.items()}
    for tool, required in REQUIRED_EXACT_PATHS.items():
        available = lower_tools.get(tool, set())
        for path in required:
            if path.lower() not in available:
                raise ValueError(f"production runtime is not launchable by the app: missing {path}")

    if profile == FULL_PROFILE:
        llama = lower_tools.get("llama_cpp", set())
        if not any(path.lower() in llama for path in SEMANTIC_SERVER_CHOICES):
            raise ValueError(
                "production semantic runtime is missing an application-resolvable "
                f"llama.cpp server: expected one of {list(SEMANTIC_SERVER_CHOICES)}"
            )
        models = sorted(
            path for path in tools.get("semantic_model", set())
            if path.lower().startswith(SEMANTIC_MODEL_PREFIX) and path.lower().endswith(".gguf")
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


def verify_status(status: dict[str, Any], profile: str | None = None) -> None:
    resolved = selected_profile(status, profile)
    tools = paths_by_tool(status)
    verify_entry_points(tools, resolved)
    verify_offline_resource_mapping()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default=WINDOWS_TARGET)
    parser.add_argument("--profile", choices=PROFILES)
    args = parser.parse_args()
    status = load_status(args.target)
    profile = selected_profile(status, args.profile)
    verify_entry_points(paths_by_tool(status), profile)
    verify_offline_resource_mapping()
    print(f"WINDOWS RUNTIME/APP PARITY OK: target={args.target}; profile={profile}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=__import__('sys').stderr)
        raise SystemExit(1)
