from __future__ import annotations

import argparse
import base64
import hashlib
import json
import stat
import uuid
from datetime import datetime, timezone
from pathlib import Path

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey, Ed25519PublicKey

SCHEMA = "dokkomplekt.windows-signed-handoff.v1"
MANIFEST_NAME = "SIGNED_HANDOFF.json"
SIGNATURE_NAME = "SIGNED_HANDOFF.json.sig"


def fail(message: str) -> None:
    raise SystemExit(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_reparse(path: Path) -> bool:
    st = path.lstat()
    attrs = getattr(st, "st_file_attributes", 0)
    reparse = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return bool(attrs & reparse)


def iter_payload_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for path in root.rglob("*"):
        if path.name in {MANIFEST_NAME, SIGNATURE_NAME} and path.parent == root:
            continue
        if path.is_symlink() or is_reparse(path):
            fail(f"handoff contains symlink/reparse point: {path}")
        if path.is_dir():
            continue
        if not path.is_file():
            fail(f"handoff contains unsupported filesystem entry: {path}")
        files.append(path)
    files.sort(key=lambda p: p.relative_to(root).as_posix())
    if not files:
        fail("handoff payload is empty")
    return files


def load_private_key(path: Path) -> Ed25519PrivateKey:
    key = serialization.load_pem_private_key(path.read_bytes(), password=None)
    if not isinstance(key, Ed25519PrivateKey):
        fail("handoff signing key must be Ed25519")
    return key


def load_public_key(path: Path) -> Ed25519PublicKey:
    key = serialization.load_pem_public_key(path.read_bytes())
    if not isinstance(key, Ed25519PublicKey):
        fail("handoff trusted public key must be Ed25519")
    return key


def validate_identity(release_sha: str, request_id: str) -> None:
    if len(release_sha) != 40 or any(ch not in "0123456789abcdef" for ch in release_sha):
        fail("release_sha must be an exact lowercase 40-character SHA")
    try:
        parsed = uuid.UUID(request_id)
    except ValueError as exc:
        fail(f"request_id must be a UUID: {exc}")
    if str(parsed) != request_id.lower():
        fail("request_id must use canonical UUID form")


def manifest_bytes(payload: dict) -> bytes:
    return (json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def build(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    if not root.is_dir():
        fail(f"handoff root is missing: {root}")
    validate_identity(args.release_sha, args.request_id)
    entries = []
    for path in iter_payload_files(root):
        entries.append(
            {
                "path": path.relative_to(root).as_posix(),
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    payload = {
        "schema": SCHEMA,
        "release_sha": args.release_sha,
        "request_id": args.request_id.lower(),
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "files": entries,
    }
    raw = manifest_bytes(payload)
    manifest = root / MANIFEST_NAME
    signature = root / SIGNATURE_NAME
    manifest.write_bytes(raw)
    private_key = load_private_key(Path(args.signing_key))
    signature.write_bytes(base64.b64encode(private_key.sign(raw)) + b"\n")
    print(json.dumps({"ok": True, "manifest": str(manifest), "files": len(entries)}, ensure_ascii=False))
    return 0


def verify(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    if not root.is_dir():
        fail(f"handoff root is missing: {root}")
    validate_identity(args.release_sha, args.request_id)
    manifest = root / MANIFEST_NAME
    signature = root / SIGNATURE_NAME
    if not manifest.is_file() or not signature.is_file():
        fail("signed handoff manifest/signature is missing")
    raw = manifest.read_bytes()
    try:
        sig = base64.b64decode(signature.read_text(encoding="ascii").strip(), validate=True)
    except Exception as exc:
        fail(f"handoff signature is not valid base64: {exc}")
    public_key = load_public_key(Path(args.trusted_public_key))
    try:
        public_key.verify(sig, raw)
    except InvalidSignature:
        fail("signed handoff manifest signature is invalid")
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as exc:
        fail(f"signed handoff manifest is invalid JSON: {exc}")
    if payload.get("schema") != SCHEMA:
        fail("signed handoff schema mismatch")
    if payload.get("release_sha") != args.release_sha:
        fail("signed handoff release_sha mismatch")
    if payload.get("request_id") != args.request_id.lower():
        fail("signed handoff request_id mismatch")
    declared = payload.get("files")
    if not isinstance(declared, list) or not declared:
        fail("signed handoff file inventory is empty")
    expected: dict[str, tuple[int, str]] = {}
    for entry in declared:
        if not isinstance(entry, dict):
            fail("signed handoff file inventory contains a non-object")
        rel = entry.get("path")
        size = entry.get("size")
        digest = entry.get("sha256")
        if not isinstance(rel, str) or rel.startswith("/") or ".." in Path(rel).parts:
            fail(f"invalid handoff relative path: {rel!r}")
        if rel in expected:
            fail(f"duplicate handoff path: {rel}")
        if not isinstance(size, int) or size < 0:
            fail(f"invalid handoff size: {rel}")
        if not isinstance(digest, str) or len(digest) != 64 or any(ch not in "0123456789abcdef" for ch in digest):
            fail(f"invalid handoff sha256: {rel}")
        expected[rel] = (size, digest)
    actual_files = iter_payload_files(root)
    actual = {path.relative_to(root).as_posix(): path for path in actual_files}
    if set(actual) != set(expected):
        missing = sorted(set(expected) - set(actual))
        unexpected = sorted(set(actual) - set(expected))
        fail(f"handoff file set mismatch: missing={missing}; unexpected={unexpected}")
    for rel, path in actual.items():
        size, digest = expected[rel]
        if path.stat().st_size != size:
            fail(f"handoff size mismatch: {rel}")
        if sha256_file(path) != digest:
            fail(f"handoff sha256 mismatch: {rel}")
    report = {
        "schema": "dokkomplekt.windows-signed-handoff-verification.v1",
        "ok": True,
        "release_sha": args.release_sha,
        "request_id": args.request_id.lower(),
        "files": len(actual),
        "verified_at_utc": datetime.now(timezone.utc).isoformat(),
    }
    if args.json_report:
        out = Path(args.json_report)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="Create or verify a cryptographically signed Windows release handoff.")
    sub = p.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build")
    b.add_argument("root")
    b.add_argument("--release-sha", required=True)
    b.add_argument("--request-id", required=True)
    b.add_argument("--signing-key", required=True)
    b.set_defaults(func=build)
    v = sub.add_parser("verify")
    v.add_argument("root")
    v.add_argument("--release-sha", required=True)
    v.add_argument("--request-id", required=True)
    v.add_argument("--trusted-public-key", required=True)
    v.add_argument("--json-report")
    v.set_defaults(func=verify)
    return p


def main() -> int:
    args = parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
