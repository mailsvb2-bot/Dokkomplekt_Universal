#!/usr/bin/env python3
"""Verify and stage an independently approved signed offline runtime bundle.

This is the hosted-CI replacement for a persistent runner-owned runtime tree.
The exact bundle must be signed by the production runtime key *and* approved by
an independent offline Ed25519 key before any executable is staged.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import stat
import sys
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

try:
    from scripts._release_policy import validate_relative_runtime_path
    from scripts.windows_runtime_bundle_approval import verify_payload as verify_offline_approval
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path
    from windows_runtime_bundle_approval import verify_payload as verify_offline_approval

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
PAYLOAD_SCHEMA = "dokkomplekt.offline-runtime.signature.v1"
SBOM_SCHEMA = "dokkomplekt.offline-runtime.sbom.v1"
TARGET = "windows-x86_64"
MAX_ENTRIES = 100_000
MAX_UNPACKED_BYTES = 16 * 1024 * 1024 * 1024


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def direct_file(path: Path, label: str) -> Path:
    path = path.resolve()
    if not path.is_file():
        raise FileNotFoundError(f"{label} does not exist: {path}")
    if path.is_symlink():
        raise ValueError(f"{label} must not be a symlink")
    return path


def load_ed25519_public_key(path: Path, label: str) -> Ed25519PublicKey:
    key = serialization.load_pem_public_key(direct_file(path, label).read_bytes())
    if not isinstance(key, Ed25519PublicKey):
        raise ValueError(f"{label} must be Ed25519 PEM")
    return key


def safe_zip_member(name: str) -> str:
    if not name or "\\" in name or name.startswith("/"):
        raise ValueError(f"unsafe ZIP member: {name!r}")
    pure = PurePosixPath(name)
    if pure.is_absolute() or any(part in {"", ".", ".."} for part in pure.parts):
        raise ValueError(f"unsafe ZIP member: {name!r}")
    return pure.as_posix()


def is_zip_symlink(info: zipfile.ZipInfo) -> bool:
    mode = (info.external_attr >> 16) & 0xFFFF
    return stat.S_ISLNK(mode)


def checked_member(archive: zipfile.ZipFile, name: str, expected_sha: str, expected_size: int) -> bytes:
    info = archive.getinfo(name)
    if info.is_dir() or is_zip_symlink(info):
        raise ValueError(f"runtime ZIP member has unsafe type: {name}")
    if info.file_size != expected_size:
        raise ValueError(f"runtime ZIP member size mismatch: {name}")
    data = archive.read(info)
    if len(data) != expected_size or sha256_bytes(data) != expected_sha:
        raise ValueError(f"runtime ZIP member digest mismatch: {name}")
    return data


def atomic_write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(data)
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--payload", type=Path, required=True)
    parser.add_argument("--signature", type=Path, required=True)
    parser.add_argument("--trusted-runtime-public-key", type=Path, required=True)
    parser.add_argument("--approval-signature", type=Path, required=True)
    parser.add_argument("--trusted-approval-public-key", type=Path, required=True)
    parser.add_argument("--target", default=TARGET)
    parser.add_argument("--clean", action="store_true")
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    if args.target != TARGET:
        raise ValueError(f"hosted Windows runtime target must be {TARGET}")
    bundle = direct_file(args.bundle, "runtime bundle")
    payload_path = direct_file(args.payload, "runtime signing payload")
    signature_path = direct_file(args.signature, "runtime signature")
    trusted_runtime_public_key = direct_file(args.trusted_runtime_public_key, "trusted runtime public key")
    payload_bytes = payload_path.read_bytes()
    payload = json.loads(payload_bytes.decode("utf-8"))
    if payload.get("schema") != PAYLOAD_SCHEMA or payload.get("target") != TARGET:
        raise ValueError("runtime signing payload schema/target mismatch")
    if payload.get("supply_chain_locked") is not True or payload.get("semantic_model_required") is not True:
        raise ValueError("hosted production runtime must be supply-chain locked and include semantic model")
    if payload.get("distribution_review_bound") is not True:
        raise ValueError("hosted production runtime must bind the reviewed distribution inventory")
    if payload.get("bundle") != bundle.name:
        raise ValueError("runtime payload bundle filename mismatch")
    if sha256_file(bundle) != str(payload.get("bundle_sha256", "")).lower():
        raise ValueError("runtime bundle SHA-256 mismatch")
    if bundle.stat().st_size != payload.get("bundle_size_bytes"):
        raise ValueError("runtime bundle size mismatch")

    runtime_key = load_ed25519_public_key(trusted_runtime_public_key, "trusted runtime public key")
    runtime_signature = signature_path.read_bytes()
    if len(runtime_signature) != 64:
        raise ValueError("runtime signature must be a raw 64-byte Ed25519 signature")
    try:
        runtime_key.verify(runtime_signature, payload_bytes)
    except InvalidSignature as exc:
        raise ValueError("runtime release signature verification failed") from exc

    approval = verify_offline_approval(
        payload_path,
        direct_file(args.approval_signature, "runtime offline approval signature"),
        direct_file(args.trusted_approval_public_key, "trusted offline approval public key"),
    )

    destination = (TOOLS_ROOT / TARGET).resolve()
    destination.relative_to(TOOLS_ROOT.resolve())
    if args.clean and destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True, exist_ok=True)

    staged: list[dict[str, Any]] = []
    total_unpacked = 0
    with zipfile.ZipFile(bundle) as archive:
        infos = archive.infolist()
        if not infos or len(infos) > MAX_ENTRIES:
            raise ValueError("runtime bundle ZIP entry count is invalid")
        names = [safe_zip_member(info.filename) for info in infos]
        if len(names) != len(set(names)):
            raise ValueError("runtime bundle contains duplicate ZIP members")
        if any(is_zip_symlink(info) for info in infos):
            raise ValueError("runtime bundle contains a symlink entry")
        total_unpacked = sum(info.file_size for info in infos if not info.is_dir())
        if total_unpacked > MAX_UNPACKED_BYTES:
            raise ValueError("runtime bundle exceeds unpacked size limit")

        sbom_bytes = archive.read("runtime-sbom.json")
        if sha256_bytes(sbom_bytes) != payload.get("sbom_sha256"):
            raise ValueError("runtime SBOM digest mismatch")
        sbom = json.loads(sbom_bytes.decode("utf-8"))
        if sbom.get("schema") != SBOM_SCHEMA or sbom.get("target") != TARGET:
            raise ValueError("runtime SBOM schema/target mismatch")
        if sbom.get("network_used") is not False or sbom.get("supply_chain_locked") is not True:
            raise ValueError("runtime SBOM does not prove offline locked provenance")
        if sbom.get("semantic_model_required") is not True:
            raise ValueError("runtime SBOM does not require semantic model")
        files = sbom.get("files")
        licenses = sbom.get("license_notices")
        review = sbom.get("distribution_review")
        if not isinstance(files, list) or not files:
            raise ValueError("runtime SBOM files array is missing")
        if not isinstance(licenses, list):
            raise ValueError("runtime SBOM license_notices must be an array")
        if not isinstance(review, dict) or review.get("complete_portable_tree") is not True:
            raise ValueError("runtime SBOM reviewed distribution inventory is missing")

        expected_members = {"runtime-sbom.json"}
        for index, item in enumerate(files):
            if not isinstance(item, dict):
                raise ValueError(f"runtime SBOM files[{index}] must be an object")
            relative = validate_relative_runtime_path(item.get("path"), f"runtime SBOM files[{index}].path")
            expected_sha = str(item.get("sha256", "")).lower()
            expected_size = item.get("size_bytes")
            if len(expected_sha) != 64 or not isinstance(expected_size, int) or expected_size < 0:
                raise ValueError(f"runtime SBOM files[{index}] digest/size invalid")
            member = f"runtime/{TARGET}/{relative}"
            expected_members.add(member)
            data = checked_member(archive, member, expected_sha, expected_size)
            output = destination / Path(relative)
            atomic_write(output, data)
            entry = {
                "tool": str(item.get("tool", "")),
                "path": relative,
                "sha256": expected_sha,
                "size_bytes": expected_size,
                "executable": bool(item.get("executable", False)),
            }
            for key in ("version", "source_url", "license", "license_path", "license_sha256"):
                if key in item:
                    entry[key] = item[key]
            staged.append(entry)

        for index, item in enumerate(licenses):
            if not isinstance(item, dict):
                raise ValueError(f"runtime SBOM license_notices[{index}] must be an object")
            relative = validate_relative_runtime_path(item.get("path"), f"license_notices[{index}].path")
            member = f"runtime/{TARGET}/{relative}"
            expected_members.add(member)
            data = checked_member(archive, member, str(item.get("sha256", "")).lower(), int(item.get("size_bytes", -1)))
            atomic_write(destination / Path(relative), data)

        review_payload: dict[str, Any] = {"complete_portable_tree": True}
        for key in ("reviewer", "reviewed_at", "scope", "inventory_path", "inventory_sha256"):
            value = str(review.get(key, "")).strip()
            if not value:
                raise ValueError(f"runtime distribution_review is missing {key}")
            review_payload[key] = value
        inventory_relative = validate_relative_runtime_path(review_payload["inventory_path"], "distribution inventory path")
        inventory_member = f"runtime/{TARGET}/{inventory_relative}"
        expected_members.add(inventory_member)
        inventory_info = archive.getinfo(inventory_member)
        inventory_data = checked_member(
            archive,
            inventory_member,
            str(review_payload["inventory_sha256"]).lower(),
            inventory_info.file_size,
        )
        atomic_write(destination / Path(inventory_relative), inventory_data)
        review_payload["inventory_path"] = inventory_relative

        actual_members = {name for name in names if not name.endswith("/")}
        if actual_members != expected_members:
            missing = sorted(expected_members - actual_members)
            extra = sorted(actual_members - expected_members)
            raise ValueError(f"runtime bundle file set mismatch; missing={missing}; extra={extra}")

    status = {
        "schema": 1,
        "target": TARGET,
        "generated_by": "scripts/stage_signed_runtime_bundle.py",
        "network_used": False,
        "supply_chain_locked": True,
        "files": staged,
        "distribution_review": review_payload,
    }
    status_path = destination / "sidecar-status.json"
    status_path.write_text(json.dumps(status, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    trusted_key_bytes = trusted_runtime_public_key.read_bytes()
    report = {
        "schema": "dokkomplekt.hosted-runtime-stage.v1",
        "ok": True,
        "target": TARGET,
        "bundle": str(bundle),
        "bundle_sha256": payload["bundle_sha256"],
        "runtime_signature_verified": True,
        "offline_approval_verified": True,
        "offline_approval_key_id": approval["approval_key_id"],
        "trusted_runtime_public_key_sha256": sha256_bytes(trusted_key_bytes),
        "files": len(staged),
        "unpacked_bytes": total_unpacked,
        "sidecar_status": str(status_path),
    }
    if args.json_report:
        report_path = args.json_report.resolve()
        report_path.parent.mkdir(parents=True, exist_ok=True)
        # These public evidence files are copied from the exact objects used by
        # this successful staging decision, so downstream hardware evidence can
        # bind to the real trust root/status without artifact-provided TOFU.
        atomic_write(report_path.parent / "sidecar-status.json", status_path.read_bytes())
        atomic_write(report_path.parent / "runtime-trusted-public.pem", trusted_key_bytes)
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
