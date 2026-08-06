#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import json
import os
import re
from pathlib import Path

try:
    from scripts._release_policy import validate_public_https_url, validate_source_reference
except ModuleNotFoundError:
    from _release_policy import validate_public_https_url, validate_source_reference

PRODUCTION_BUILD_REQUIRED = (
    "DOKKOMPLEKT_GATE_PUBKEY_B64",
    "DOKKOMPLEKT_LICENSE_PUBKEY_B64",
    "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
    "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64",
    "DOKKOMPLEKT_REFDATA_PUBKEY_B64",
    "DOKKOMPLEKT_UPDATE_MANIFEST_URL",
    "DOKKOMPLEKT_REFDATA_URL",
    "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
    "DOKKOMPLEKT_COMPONENTS_BASE_URL",
)
RUNTIME_REQUIRED = PRODUCTION_BUILD_REQUIRED + (
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
    "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
    "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64",
    "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
    "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH",
)
HARDWARE_REQUIRED = (
    "DOKKOMPLEKT_TEST_PRINTER",
    "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH",
)
BASE64_VARS = {
    "DOKKOMPLEKT_GATE_PUBKEY_B64",
    "DOKKOMPLEKT_LICENSE_PUBKEY_B64",
    "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
    "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64",
    "DOKKOMPLEKT_REFDATA_PUBKEY_B64",
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
    "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
    "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64",
    "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
}
ED25519_PUBLIC_VARS = {
    "DOKKOMPLEKT_GATE_PUBKEY_B64",
    "DOKKOMPLEKT_LICENSE_PUBKEY_B64",
    "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
    "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64",
    "DOKKOMPLEKT_REFDATA_PUBKEY_B64",
}
ED25519_PRIVATE_SEED_VARS = {"DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64"}
URL_VARS = {
    "DOKKOMPLEKT_UPDATE_MANIFEST_URL",
    "DOKKOMPLEKT_REFDATA_URL",
    "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
    "DOKKOMPLEKT_COMPONENTS_BASE_URL",
}
SUPPORTED_RUNTIME_TOOLS = {
    "tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip",
    "msgconvert", "llama_cpp", "semantic_model",
}
REQUIRED_RUNTIME_TOOLS = {
    "tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip",
    "llama_cpp", "semantic_model",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def resolve_manifest_file(manifest_path: Path, raw: object) -> Path:
    path = Path(os.path.expandvars(os.path.expanduser(str(raw or ""))))
    if not path.is_absolute():
        path = (manifest_path.parent / path).resolve()
    return path


def safe_relative(raw: object) -> str:
    normalized = str(raw or "").replace("\\", "/")
    path = Path(normalized)
    if (
        path.is_absolute()
        or normalized.startswith("//")
        or re.match(r"^[A-Za-z]:", normalized)
        or not path.parts
        or ".." in path.parts
    ):
        raise ValueError("unsafe relative target")
    return path.as_posix()


def validate_runner_manifest(path: Path) -> list[str]:
    errors: list[str] = []
    label = "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH"
    try:
        data = json.loads(path.read_text("utf-8"))
    except Exception as exc:
        return [f"{label}: invalid JSON: {exc}"]
    if not isinstance(data, dict) or data.get("schema") != 1:
        errors.append(f"{label}: manifest schema must be 1")
    if data.get("target") != "windows-x86_64":
        errors.append(f"{label}: target must be windows-x86_64")
    if data.get("supply_chain_locked") is not True:
        errors.append(f"{label}: supply_chain_locked must be true")
    files = data.get("files")
    if not isinstance(files, list) or not files:
        errors.append(f"{label}: files must be a non-empty array")
        return errors

    tools: set[str] = set()
    targets: dict[str, set[str]] = {}
    seen_targets: set[str] = set()
    for index, raw in enumerate(files):
        prefix = f"{label}: files[{index}]"
        if not isinstance(raw, dict):
            errors.append(f"{prefix} must be an object")
            continue
        tool = str(raw.get("tool", "")).strip().lower()
        if tool not in SUPPORTED_RUNTIME_TOOLS:
            errors.append(f"{prefix}.tool is unsupported")
        else:
            tools.add(tool)
        try:
            target = safe_relative(raw.get("target"))
        except ValueError:
            errors.append(f"{prefix}.target is unsafe")
            target = ""
        if target:
            if target in seen_targets:
                errors.append(f"{prefix}.target is duplicated")
            seen_targets.add(target)
            targets.setdefault(tool, set()).add(target)
        digest = str(raw.get("sha256", "")).strip()
        if not SHA256_RE.fullmatch(digest):
            errors.append(f"{prefix}.sha256 must be a lowercase SHA-256 digest")
        for key in ("version", "source_url", "license", "license_file", "license_sha256"):
            value = str(raw.get(key, "")).strip()
            if not value or value.upper().startswith("REPLACE_"):
                errors.append(f"{prefix}.{key} is required and cannot be a placeholder")
        source_url = str(raw.get("source_url", "")).strip()
        if source_url:
            try:
                validate_source_reference(source_url, f"{prefix}.source_url")
            except ValueError as exc:
                errors.append(str(exc))
        source = resolve_manifest_file(path, raw.get("source"))
        if not source.is_file():
            errors.append(f"{prefix}.source does not exist")
        elif SHA256_RE.fullmatch(digest) and sha256_file(source) != digest:
            errors.append(f"{prefix}.sha256 mismatch")
        license_file = resolve_manifest_file(path, raw.get("license_file"))
        if not license_file.is_file():
            errors.append(f"{prefix}.license_file does not exist")
        license_digest = str(raw.get("license_sha256", "")).strip()
        if license_file.is_file() and SHA256_RE.fullmatch(license_digest):
            if sha256_file(license_file) != license_digest:
                errors.append(f"{prefix}.license_sha256 mismatch")

    missing = sorted(REQUIRED_RUNTIME_TOOLS - tools)
    if missing:
        errors.append(f"{label}: runtime is incomplete; missing tools: {missing}")
    if not any(
        str(raw.get("tool", "")).strip().lower() == "semantic_model"
        and str(raw.get("target", "")).lower().endswith(".gguf")
        for raw in files if isinstance(raw, dict)
    ):
        errors.append(f"{label}: semantic_model must include a GGUF file")

    review = data.get("distribution_review")
    if not isinstance(review, dict) or review.get("complete_portable_tree") is not True:
        errors.append(f"{label}: reviewed complete portable-tree inventory is required")
        return errors
    for key in ("reviewer", "reviewed_at", "scope", "inventory_file", "inventory_sha256"):
        value = str(review.get(key, "")).strip()
        if not value or value.upper().startswith("REPLACE_"):
            errors.append(f"{label}: distribution_review.{key} is required")
    reviewed_at = str(review.get("reviewed_at", "")).strip()
    if reviewed_at:
        try:
            if dt.date.fromisoformat(reviewed_at) > dt.date.today():
                errors.append(f"{label}: distribution_review.reviewed_at cannot be in the future")
        except ValueError:
            errors.append(f"{label}: distribution_review.reviewed_at must be YYYY-MM-DD")
    inventory = resolve_manifest_file(path, review.get("inventory_file"))
    inventory_digest = str(review.get("inventory_sha256", "")).strip()
    if not inventory.is_file():
        errors.append(f"{label}: distribution inventory does not exist")
    elif not SHA256_RE.fullmatch(inventory_digest) or sha256_file(inventory) != inventory_digest:
        errors.append(f"{label}: distribution inventory SHA-256 mismatch")
    else:
        try:
            inventory_data = json.loads(inventory.read_text("utf-8"))
            declared = inventory_data.get("tools") if inventory_data.get("schema") == 1 else None
            if not isinstance(declared, dict):
                raise ValueError("schema/tools")
            normalized: dict[str, set[str]] = {}
            for tool, raw_paths in declared.items():
                if not isinstance(raw_paths, list) or not raw_paths:
                    raise ValueError(f"invalid inventory for {tool}")
                paths = {safe_relative(item) for item in raw_paths}
                if len(paths) != len(raw_paths):
                    raise ValueError(f"duplicate inventory path for {tool}")
                normalized[str(tool)] = paths
            if normalized != targets:
                errors.append(f"{label}: distribution inventory does not exactly match files")
        except Exception as exc:
            errors.append(f"{label}: invalid distribution inventory: {exc}")
    return errors


def check(mode: str, env: dict[str, str]) -> dict[str, object]:
    required = {
        "production-build": PRODUCTION_BUILD_REQUIRED,
        "windows-runtime": RUNTIME_REQUIRED,
        "windows-hardware": HARDWARE_REQUIRED,
    }[mode]
    errors: list[str] = []
    checked: list[str] = []
    for name in required:
        value = env.get(name, "").strip()
        checked.append(name)
        if not value:
            errors.append(f"{name}: missing")
            continue
        if name in BASE64_VARS:
            try:
                decoded = base64.b64decode(value, validate=True)
                if not decoded:
                    errors.append(f"{name}: decoded value is empty")
                elif name in ED25519_PUBLIC_VARS and len(decoded) != 32:
                    errors.append(f"{name}: Ed25519 public key must be 32 bytes")
                elif name in ED25519_PRIVATE_SEED_VARS and len(decoded) != 32:
                    errors.append(f"{name}: Ed25519 private seed must be 32 bytes")
            except Exception:
                errors.append(f"{name}: invalid base64")
        if name in URL_VARS:
            try:
                validate_public_https_url(value, name)
            except ValueError as exc:
                errors.append(str(exc))
        if name == "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH":
            path = Path(value)
            if not path.is_absolute():
                errors.append(f"{name}: must be an absolute runner-owned path")
            elif not path.is_file():
                errors.append(f"{name}: file does not exist")
            else:
                errors.extend(validate_runner_manifest(path))
        if name == "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH":
            path = Path(value)
            if not path.is_absolute():
                errors.append(f"{name}: must be an absolute path")
    timestamp = env.get("DOKKOMPLEKT_TIMESTAMP_SERVER", "").strip()
    if mode == "windows-runtime" and timestamp:
        try:
            validate_public_https_url(timestamp, "DOKKOMPLEKT_TIMESTAMP_SERVER")
        except ValueError as exc:
            errors.append(str(exc))
    return {
        "schema": "dokkomplekt.release-environment-preflight.v1",
        "mode": mode,
        "ok": not errors,
        "checked": checked,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        choices=("production-build", "windows-runtime", "windows-hardware"),
        required=True,
    )
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()
    report = check(args.mode, dict(os.environ))
    payload = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(payload, encoding="utf-8")
    print(payload, end="")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
