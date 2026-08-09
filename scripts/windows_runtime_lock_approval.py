#!/usr/bin/env python3
"""Offline Ed25519 approval/signature for the runner-owned Windows runtime lock.

The approval private key is intentionally NOT a GitHub Actions secret and must
never exist on the production self-hosted runner. A reviewer signs the exact
bytes of the generated `windows-x86_64-manifest.json` on a separate trusted
machine. The hardware runner receives only the manifest, detached signature and
pinned public key, then verifies the approval before staging any runtime file.
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

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey

SCHEMA = "dokkomplekt.windows-runtime-lock-approval.v1"
TARGET = "windows-x86_64"


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def direct_file(path: Path, label: str) -> Path:
    if not path.is_absolute():
        raise ValueError(f"{label} must be an absolute path")
    if not path.exists():
        raise FileNotFoundError(f"{label} does not exist: {path}")
    if path.is_symlink():
        raise ValueError(f"{label} must not be a symlink")
    is_junction = getattr(path, "is_junction", None)
    if callable(is_junction) and is_junction():
        raise ValueError(f"{label} must not be a junction")
    item = path.resolve()
    if not item.is_file():
        raise ValueError(f"{label} must be a regular file")
    return item


def load_manifest(path: Path) -> tuple[dict[str, Any], bytes]:
    manifest = direct_file(path, "runtime manifest")
    payload = manifest.read_bytes()
    try:
        data = json.loads(payload.decode("utf-8"))
    except Exception as exc:
        raise ValueError(f"runtime manifest is not valid UTF-8 JSON: {exc}") from exc
    if not isinstance(data, dict) or data.get("schema") != 1:
        raise ValueError("runtime manifest schema must be 1")
    if data.get("target") != TARGET:
        raise ValueError(f"runtime manifest target must be {TARGET}")
    if data.get("supply_chain_locked") is not True:
        raise ValueError("runtime manifest must have supply_chain_locked=true")
    files = data.get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("runtime manifest must contain a non-empty files array")
    review = data.get("distribution_review")
    if not isinstance(review, dict) or review.get("complete_portable_tree") is not True:
        raise ValueError("runtime manifest must bind a reviewed complete portable-tree inventory")
    return data, payload


def load_private_key(path: Path) -> Ed25519PrivateKey:
    key_path = direct_file(path, "approval private key")
    key = serialization.load_pem_private_key(key_path.read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError("approval private key must be Ed25519 PEM")
    return key


def load_public_key(path: Path) -> Ed25519PublicKey:
    key_path = direct_file(path, "trusted approval public key")
    key = serialization.load_pem_public_key(key_path.read_bytes())
    if not isinstance(key, Ed25519PublicKey):
        raise ValueError("trusted approval public key must be Ed25519 PEM")
    return key


def public_key_id(key: Ed25519PublicKey) -> str:
    der = key.public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return hashlib.sha256(der).hexdigest()


def atomic_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(payload)
    os.replace(temporary, path)


def atomic_json(path: Path, payload: object) -> None:
    atomic_write(
        path,
        (json.dumps(payload, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
    )


def sign_manifest(
    manifest_path: Path,
    private_key_path: Path,
    signature_path: Path,
    metadata_path: Path,
    reviewer: str,
) -> dict[str, Any]:
    data, payload = load_manifest(manifest_path)
    reviewer = reviewer.strip()
    if not reviewer or reviewer.upper().startswith("REPLACE_"):
        raise ValueError("reviewer is required and cannot be a placeholder")
    private_key = load_private_key(private_key_path)
    public_key = private_key.public_key()
    signature = private_key.sign(payload)
    if len(signature) != 64:
        raise ValueError("unexpected Ed25519 signature length")
    atomic_write(signature_path.resolve(), signature)
    metadata = {
        "schema": SCHEMA,
        "algorithm": "Ed25519",
        "target": data["target"],
        "manifest": manifest_path.name,
        "manifest_sha256": sha256_bytes(payload),
        "manifest_size_bytes": len(payload),
        "signature": signature_path.name,
        "signature_sha256": sha256_bytes(signature),
        "approval_key_id": public_key_id(public_key),
        "reviewer": reviewer,
        "approved_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "private_key_present_on_runner": False,
    }
    atomic_json(metadata_path.resolve(), metadata)
    return metadata


def verify_manifest(
    manifest_path: Path,
    signature_path: Path,
    public_key_path: Path,
) -> dict[str, Any]:
    data, payload = load_manifest(manifest_path)
    signature_file = direct_file(signature_path, "runtime lock signature")
    signature = signature_file.read_bytes()
    if len(signature) != 64:
        raise ValueError("runtime lock signature must be a raw 64-byte Ed25519 signature")
    public_key = load_public_key(public_key_path)
    try:
        public_key.verify(signature, payload)
    except InvalidSignature as exc:
        raise ValueError("runtime lock approval signature verification failed") from exc
    return {
        "schema": SCHEMA,
        "ok": True,
        "algorithm": "Ed25519",
        "target": data["target"],
        "manifest": str(manifest_path.resolve()),
        "manifest_sha256": sha256_bytes(payload),
        "manifest_size_bytes": len(payload),
        "signature": str(signature_file),
        "signature_sha256": sha256_bytes(signature),
        "approval_key_id": public_key_id(public_key),
        "supply_chain_locked": True,
        "complete_portable_tree": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    sign = sub.add_parser("sign", help="sign an exact reviewed runtime lock offline")
    sign.add_argument("manifest", type=Path)
    sign.add_argument("--private-key", type=Path, required=True)
    sign.add_argument("--signature", type=Path)
    sign.add_argument("--metadata", type=Path)
    sign.add_argument("--reviewer", required=True)

    verify = sub.add_parser("verify", help="verify offline approval before staging")
    verify.add_argument("manifest", type=Path)
    verify.add_argument("--signature", type=Path)
    verify.add_argument("--trusted-public-key", type=Path, required=True)
    verify.add_argument("--json-report", type=Path)

    args = parser.parse_args()
    manifest = args.manifest.resolve()
    default_signature = Path(str(manifest) + ".sig")

    if args.command == "sign":
        signature = (args.signature or default_signature).resolve()
        metadata = (args.metadata or Path(str(manifest) + ".approval.json")).resolve()
        report = sign_manifest(
            manifest,
            args.private_key.resolve(),
            signature,
            metadata,
            args.reviewer,
        )
        print(json.dumps(report, ensure_ascii=False))
        return 0

    signature = (args.signature or default_signature).resolve()
    report = verify_manifest(manifest, signature, args.trusted_public_key.resolve())
    if args.json_report:
        atomic_json(args.json_report.resolve(), report)
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
