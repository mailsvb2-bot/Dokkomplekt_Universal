#!/usr/bin/env python3
"""Stage verified local sidecar binaries for Tauri packaging.

This script deliberately never downloads anything.  A release engineer supplies a
JSON manifest containing local source paths and exact SHA-256 digests.  Files are
copied only after verification into src-tauri/resources/tools/<target>/, where the
desktop runtime searches for approved OCR, office and message converters.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import sys
from pathlib import Path
from typing import Any

try:
    from scripts._release_policy import validate_relative_runtime_path
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path

ROOT = Path(__file__).resolve().parents[1]
DEST_ROOT = ROOT / "src-tauri" / "resources" / "tools"
SUPPORTED_TOOLS = {
    "tesseract",
    "poppler",
    "libreoffice",
    "sumatrapdf",
    "7zip",
    "msgconvert",
    "llama_cpp",
    "semantic_model",
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def safe_relative(value: str) -> Path:
    return Path(validate_relative_runtime_path(value, "runtime target"))


def load_manifest(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text("utf-8"))
    if data.get("schema") != 1:
        raise ValueError("manifest schema must be 1")
    target = str(data.get("target", "")).strip()
    if not target or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in target):
        raise ValueError("target must be a safe platform-arch identifier")
    entries = data.get("files")
    if not isinstance(entries, list) or not entries:
        raise ValueError("manifest must contain a non-empty files array")
    return data



def stage_distribution_review(
    data: dict[str, Any], manifest_path: Path, destination: Path
) -> dict[str, Any] | None:
    raw = data.get("distribution_review")
    if raw is None:
        return None
    if not isinstance(raw, dict) or raw.get("complete_portable_tree") is not True:
        raise ValueError("distribution_review must assert complete_portable_tree=true")
    for key in ("reviewer", "reviewed_at", "scope", "inventory_file", "inventory_sha256"):
        if not str(raw.get(key, "")).strip():
            raise ValueError(f"distribution_review.{key} is required")
    inventory_source = Path(
        os.path.expandvars(os.path.expanduser(str(raw["inventory_file"])))
    )
    if not inventory_source.is_absolute():
        inventory_source = (manifest_path.parent / inventory_source).resolve()
    if not inventory_source.is_file():
        raise FileNotFoundError(f"distribution inventory does not exist: {inventory_source}")
    expected = str(raw["inventory_sha256"]).strip().lower()
    actual = sha256_file(inventory_source)
    if actual != expected:
        raise ValueError(
            f"distribution inventory SHA-256 mismatch: expected {expected}, got {actual}"
        )
    inventory_relative = Path("_evidence") / f"runtime-inventory-{actual[:16]}.json"
    inventory_output = destination / inventory_relative
    inventory_output.parent.mkdir(parents=True, exist_ok=True)
    temporary = inventory_output.with_name(inventory_output.name + ".tmp")
    shutil.copy2(inventory_source, temporary)
    if sha256_file(temporary) != expected:
        temporary.unlink(missing_ok=True)
        raise ValueError("staged distribution inventory changed during copy")
    os.replace(temporary, inventory_output)
    return {
        "complete_portable_tree": True,
        "reviewer": str(raw["reviewer"]).strip(),
        "reviewed_at": str(raw["reviewed_at"]).strip(),
        "scope": str(raw["scope"]).strip(),
        "inventory_path": inventory_relative.as_posix(),
        "inventory_sha256": actual,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--clean", action="store_true", help="replace the target directory")
    args = parser.parse_args()

    manifest_path = args.manifest.resolve()
    data = load_manifest(manifest_path)
    target = data["target"]
    destination = DEST_ROOT / target
    if args.clean and destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)

    staged: list[dict[str, Any]] = []
    for index, raw in enumerate(data["files"]):
        if not isinstance(raw, dict):
            raise ValueError(f"files[{index}] must be an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if tool not in SUPPORTED_TOOLS:
            raise ValueError(f"files[{index}].tool is unsupported: {tool!r}")
        source = Path(os.path.expandvars(os.path.expanduser(str(raw.get("source", "")))))
        if not source.is_absolute():
            source = (manifest_path.parent / source).resolve()
        if not source.is_file():
            raise FileNotFoundError(f"sidecar source does not exist: {source}")
        expected = str(raw.get("sha256", "")).strip().lower()
        if len(expected) != 64 or any(c not in "0123456789abcdef" for c in expected):
            raise ValueError(f"files[{index}].sha256 must be a lowercase SHA-256 digest")
        actual = sha256_file(source)
        if actual != expected:
            raise ValueError(f"SHA-256 mismatch for {source.name}: expected {expected}, got {actual}")
        relative = safe_relative(str(raw.get("target", "")))
        output = destination / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        temporary = output.with_name(output.name + ".tmp")
        shutil.copy2(source, temporary)
        copied_sha256 = sha256_file(temporary)
        if copied_sha256 != expected:
            temporary.unlink(missing_ok=True)
            raise ValueError(
                f"staged copy SHA-256 mismatch for {source.name}: expected {expected}, got {copied_sha256}"
            )
        os.replace(temporary, output)
        if bool(raw.get("executable", True)) and os.name != "nt":
            output.chmod(output.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        staged_entry = {
            "tool": tool,
            "path": relative.as_posix(),
            "sha256": actual,
            "size_bytes": output.stat().st_size,
        }
        if data.get("supply_chain_locked") is True:
            for metadata_key in ("version", "source_url", "license", "license_file", "license_sha256"):
                value = str(raw.get(metadata_key, "")).strip()
                if not value:
                    raise ValueError(f"files[{index}] is missing supply-chain field {metadata_key}")
            license_source = Path(os.path.expandvars(os.path.expanduser(str(raw["license_file"]))))
            if not license_source.is_absolute():
                license_source = (manifest_path.parent / license_source).resolve()
            if not license_source.is_file():
                raise FileNotFoundError(f"license notice does not exist: {license_source}")
            license_expected = str(raw["license_sha256"]).strip().lower()
            license_actual = sha256_file(license_source)
            if license_actual != license_expected:
                raise ValueError(
                    f"license SHA-256 mismatch for {license_source.name}: expected {license_expected}, got {license_actual}"
                )
            safe_license_name = "".join(
                character if character.isalnum() or character in "._-" else "_"
                for character in license_source.name
            ) or "LICENSE.txt"
            license_relative = Path("_licenses") / f"{license_actual[:16]}-{safe_license_name}"
            license_output = destination / license_relative
            license_output.parent.mkdir(parents=True, exist_ok=True)
            if not license_output.exists():
                license_temporary = license_output.with_name(license_output.name + ".tmp")
                shutil.copy2(license_source, license_temporary)
                if sha256_file(license_temporary) != license_expected:
                    license_temporary.unlink(missing_ok=True)
                    raise ValueError("staged license notice changed during copy")
                os.replace(license_temporary, license_output)
            elif sha256_file(license_output) != license_expected:
                raise ValueError(f"existing staged license notice is corrupted: {license_output}")
            staged_entry.update({
                "version": str(raw["version"]).strip(),
                "source_url": str(raw["source_url"]).strip(),
                "license": str(raw["license"]).strip(),
                "license_path": license_relative.as_posix(),
                "license_sha256": license_actual,
            })
        staged.append(staged_entry)

    distribution_review = stage_distribution_review(data, manifest_path, destination)
    status = {
        "schema": 1,
        "target": target,
        "generated_by": "scripts/prepare_sidecars.py",
        "network_used": False,
        "supply_chain_locked": data.get("supply_chain_locked") is True,
        "files": staged,
    }
    if distribution_review is not None:
        status["distribution_review"] = distribution_review
    (destination / "sidecar-status.json").write_text(
        json.dumps(status, ensure_ascii=False, indent=2) + "\n", "utf-8"
    )
    print(f"Staged {len(staged)} verified sidecar files in {destination}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
