#!/usr/bin/env python3
"""Offline Ed25519 approval for an exact signed Windows runtime bundle payload.

The production runtime signing key may live in a protected GitHub environment,
but runtime composition still requires an independent offline approval.  The
offline approval private key never needs to exist in GitHub Actions.  A reviewer
signs the exact ``*.signing.json`` payload emitted by
``create_offline_runtime_bundle.py``; hosted signing jobs receive only the raw
approval signature and the pinned approval public key.
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

SCHEMA = "dokkomplekt.windows-runtime-bundle-approval.v1"
PAYLOAD_SCHEMA = "dokkomplekt.offline-runtime.signature.v1"
TARGET = "windows-x86_64"


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def direct_file(path: Path, label: str) -> Path:
    path = path.resolve()
    if not path.is_file():
        raise FileNotFoundError(f"{label} does not exist: {path}")
    if path.is_symlink():
        raise ValueError(f"{label} must not be a symlink")
    is_junction = getattr(path, "is_junction", None)
    if callable(is_junction) and is_junction():
        raise ValueError(f"{label} must not be a junction")
    return path


def load_payload(path: Path) -> tuple[dict[str, Any], bytes]:
    payload_path = direct_file(path, "runtime signing payload")
    payload = payload_path.read_bytes()
    try:
        data = json.loads(payload.decode("utf-8"))
    except Exception as exc:
        raise ValueError(f"runtime signing payload is not valid UTF-8 JSON: {exc}") from exc
    if not isinstance(data, dict) or data.get("schema") != PAYLOAD_SCHEMA:
        raise ValueError(f"runtime signing payload schema must be {PAYLOAD_SCHEMA}")
    if data.get("target") != TARGET:
        raise ValueError(f"runtime signing payload target must be {TARGET}")
    if data.get("supply_chain_locked") is not True:
        raise ValueError("runtime signing payload must assert supply_chain_locked=true")
    if data.get("semantic_model_required") is not True:
        raise ValueError("runtime signing payload must require the semantic model")
    digest = str(data.get("bundle_sha256", "")).strip().lower()
    if len(digest) != 64 or any(ch not in "0123456789abcdef" for ch in digest):
        raise ValueError("runtime signing payload bundle_sha256 is invalid")
    size = data.get("bundle_size_bytes")
    if not isinstance(size, int) or size <= 0:
        raise ValueError("runtime signing payload bundle_size_bytes is invalid")
    return data, payload


def load_private_key(path: Path) -> Ed25519PrivateKey:
    key = serialization.load_pem_private_key(direct_file(path, "approval private key").read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError("approval private key must be Ed25519 PEM")
    return key


def load_public_key(path: Path) -> Ed25519PublicKey:
    key = serialization.load_pem_public_key(direct_file(path, "trusted approval public key").read_bytes())
    if not isinstance(key, Ed25519PublicKey):
        raise ValueError("trusted approval public key must be Ed25519 PEM")
    return key


def public_key_id(key: Ed25519PublicKey) -> str:
    der = key.public_bytes(serialization.Encoding.DER, serialization.PublicFormat.SubjectPublicKeyInfo)
    return hashlib.sha256(der).hexdigest()


def atomic_write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_bytes(payload)
    os.replace(temporary, path)


def atomic_json(path: Path, value: object) -> None:
    atomic_write(path, (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def sign_payload(
    payload_path: Path,
    private_key_path: Path,
    signature_path: Path,
    metadata_path: Path,
    reviewer: str,
) -> dict[str, Any]:
    data, payload = load_payload(payload_path)
    reviewer = reviewer.strip()
    if not reviewer or reviewer.upper().startswith("REPLACE_"):
        raise ValueError("reviewer is required and cannot be a placeholder")
    private_key = load_private_key(private_key_path)
    signature = private_key.sign(payload)
    if len(signature) != 64:
        raise ValueError("unexpected Ed25519 signature length")
    atomic_write(signature_path.resolve(), signature)
    metadata = {
        "schema": SCHEMA,
        "algorithm": "Ed25519",
        "target": data["target"],
        "payload": payload_path.name,
        "payload_sha256": sha256_bytes(payload),
        "bundle_sha256": data["bundle_sha256"],
        "signature": signature_path.name,
        "signature_sha256": sha256_bytes(signature),
        "approval_key_id": public_key_id(private_key.public_key()),
        "reviewer": reviewer,
        "approved_at_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "private_key_present_in_ci": False,
    }
    atomic_json(metadata_path.resolve(), metadata)
    return metadata


def verify_payload(payload_path: Path, signature_path: Path, public_key_path: Path) -> dict[str, Any]:
    data, payload = load_payload(payload_path)
    signature = direct_file(signature_path, "runtime bundle approval signature").read_bytes()
    if len(signature) != 64:
        raise ValueError("runtime bundle approval signature must be a raw 64-byte Ed25519 signature")
    public_key = load_public_key(public_key_path)
    try:
        public_key.verify(signature, payload)
    except InvalidSignature as exc:
        raise ValueError("runtime bundle offline approval signature verification failed") from exc
    return {
        "schema": SCHEMA,
        "ok": True,
        "algorithm": "Ed25519",
        "target": data["target"],
        "payload": str(payload_path.resolve()),
        "payload_sha256": sha256_bytes(payload),
        "bundle_sha256": data["bundle_sha256"],
        "signature": str(signature_path.resolve()),
        "signature_sha256": sha256_bytes(signature),
        "approval_key_id": public_key_id(public_key),
        "supply_chain_locked": True,
        "semantic_model_required": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    sign = sub.add_parser("sign", help="offline-approve an exact signed runtime payload")
    sign.add_argument("payload", type=Path)
    sign.add_argument("--private-key", type=Path, required=True)
    sign.add_argument("--signature", type=Path)
    sign.add_argument("--metadata", type=Path)
    sign.add_argument("--reviewer", required=True)

    verify = sub.add_parser("verify", help="verify offline runtime-bundle approval")
    verify.add_argument("payload", type=Path)
    verify.add_argument("--signature", type=Path)
    verify.add_argument("--trusted-public-key", type=Path, required=True)
    verify.add_argument("--json-report", type=Path)

    args = parser.parse_args()
    payload = args.payload.resolve()
    default_signature = Path(str(payload) + ".approval.sig")
    if args.command == "sign":
        signature = (args.signature or default_signature).resolve()
        metadata = (args.metadata or Path(str(payload) + ".approval.json")).resolve()
        report = sign_payload(payload, args.private_key.resolve(), signature, metadata, args.reviewer)
    else:
        signature = (args.signature or default_signature).resolve()
        report = verify_payload(payload, signature, args.trusted_public_key.resolve())
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
