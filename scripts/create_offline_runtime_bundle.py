#!/usr/bin/env python3
"""Create a deterministic, optionally Ed25519-signed offline runtime bundle.

The script never downloads dependencies. It only packages files that were
previously staged by ``prepare_sidecars.py`` and revalidated by
``assert_offline_runtime_ready.py``. A release signature is detached from the
ZIP so the application/installer can verify it before extraction.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
FIXED_ZIP_TIME = (2020, 1, 1, 0, 0, 0)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def validate_target(target: str) -> str:
    if not target or any(ch not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for ch in target):
        raise ValueError("target must be a safe platform-arch identifier")
    return target


def load_verified_status(target: str, require_model: bool, require_supply_chain: bool) -> tuple[Path, dict[str, Any]]:
    command = [
        sys.executable,
        str(ROOT / "scripts" / "assert_offline_runtime_ready.py"),
        "--target",
        target,
    ]
    if require_model:
        command.append("--require-semantic-model")
    if require_supply_chain:
        command.append("--require-supply-chain")
    subprocess.run(command, cwd=ROOT, check=True)
    target_dir = (TOOLS_ROOT / target).resolve()
    target_dir.relative_to(TOOLS_ROOT.resolve())
    status = json.loads((target_dir / "sidecar-status.json").read_text("utf-8"))
    return target_dir, status


def zip_info(name: str, executable: bool = False) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_DEFLATED
    mode = 0o755 if executable else 0o644
    info.external_attr = (mode & 0xFFFF) << 16
    info.create_system = 3
    return info


def create_bundle(
    target: str,
    output_dir: Path,
    require_model: bool,
    require_supply_chain: bool = False,
) -> tuple[Path, Path, dict[str, Any]]:
    """Build a deterministic runtime archive from an already verified stage.

    ``require_supply_chain`` was added after the original v18.1.2 helper API was
    published. Keep a conservative default for callers that only requested the
    earlier model-ready contract, while release entry points explicitly pass
    ``True`` and therefore remain fail-closed.
    """
    target = validate_target(target)
    target_dir, status = load_verified_status(target, require_model, require_supply_chain)
    output_dir.mkdir(parents=True, exist_ok=True)
    entries = []
    for raw in status["files"]:
        relative = Path(str(raw["path"]).replace("\\", "/"))
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"unsafe staged path: {relative}")
        source = target_dir / relative
        actual = sha256_file(source)
        if actual != raw["sha256"]:
            raise ValueError(f"staged file changed after verification: {relative}")
        entry = {
            "tool": raw["tool"],
            "path": relative.as_posix(),
            "sha256": actual,
            "size_bytes": source.stat().st_size,
            "executable": os.access(source, os.X_OK),
        }
        for metadata_key in ("version", "source_url", "license", "license_path", "license_sha256"):
            if metadata_key in raw:
                entry[metadata_key] = raw[metadata_key]
        entries.append(entry)
    entries.sort(key=lambda item: item["path"])
    license_entries = []
    seen_licenses = set()
    for item in entries:
        license_path = item.get("license_path")
        if not license_path or license_path in seen_licenses:
            continue
        seen_licenses.add(license_path)
        relative = Path(str(license_path).replace("\\", "/"))
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"unsafe staged license path: {relative}")
        source = target_dir / relative
        actual = sha256_file(source)
        if actual != item.get("license_sha256"):
            raise ValueError(f"staged license changed after verification: {relative}")
        license_entries.append({
            "path": relative.as_posix(),
            "sha256": actual,
            "size_bytes": source.stat().st_size,
        })
    license_entries.sort(key=lambda item: item["path"])
    sbom = {
        "schema": "dokkomplekt.offline-runtime.sbom.v1",
        "target": target,
        "network_used": False,
        "semantic_model_required": require_model,
        "supply_chain_locked": status.get("supply_chain_locked") is True,
        "files": entries,
        "license_notices": license_entries,
    }
    sbom_bytes = canonical_json(sbom)
    output = output_dir / f"Dokkomplekt-offline-runtime-{target}.zip"
    temporary = output.with_suffix(".zip.tmp")
    with zipfile.ZipFile(temporary, "w", allowZip64=True) as archive:
        archive.writestr(zip_info("runtime-sbom.json"), sbom_bytes)
        for item in entries:
            source = target_dir / item["path"]
            archive.writestr(
                zip_info(f"runtime/{target}/{item['path']}", bool(item["executable"])),
                source.read_bytes(),
            )
        for item in license_entries:
            source = target_dir / item["path"]
            archive.writestr(
                zip_info(f"runtime/{target}/{item['path']}", False),
                source.read_bytes(),
            )
    temporary.replace(output)
    payload = {
        "schema": "dokkomplekt.offline-runtime.signature.v1",
        "target": target,
        "bundle": output.name,
        "bundle_sha256": sha256_file(output),
        "bundle_size_bytes": output.stat().st_size,
        "sbom_sha256": hashlib.sha256(sbom_bytes).hexdigest(),
        "semantic_model_required": require_model,
        "supply_chain_locked": status.get("supply_chain_locked") is True,
    }
    payload_path = output.with_suffix(output.suffix + ".signing.json")
    payload_path.write_bytes(canonical_json(payload))
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{payload['bundle_sha256']}  {output.name}\n", "utf-8"
    )
    return output, payload_path, payload


def public_key_der(path: Path) -> bytes:
    with tempfile.NamedTemporaryFile(delete=False) as der:
        der_path = Path(der.name)
    try:
        subprocess.run(
            ["openssl", "pkey", "-pubin", "-in", str(path), "-outform", "DER", "-out", str(der_path)],
            check=True,
        )
        return der_path.read_bytes()
    finally:
        der_path.unlink(missing_ok=True)


def sign_payload(
    payload_path: Path,
    private_key: Path,
    trusted_public_key: Path | None = None,
) -> tuple[Path, Path, str]:
    if not private_key.is_file():
        raise FileNotFoundError(f"signing key not found: {private_key}")
    signature = payload_path.with_suffix(payload_path.suffix + ".sig")
    public_key = payload_path.with_suffix(payload_path.suffix + ".public.pem")
    subprocess.run(
        ["openssl", "pkey", "-in", str(private_key), "-pubout", "-out", str(public_key)],
        check=True,
    )
    subprocess.run(
        [
            "openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(private_key),
            "-in", str(payload_path), "-out", str(signature),
        ],
        check=True,
    )
    verification_key = public_key
    if trusted_public_key is not None:
        if not trusted_public_key.is_file():
            raise FileNotFoundError(f"trusted public key not found: {trusted_public_key}")
        generated_der = public_key_der(public_key)
        trusted_der = public_key_der(trusted_public_key)
        if generated_der != trusted_der:
            raise ValueError("runtime private key does not match the pinned trusted public key")
        verification_key = trusted_public_key
    subprocess.run(
        [
            "openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey", str(verification_key),
            "-in", str(payload_path), "-sigfile", str(signature),
        ],
        check=True,
    )
    key_id = hashlib.sha256(public_key_der(verification_key)).hexdigest()
    return signature, public_key, key_id


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    parser.add_argument("--output-dir", type=Path, default=ROOT / "release-runtime")
    parser.add_argument("--require-semantic-model", action="store_true")
    parser.add_argument("--require-supply-chain", action="store_true")
    parser.add_argument("--signing-key", type=Path)
    parser.add_argument("--trusted-public-key", type=Path)
    parser.add_argument("--require-signature", action="store_true")
    args = parser.parse_args()

    bundle, payload_path, payload = create_bundle(
        args.target,
        args.output_dir.resolve(),
        args.require_semantic_model,
        args.require_supply_chain,
    )
    signature = None
    key_id = None
    if args.signing_key:
        if args.require_signature and args.trusted_public_key is None:
            raise ValueError("release runtime bundle requires --trusted-public-key to prevent trust-on-first-use")
        signature, _public, key_id = sign_payload(
            payload_path,
            args.signing_key.resolve(),
            args.trusted_public_key.resolve() if args.trusted_public_key else None,
        )
    elif args.require_signature:
        raise ValueError("release runtime bundle requires --signing-key")
    print(
        json.dumps(
            {
                "bundle": str(bundle),
                "sha256": payload["bundle_sha256"],
                "signature": str(signature) if signature else None,
                "signing_key_id": key_id,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
