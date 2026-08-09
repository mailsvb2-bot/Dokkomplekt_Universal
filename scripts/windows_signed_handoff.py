from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import shutil
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
BUILD_EVIDENCE_DIR = "build-evidence"
TRANSFERRED_GATE_DIRS = {
    ".cargo-gate": f"{BUILD_EVIDENCE_DIR}/cargo-gate",
    ".release-gate": f"{BUILD_EVIDENCE_DIR}/release-gate",
}


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


def assert_direct_tree_entry(path: Path) -> None:
    if path.is_symlink() or is_reparse(path):
        fail(f"handoff evidence contains symlink/reparse point: {path}")


def copy_verified_tree(source: Path, destination: Path) -> None:
    if not source.is_dir():
        fail(f"required handoff evidence directory is missing: {source}")
    if destination.exists():
        fail(f"handoff evidence destination already exists: {destination}")
    destination.mkdir(parents=True)
    for path in sorted(source.rglob("*"), key=lambda p: p.relative_to(source).as_posix()):
        assert_direct_tree_entry(path)
        relative = path.relative_to(source)
        target = destination / relative
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        elif path.is_file():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
        else:
            fail(f"unsupported handoff evidence entry: {path}")


def copy_verified_tree_into(source: Path, destination: Path) -> None:
    if not source.is_dir():
        fail(f"verified handoff evidence directory is missing: {source}")
    destination.mkdir(parents=True, exist_ok=True)
    for path in sorted(source.rglob("*"), key=lambda p: p.relative_to(source).as_posix()):
        assert_direct_tree_entry(path)
        relative = path.relative_to(source)
        target = destination / relative
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            continue
        if not path.is_file():
            fail(f"unsupported verified handoff evidence entry: {path}")
        if target.exists():
            fail(f"refusing to overwrite pre-existing hardware evidence: {target}")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)


def stage_repository_gate_evidence(root: Path) -> None:
    repository = Path.cwd().resolve()
    for source_name, target_relative in TRANSFERRED_GATE_DIRS.items():
        source = repository / source_name
        target = root / target_relative
        copy_verified_tree(source, target)


def restore_verified_build_evidence(root: Path) -> None:
    repository = Path.cwd().resolve()
    build_evidence = root / BUILD_EVIDENCE_DIR
    if not build_evidence.is_dir():
        fail("signed handoff is missing build-evidence")

    for source_name, target_relative in TRANSFERRED_GATE_DIRS.items():
        source = root / target_relative
        destination = repository / source_name
        copy_verified_tree(source, destination)

    verification_source = build_evidence
    verification_destination = repository / "verification" / "release"
    verification_destination.mkdir(parents=True, exist_ok=True)
    excluded = {Path(value).parts[-1] for value in TRANSFERRED_GATE_DIRS.values()}
    for path in sorted(verification_source.iterdir(), key=lambda p: p.name):
        if path.name in excluded:
            continue
        assert_direct_tree_entry(path)
        target = verification_destination / path.name
        if path.is_dir():
            copy_verified_tree_into(path, target)
        elif path.is_file():
            if target.exists():
                fail(f"refusing to overwrite pre-existing hardware evidence: {target}")
            shutil.copy2(path, target)
        else:
            fail(f"unsupported build-evidence entry: {path}")

    cargo_archive = verification_destination / "cargo-gate"
    copy_verified_tree_into(repository / ".cargo-gate", cargo_archive)


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


def validate_host_id(value: str, label: str) -> str:
    normalized = value.strip().lower()
    if len(normalized) != 64 or any(ch not in "0123456789abcdef" for ch in normalized):
        fail(f"{label} must be a lowercase SHA-256 host fingerprint")
    return normalized


def windows_host_fingerprint() -> str:
    if os.name != "nt":
        fail("Windows host fingerprint must be supplied explicitly outside Windows")
    import winreg  # type: ignore[import-not-found]

    try:
        with winreg.OpenKey(
            winreg.HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Cryptography",
            0,
            winreg.KEY_READ | getattr(winreg, "KEY_WOW64_64KEY", 0),
        ) as key:
            machine_guid = str(winreg.QueryValueEx(key, "MachineGuid")[0]).strip().lower()
    except OSError as exc:
        fail(f"cannot read Windows MachineGuid for trust-domain separation: {exc}")
    computer = os.environ.get("COMPUTERNAME", "").strip().lower()
    if not computer or not machine_guid:
        fail("Windows host identity is incomplete")
    return hashlib.sha256(f"{computer}\n{machine_guid}".encode("utf-8")).hexdigest()


def resolved_host_id(explicit: str | None, label: str) -> str:
    return validate_host_id(explicit, label) if explicit else windows_host_fingerprint()


def manifest_bytes(payload: dict) -> bytes:
    return (json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def build(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    if not root.is_dir():
        fail(f"handoff root is missing: {root}")
    validate_identity(args.release_sha, args.request_id)
    producer_host_id = resolved_host_id(args.producer_host_id, "producer_host_id")
    stage_repository_gate_evidence(root)
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
        "producer_host_id": producer_host_id,
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "files": entries,
    }
    raw = manifest_bytes(payload)
    manifest = root / MANIFEST_NAME
    signature = root / SIGNATURE_NAME
    manifest.write_bytes(raw)
    private_key = load_private_key(Path(args.signing_key))
    signature.write_bytes(base64.b64encode(private_key.sign(raw)) + b"\n")
    print(
        json.dumps(
            {
                "ok": True,
                "manifest": str(manifest),
                "files": len(entries),
                "producer_host_id": producer_host_id,
                "gate_evidence_embedded": True,
            },
            ensure_ascii=False,
        )
    )
    return 0


def verify(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    if not root.is_dir():
        fail(f"handoff root is missing: {root}")
    validate_identity(args.release_sha, args.request_id)
    consumer_host_id = resolved_host_id(args.consumer_host_id, "consumer_host_id")
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
    producer_host_id = validate_host_id(str(payload.get("producer_host_id", "")), "producer_host_id")
    if producer_host_id == consumer_host_id:
        fail("runtime/signing producer and hardware consumer resolve to the same Windows host; physical trust-domain separation is required")
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
    restore_verified_build_evidence(root)
    report = {
        "schema": "dokkomplekt.windows-signed-handoff-verification.v1",
        "ok": True,
        "release_sha": args.release_sha,
        "request_id": args.request_id.lower(),
        "producer_host_id": producer_host_id,
        "consumer_host_id": consumer_host_id,
        "physical_host_separation": True,
        "gate_evidence_restored": True,
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
    b.add_argument("--producer-host-id")
    b.set_defaults(func=build)
    v = sub.add_parser("verify")
    v.add_argument("root")
    v.add_argument("--release-sha", required=True)
    v.add_argument("--request-id", required=True)
    v.add_argument("--trusted-public-key", required=True)
    v.add_argument("--consumer-host-id")
    v.add_argument("--json-report")
    v.set_defaults(func=verify)
    return p


def main() -> int:
    args = parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
