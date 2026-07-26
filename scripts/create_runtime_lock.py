#!/usr/bin/env python3
"""Create a hash- and provenance-locked offline runtime manifest.

The catalog is reviewed input; this command never downloads software. Every
runtime artifact and its license notice must already exist locally. The emitted
JSON remains compatible with prepare_sidecars.py while adding immutable version,
origin and license evidence used by the production release gate.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

SUPPORTED_TOOLS = {
    "tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip",
    "msgconvert", "llama_cpp", "semantic_model",
}
REQUIRED_TOOLS = {
    "tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip",
    "llama_cpp", "semantic_model",
}
SAFE_TARGET = re.compile(r"^[A-Za-z0-9_-]+$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def resolve(base: Path, value: Any, title: str) -> Path:
    raw = os.path.expandvars(os.path.expanduser(str(value or "")))
    path = Path(raw)
    if not path.is_absolute():
        path = (base / path).resolve()
    if not path.is_file():
        raise FileNotFoundError(f"{title} does not exist: {path}")
    return path


def safe_relative(value: Any) -> str:
    path = Path(str(value or "").replace("\\", "/"))
    if path.is_absolute() or not path.parts or ".." in path.parts:
        raise ValueError(f"unsafe target path: {value!r}")
    return path.as_posix()


def required_text(raw: dict[str, Any], key: str, index: int) -> str:
    value = str(raw.get(key, "")).strip()
    if not value or value.upper().startswith("REPLACE_"):
        raise ValueError(f"artifacts[{index}].{key} is required and cannot be a placeholder")
    return value


def validated_source_url(raw: dict[str, Any], index: int) -> str:
    value = required_text(raw, "source_url", index)
    parsed = urlparse(value)
    if parsed.scheme == "https":
        if not parsed.hostname or parsed.username or parsed.password or parsed.fragment:
            raise ValueError(f"artifacts[{index}].source_url must be a clean HTTPS URL")
    elif parsed.scheme == "urn":
        if not parsed.path:
            raise ValueError(f"artifacts[{index}].source_url contains an empty URN")
    else:
        raise ValueError(
            f"artifacts[{index}].source_url must use https:// or an approved urn: identifier"
        )
    return value


def build_distribution_review(
    data: dict[str, Any], catalog_path: Path, artifacts: list[dict[str, Any]]
) -> dict[str, Any] | None:
    raw = data.get("distribution_review")
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise ValueError("distribution_review must be an object")
    if raw.get("complete_portable_tree") is not True:
        raise ValueError("distribution_review.complete_portable_tree must be true")
    reviewer = str(raw.get("reviewer", "")).strip()
    scope = str(raw.get("scope", "")).strip()
    reviewed_at = str(raw.get("reviewed_at", "")).strip()
    for key, value in (("reviewer", reviewer), ("scope", scope), ("reviewed_at", reviewed_at)):
        if not value or value.upper().startswith("REPLACE_"):
            raise ValueError(f"distribution_review.{key} is required and cannot be a placeholder")
    try:
        review_date = dt.date.fromisoformat(reviewed_at)
    except ValueError as exc:
        raise ValueError("distribution_review.reviewed_at must be YYYY-MM-DD") from exc
    if review_date > dt.date.today():
        raise ValueError("distribution_review.reviewed_at cannot be in the future")

    inventory_file = resolve(
        catalog_path.parent, raw.get("inventory_file"), "distribution_review.inventory_file"
    )
    inventory = json.loads(inventory_file.read_text("utf-8"))
    if inventory.get("schema") != 1 or not isinstance(inventory.get("tools"), dict):
        raise ValueError("distribution inventory must use schema=1 and contain a tools object")

    expected: dict[str, set[str]] = {}
    for artifact in artifacts:
        expected.setdefault(str(artifact["tool"]), set()).add(str(artifact["target"]))
    declared_tools = inventory["tools"]
    if set(declared_tools) != set(expected):
        raise ValueError(
            "distribution inventory tool set does not match runtime artifacts: "
            f"expected={sorted(expected)}, declared={sorted(declared_tools)}"
        )
    for tool, expected_paths in expected.items():
        raw_paths = declared_tools.get(tool)
        if not isinstance(raw_paths, list) or not raw_paths:
            raise ValueError(f"distribution inventory for {tool} must be a non-empty array")
        declared_paths = {safe_relative(value) for value in raw_paths}
        if len(declared_paths) != len(raw_paths):
            raise ValueError(f"distribution inventory for {tool} contains duplicate paths")
        if declared_paths != expected_paths:
            missing = sorted(expected_paths - declared_paths)
            extra = sorted(declared_paths - expected_paths)
            raise ValueError(
                f"distribution inventory mismatch for {tool}; missing={missing}; extra={extra}"
            )

    return {
        "complete_portable_tree": True,
        "reviewer": reviewer,
        "reviewed_at": reviewed_at,
        "scope": scope,
        "inventory_file": str(inventory_file),
        "inventory_sha256": sha256_file(inventory_file),
    }


def build_lock(catalog_path: Path) -> dict[str, Any]:
    data = json.loads(catalog_path.read_text("utf-8"))
    if data.get("schema") != 1:
        raise ValueError("catalog schema must be 1")
    target = str(data.get("target", "")).strip()
    if not SAFE_TARGET.fullmatch(target):
        raise ValueError("target must be a safe platform-arch identifier")
    artifacts = data.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        raise ValueError("catalog must contain artifacts")

    output: list[dict[str, Any]] = []
    seen_targets: set[str] = set()
    tools: set[str] = set()
    for index, raw in enumerate(artifacts):
        if not isinstance(raw, dict):
            raise ValueError(f"artifacts[{index}] must be an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if tool not in SUPPORTED_TOOLS:
            raise ValueError(f"artifacts[{index}].tool is unsupported: {tool!r}")
        source = resolve(catalog_path.parent, raw.get("source"), f"artifacts[{index}].source")
        license_file = resolve(
            catalog_path.parent, raw.get("license_file"), f"artifacts[{index}].license_file"
        )
        target_path = safe_relative(raw.get("target"))
        if target_path in seen_targets:
            raise ValueError(f"duplicate runtime target: {target_path}")
        seen_targets.add(target_path)
        tools.add(tool)
        output.append({
            "tool": tool,
            "source": str(source),
            "target": target_path,
            "sha256": sha256_file(source),
            "executable": bool(raw.get("executable", True)),
            "version": required_text(raw, "version", index),
            "source_url": validated_source_url(raw, index),
            "license": required_text(raw, "license", index),
            "license_file": str(license_file),
            "license_sha256": sha256_file(license_file),
        })
    missing = sorted(REQUIRED_TOOLS - tools)
    if missing:
        raise ValueError(f"runtime catalog is incomplete; missing tools: {missing}")
    if not any(item["tool"] == "semantic_model" and item["target"].lower().endswith(".gguf") for item in output):
        raise ValueError("semantic_model must include a GGUF file")
    lock = {
        "schema": 1,
        "target": target,
        "supply_chain_locked": True,
        "generated_by": "scripts/create_runtime_lock.py",
        "files": output,
    }
    distribution_review = build_distribution_review(data, catalog_path, output)
    if distribution_review is not None:
        lock["distribution_review"] = distribution_review
    return lock


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("catalog", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    lock = build_lock(args.catalog.resolve())
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(lock, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(f"RUNTIME LOCK CREATED: {output}; files={len(lock['files'])}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
