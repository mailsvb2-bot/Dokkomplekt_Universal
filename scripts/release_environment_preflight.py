#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import csv
import ctypes
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

try:
    from scripts._release_policy import (
        validate_public_https_url,
        validate_relative_runtime_path,
        validate_source_reference,
    )
except ModuleNotFoundError:
    from _release_policy import (
        validate_public_https_url,
        validate_relative_runtime_path,
        validate_source_reference,
    )

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
WINDOWS_RUNTIME_ROOT = Path(r"C:\ProgramData\DokkomplektRuntime")
WINDOWS_RUNTIME_ACL_EVIDENCE = Path(r"C:\ProgramData\DokkomplektE2E\RUNTIME_SERVICE_ACL.json")
WINDOWS_NETWORK_SERVICE_SID = "S-1-5-20"


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
    return validate_relative_runtime_path(raw, "runtime target")


def _is_reparse_like(path: Path) -> bool:
    if path.is_symlink():
        return True
    is_junction = getattr(path, "is_junction", None)
    return bool(is_junction and is_junction())


def _is_under_root(path: Path, root: Path) -> bool:
    try:
        candidate = os.path.normcase(str(path.resolve(strict=True)))
        base = os.path.normcase(str(root.resolve(strict=True)))
        return os.path.commonpath([candidate, base]) == base
    except (OSError, ValueError):
        return False


def _current_windows_sid() -> str:
    completed = subprocess.run(
        ["whoami", "/user", "/fo", "csv", "/nh"],
        check=True,
        text=True,
        capture_output=True,
    )
    rows = list(csv.reader([completed.stdout.strip()]))
    if len(rows) != 1 or len(rows[0]) < 2:
        raise RuntimeError("whoami /user returned an unexpected result")
    return rows[0][-1].strip()


def _current_windows_session_id() -> int:
    session_id = ctypes.c_uint32()
    ok = ctypes.windll.kernel32.ProcessIdToSessionId(os.getpid(), ctypes.byref(session_id))
    if not ok:
        raise OSError("ProcessIdToSessionId failed")
    return int(session_id.value)


def validate_windows_runtime_service_boundary(
    manifest_path: Path,
    runtime_root: Path,
    acl_evidence_path: Path,
    *,
    current_sid: str,
    session_id: int,
) -> list[str]:
    errors: list[str] = []
    label = "windows-runtime-service-boundary"
    if current_sid != WINDOWS_NETWORK_SERVICE_SID:
        errors.append(
            f"{label}: runtime job must execute as Network Service SID {WINDOWS_NETWORK_SERVICE_SID}; got {current_sid or '<empty>'}"
        )
    if session_id != 0:
        errors.append(f"{label}: runtime job must execute in Windows Session 0; got session {session_id}")

    try:
        root = runtime_root.resolve(strict=True)
    except OSError as exc:
        return errors + [f"{label}: runtime root is unavailable: {exc}"]
    if not root.is_dir() or _is_reparse_like(root):
        errors.append(f"{label}: runtime root must be a direct directory")
        return errors

    boundary_paths: list[tuple[str, Path]] = [("manifest", manifest_path)]
    signature = Path(str(manifest_path) + ".sig")
    boundary_paths.append(("approval signature", signature))
    try:
        data = json.loads(manifest_path.read_text("utf-8"))
    except Exception as exc:
        return errors + [f"{label}: cannot read runtime manifest as service identity: {exc}"]
    files = data.get("files") if isinstance(data, dict) else None
    if isinstance(files, list):
        for index, raw in enumerate(files):
            if not isinstance(raw, dict):
                continue
            boundary_paths.append((f"files[{index}].source", resolve_manifest_file(manifest_path, raw.get("source"))))
            boundary_paths.append((f"files[{index}].license_file", resolve_manifest_file(manifest_path, raw.get("license_file"))))
    review = data.get("distribution_review") if isinstance(data, dict) else None
    if isinstance(review, dict):
        boundary_paths.append(("distribution_review.inventory_file", resolve_manifest_file(manifest_path, review.get("inventory_file"))))

    for name, path in boundary_paths:
        if not path.exists():
            errors.append(f"{label}: {name} is missing")
            continue
        if path.is_dir() or _is_reparse_like(path):
            errors.append(f"{label}: {name} must be a direct regular file")
            continue
        if not _is_under_root(path, root):
            errors.append(f"{label}: {name} escapes fixed runtime root {root}")
            continue
        try:
            with path.open("rb") as stream:
                stream.read(1)
        except OSError as exc:
            errors.append(f"{label}: Network Service cannot read {name}: {exc}")

    try:
        evidence = json.loads(acl_evidence_path.read_text("utf-8"))
    except Exception as exc:
        errors.append(f"{label}: runtime ACL evidence is unavailable: {exc}")
        return errors
    if evidence.get("schema") != "dokkomplekt.runtime-service-acl.v2":
        errors.append(f"{label}: runtime ACL evidence schema mismatch")
    if str(evidence.get("service_sid", "")) != WINDOWS_NETWORK_SERVICE_SID:
        errors.append(f"{label}: runtime ACL evidence SID mismatch")
    if str(evidence.get("access", "")) != "ReadAndExecute" or evidence.get("recursive_acl_applied") is not True:
        errors.append(f"{label}: runtime ACL evidence does not prove recursive ReadAndExecute")
    try:
        if os.path.normcase(str(Path(str(evidence.get("runtime_root", ""))).resolve(strict=True))) != os.path.normcase(str(root)):
            errors.append(f"{label}: runtime ACL evidence root mismatch")
    except OSError:
        errors.append(f"{label}: runtime ACL evidence root cannot be resolved")
    try:
        if os.path.normcase(str(Path(str(evidence.get("manifest_path", ""))).resolve(strict=True))) != os.path.normcase(str(manifest_path.resolve(strict=True))):
            errors.append(f"{label}: runtime ACL evidence manifest mismatch")
    except OSError:
        errors.append(f"{label}: runtime ACL evidence manifest cannot be resolved")
    return errors


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
    runtime_manifest: Path | None = None
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
            runtime_manifest = path
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
    if mode == "windows-runtime" and os.name == "nt" and runtime_manifest is not None and runtime_manifest.is_file():
        checked.extend(("windows-runtime-service-sid", "windows-runtime-session-0", "windows-runtime-bounded-root", "windows-runtime-acl-evidence"))
        try:
            current_sid = _current_windows_sid()
            session_id = _current_windows_session_id()
            errors.extend(
                validate_windows_runtime_service_boundary(
                    runtime_manifest,
                    WINDOWS_RUNTIME_ROOT,
                    WINDOWS_RUNTIME_ACL_EVIDENCE,
                    current_sid=current_sid,
                    session_id=session_id,
                )
            )
        except Exception as exc:
            errors.append(f"windows-runtime-service-boundary: unable to prove Windows service identity: {exc}")
    return {
        "schema": "dokkomplekt.release-environment-preflight.v2",
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
