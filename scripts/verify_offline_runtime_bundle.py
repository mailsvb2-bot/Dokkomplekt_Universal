#!/usr/bin/env python3
"""Verify a Dokkomplekt offline runtime ZIP and optional detached Ed25519 signature."""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import zipfile
from pathlib import Path


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--payload", type=Path)
    parser.add_argument("--signature", type=Path)
    parser.add_argument("--public-key", type=Path)
    parser.add_argument("--trusted-public-key", type=Path)
    parser.add_argument("--require-signature", action="store_true")
    args = parser.parse_args()
    bundle = args.bundle.resolve()
    payload_path = (args.payload or bundle.with_suffix(bundle.suffix + ".signing.json")).resolve()
    payload = json.loads(payload_path.read_text("utf-8"))
    if payload.get("schema") != "dokkomplekt.offline-runtime.signature.v1":
        raise ValueError("unsupported signing payload schema")
    actual = sha256_file(bundle)
    if actual != payload.get("bundle_sha256") or bundle.stat().st_size != payload.get("bundle_size_bytes"):
        raise ValueError("offline runtime bundle digest/size mismatch")
    with zipfile.ZipFile(bundle) as archive:
        bad = archive.testzip()
        if bad:
            raise ValueError(f"corrupted ZIP entry: {bad}")
        sbom = archive.read("runtime-sbom.json")
        if hashlib.sha256(sbom).hexdigest() != payload.get("sbom_sha256"):
            raise ValueError("runtime SBOM digest mismatch")
        metadata = json.loads(sbom)
        for entry in metadata.get("files", []):
            data = archive.read(f"runtime/{metadata['target']}/{entry['path']}")
            if hashlib.sha256(data).hexdigest() != entry["sha256"] or len(data) != entry["size_bytes"]:
                raise ValueError(f"runtime file mismatch: {entry['path']}")
    signature = args.signature
    public_key = args.public_key
    trusted_public_key = args.trusted_public_key
    if args.require_signature and trusted_public_key is None:
        raise ValueError("a pinned --trusted-public-key is required; artifact-provided keys are not trusted")
    verification_key = trusted_public_key or public_key
    if signature and verification_key:
        if public_key and trusted_public_key:
            def der(path: Path) -> bytes:
                import tempfile
                with tempfile.NamedTemporaryFile(delete=False) as handle:
                    output = Path(handle.name)
                try:
                    subprocess.run(
                        ["openssl", "pkey", "-pubin", "-in", str(path), "-outform", "DER", "-out", str(output)],
                        check=True,
                    )
                    return output.read_bytes()
                finally:
                    output.unlink(missing_ok=True)
            if der(public_key) != der(trusted_public_key):
                raise ValueError("artifact public key does not match pinned trusted key")
        subprocess.run(
            [
                "openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey", str(verification_key),
                "-in", str(payload_path), "-sigfile", str(signature),
            ],
            check=True,
        )
    elif args.require_signature:
        raise ValueError("signature and pinned public key are required")
    print(f"OFFLINE RUNTIME VERIFIED: {bundle.name}; sha256={actual}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
