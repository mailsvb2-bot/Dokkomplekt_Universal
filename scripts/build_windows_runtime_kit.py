#!/usr/bin/env python3
"""Build a complete, supply-chain-locked Windows runtime manifest from reviewed local trees.

This command deliberately performs no network access and never downloads runtime
components. A release engineer points it at complete local portable trees for
each required component. The builder recursively inventories every regular file,
rejects symlinks/junctions/path escapes, emits a create_runtime_lock.py compatible
catalog and exact distribution inventory, then creates the immutable runner-owned
manifest consumed by prepare_sidecars.py.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

try:
    from scripts._release_policy import validate_relative_runtime_path, validate_source_reference
    from scripts.create_runtime_lock import REQUIRED_TOOLS, SUPPORTED_TOOLS, build_lock
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path, validate_source_reference
    from create_runtime_lock import REQUIRED_TOOLS, SUPPORTED_TOOLS, build_lock

SCHEMA = "dokkomplekt.windows-runtime-kit.v1"
SPEC_SCHEMA = 1
TARGET = "windows-x86_64"
EXECUTABLE_SUFFIXES = {".exe", ".com", ".cmd", ".bat", ".ps1", ".pl"}
PRODUCTION_REQUIRED_TOOLS = set(REQUIRED_TOOLS)
# Runtime-kit staging must match the layouts searched by the desktop resolver.
# Keeping one canonical root per component prevents a reviewed/locked runtime
# from being marked production-ready while the installed app cannot locate it.
CANONICAL_TARGET_ROOTS = {tool: tool for tool in PRODUCTION_REQUIRED_TOOLS}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def atomic_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    os.replace(temporary, path)


def is_linklike(path: Path) -> bool:
    """Reject POSIX symlinks and Windows directory junction/reparse indirections."""
    if path.is_symlink():
        return True
    is_junction = getattr(path, "is_junction", None)
    return bool(is_junction()) if callable(is_junction) else False


def required_text(raw: dict[str, Any], key: str, label: str) -> str:
    value = str(raw.get(key, "")).strip()
    if not value or value.upper().startswith("REPLACE_"):
        raise ValueError(f"{label}.{key} is required and cannot be a placeholder")
    return value


def resolve_path(base: Path, value: Any, label: str, *, directory: bool) -> Path:
    raw = os.path.expandvars(os.path.expanduser(str(value or "")))
    path = Path(raw)
    if not path.is_absolute():
        path = (base / path).resolve()
    if not path.exists():
        raise FileNotFoundError(f"{label} does not exist: {path}")
    if is_linklike(path):
        raise ValueError(f"{label} must not be a symlink or junction: {path}")
    if directory:
        if not path.is_dir():
            raise ValueError(f"{label} must be a directory: {path}")
    elif not path.is_file():
        raise ValueError(f"{label} must be a regular file: {path}")
    return path.resolve()


def parse_review(data: dict[str, Any]) -> dict[str, str]:
    raw = data.get("review")
    if not isinstance(raw, dict):
        raise ValueError("review must be an object")
    reviewer = required_text(raw, "reviewer", "review")
    reviewed_at = required_text(raw, "reviewed_at", "review")
    scope = required_text(raw, "scope", "review")
    try:
        review_date = dt.date.fromisoformat(reviewed_at)
    except ValueError as exc:
        raise ValueError("review.reviewed_at must be YYYY-MM-DD") from exc
    if review_date > dt.date.today():
        raise ValueError("review.reviewed_at cannot be in the future")
    return {"reviewer": reviewer, "reviewed_at": reviewed_at, "scope": scope}


def enumerate_tree(root: Path, target_root: str, tool: str) -> list[tuple[Path, str]]:
    output: list[tuple[Path, str]] = []
    root_resolved = root.resolve()
    for candidate in sorted(root.rglob("*"), key=lambda item: item.as_posix().lower()):
        if is_linklike(candidate):
            raise ValueError(
                f"{tool} portable tree contains a symlink or junction: {candidate}"
            )
        resolved = candidate.resolve()
        try:
            resolved.relative_to(root_resolved)
        except ValueError as exc:
            raise ValueError(
                f"{tool} portable tree escapes its reviewed root: {candidate}"
            ) from exc
        if candidate.is_dir():
            continue
        if not candidate.is_file():
            raise ValueError(
                f"{tool} portable tree contains a non-regular entry: {candidate}"
            )
        relative = resolved.relative_to(root_resolved)
        target = validate_relative_runtime_path(
            (Path(target_root) / relative).as_posix(), f"{tool} runtime target"
        )
        output.append((resolved, target))
    if not output:
        raise ValueError(f"{tool} portable tree is empty: {root}")
    return output


def build_catalog(
    spec_path: Path, output_dir: Path
) -> tuple[dict[str, Any], dict[str, Any]]:
    data = json.loads(spec_path.read_text(encoding="utf-8"))
    if data.get("schema") != SPEC_SCHEMA:
        raise ValueError("runtime-kit spec schema must be 1")
    if str(data.get("target", "")).strip() != TARGET:
        raise ValueError(f"runtime-kit target must be {TARGET}")
    review = parse_review(data)
    components = data.get("components")
    if not isinstance(components, list) or not components:
        raise ValueError("runtime-kit spec must contain a non-empty components array")

    artifacts: list[dict[str, Any]] = []
    inventory_tools: dict[str, list[str]] = {}
    seen_tools: set[str] = set()
    seen_targets: set[str] = set()
    component_report: list[dict[str, Any]] = []

    for index, raw in enumerate(components):
        if not isinstance(raw, dict):
            raise ValueError(f"components[{index}] must be an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if tool not in SUPPORTED_TOOLS:
            raise ValueError(f"components[{index}].tool is unsupported: {tool!r}")
        if tool in seen_tools:
            raise ValueError(
                f"runtime-kit spec must declare exactly one reviewed root per tool: {tool}"
            )
        seen_tools.add(tool)

        root = resolve_path(
            spec_path.parent,
            raw.get("root"),
            f"components[{index}].root",
            directory=True,
        )
        license_file = resolve_path(
            spec_path.parent,
            raw.get("license_file"),
            f"components[{index}].license_file",
            directory=False,
        )
        target_root = validate_relative_runtime_path(
            required_text(raw, "target_root", f"components[{index}]"),
            f"components[{index}].target_root",
        )
        canonical_root = CANONICAL_TARGET_ROOTS.get(tool)
        if canonical_root is None:
            raise ValueError(f"no desktop runtime layout contract exists for tool: {tool}")
        if target_root.replace("\\", "/").rstrip("/").lower() != canonical_root.lower():
            raise ValueError(
                f"components[{index}].target_root for {tool} must be {canonical_root!r}; "
                "arbitrary runtime roots are not discoverable by the desktop resolver"
            )
        version = required_text(raw, "version", f"components[{index}]")
        source_url = validate_source_reference(
            required_text(raw, "source_url", f"components[{index}]"),
            f"components[{index}].source_url",
        )
        license_name = required_text(raw, "license", f"components[{index}]")

        tree = enumerate_tree(root, target_root, tool)
        inventory_tools[tool] = []
        total_bytes = 0
        for source, target in tree:
            if target in seen_targets:
                raise ValueError(f"duplicate runtime target across components: {target}")
            seen_targets.add(target)
            inventory_tools[tool].append(target)
            total_bytes += source.stat().st_size
            artifacts.append(
                {
                    "tool": tool,
                    "source": str(source),
                    "target": target,
                    "executable": source.suffix.lower() in EXECUTABLE_SUFFIXES,
                    "version": version,
                    "source_url": source_url,
                    "license": license_name,
                    "license_file": str(license_file),
                }
            )
        component_report.append(
            {
                "tool": tool,
                "root": str(root),
                "target_root": target_root,
                "files": len(tree),
                "bytes": total_bytes,
                "version": version,
                "source_url": source_url,
                "license": license_name,
                "license_file": str(license_file),
                "license_sha256": sha256_file(license_file),
            }
        )

    missing = sorted(PRODUCTION_REQUIRED_TOOLS - seen_tools)
    extra = sorted(seen_tools - PRODUCTION_REQUIRED_TOOLS)
    if missing or extra:
        raise ValueError(
            "runtime-kit component set must exactly match production requirements; "
            f"missing={missing}; extra={extra}"
        )

    inventory = {
        "schema": 1,
        "target": TARGET,
        "generated_by": "scripts/build_windows_runtime_kit.py",
        "tools": {tool: inventory_tools[tool] for tool in sorted(inventory_tools)},
    }
    inventory_path = output_dir / "runtime-inventory.json"
    atomic_json(inventory_path, inventory)

    catalog = {
        "schema": 1,
        "target": TARGET,
        "artifacts": artifacts,
        "distribution_review": {
            "complete_portable_tree": True,
            "reviewer": review["reviewer"],
            "reviewed_at": review["reviewed_at"],
            "scope": review["scope"],
            "inventory_file": str(inventory_path.resolve()),
        },
    }
    report = {
        "schema": SCHEMA,
        "target": TARGET,
        "network_used": False,
        "complete_portable_tree": True,
        "review": review,
        "components": component_report,
        "component_count": len(component_report),
        "file_count": len(artifacts),
        "total_bytes": sum(item["bytes"] for item in component_report),
    }
    return catalog, report


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Create a production Windows runtime catalog, inventory and immutable "
            "lock from reviewed local portable trees."
        )
    )
    parser.add_argument("spec", type=Path)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()

    spec_path = args.spec.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    catalog, report = build_catalog(spec_path, output_dir)
    catalog_path = output_dir / "runtime-catalog.json"
    atomic_json(catalog_path, catalog)

    lock = build_lock(catalog_path)
    lock_path = output_dir / "windows-x86_64-manifest.json"
    atomic_json(lock_path, lock)

    inventory_path = output_dir / "runtime-inventory.json"
    report.update(
        {
            "catalog_path": str(catalog_path),
            "catalog_sha256": sha256_file(catalog_path),
            "inventory_path": str(inventory_path),
            "inventory_sha256": sha256_file(inventory_path),
            "manifest_path": str(lock_path),
            "manifest_sha256": sha256_file(lock_path),
            "supply_chain_locked": lock.get("supply_chain_locked") is True,
        }
    )
    report_path = output_dir / "RUNTIME_KIT_REPORT.json"
    atomic_json(report_path, report)

    print(
        "WINDOWS PRODUCTION RUNTIME KIT LOCKED: "
        f"components={report['component_count']}; files={report['file_count']}; "
        f"manifest={lock_path}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
