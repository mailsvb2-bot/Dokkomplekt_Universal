#!/usr/bin/env python3
"""Create a deterministic, optionally Ed25519-signed offline runtime bundle.

The script never downloads dependencies. It only packages files that were
previously staged by ``prepare_sidecars.py`` and revalidated by
``assert_offline_runtime_ready.py``. A release signature is detached from the
ZIP so the application/installer can verify it before extraction.

Windows runtime composition has two canonical profiles:
- ``core``: OCR/PDF/office/print/archive tools used by normal document flows;
- ``full``: core plus the separately approved semantic runtime/model.

The stock NSIS package consumes ``core``.  Semantic functionality remains an
optional signed component because the approved candidate model can exceed the
stock NSIS data limit.
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

try:
    from scripts._release_policy import validate_relative_runtime_path
    from scripts._runtime_profile import (
        CORE_PROFILE,
        FULL_PROFILE,
        PROFILES,
        include_tool,
        normalize_profile,
        profile_requires_semantic,
        validate_profile_file_set,
    )
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path
    from _runtime_profile import (
        CORE_PROFILE,
        FULL_PROFILE,
        PROFILES,
        include_tool,
        normalize_profile,
        profile_requires_semantic,
        validate_profile_file_set,
    )

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


def staged_profile(status: dict[str, Any], requested_profile: str) -> str:
    """Resolve the profile that must be verified before deriving a bundle.

    A bounded ``core`` bundle may be derived from a verified ``full`` stage, but
    the source stage must first pass its own stronger full-profile contract.
    Legacy full stages predate ``runtime_profile`` and are recognized only when
    their semantic requirement or complete semantic tool pair proves that shape.
    """
    declared = status.get("runtime_profile")
    if declared is not None:
        return normalize_profile(
            declared, semantic_model_required=status.get("semantic_model_required")
        )
    if status.get("semantic_model_required") is True:
        return FULL_PROFILE
    tools = {str(item.get("tool", "")).strip().lower() for item in status.get("files", [])}
    if {"llama_cpp", "semantic_model"}.issubset(tools):
        return FULL_PROFILE
    return normalize_profile(
        requested_profile, semantic_model_required=(requested_profile == FULL_PROFILE)
    )


def load_verified_status(target: str, profile: str, require_supply_chain: bool) -> tuple[Path, dict[str, Any]]:
    target_dir = (TOOLS_ROOT / target).resolve()
    target_dir.relative_to(TOOLS_ROOT.resolve())
    status_path = target_dir / "sidecar-status.json"
    before = status_path.read_bytes()
    status = json.loads(before.decode("utf-8"))
    source_profile = staged_profile(status, profile)
    require_model = profile_requires_semantic(source_profile)
    command = [
        sys.executable,
        str(ROOT / "scripts" / "assert_offline_runtime_ready.py"),
        "--target",
        target,
        "--profile",
        source_profile,
    ]
    if require_model:
        command.append("--require-semantic-model")
    if require_supply_chain:
        command.append("--require-supply-chain")
    subprocess.run(command, cwd=ROOT, check=True)
    after = status_path.read_bytes()
    if after != before:
        raise ValueError("staged runtime status changed during bundle preflight verification")
    return target_dir, json.loads(after.decode("utf-8"))


def zip_info(name: str, executable: bool = False) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_DEFLATED
    mode = 0o755 if executable else 0o644
    info.external_attr = (mode & 0xFFFF) << 16
    info.create_system = 3
    return info


def bundled_distribution_review(
    target_dir: Path,
    status: dict[str, Any],
    entries: list[dict[str, Any]],
    profile: str,
) -> tuple[dict[str, Any] | None, bytes | None]:
    """Bind the review to the exact profile-specific bundle inventory.

    The reviewed staged inventory is first revalidated against the complete
    staged status. A core bundle then derives a deterministic subset containing
    only the reviewed core files; it never reuses a full inventory while
    stripping semantic payloads.
    """
    raw = status.get("distribution_review")
    if raw is None:
        return None, None
    if not isinstance(raw, dict) or raw.get("complete_portable_tree") is not True:
        raise ValueError("distribution_review must assert complete_portable_tree=true")
    review: dict[str, Any] = {"complete_portable_tree": True}
    for key in ("reviewer", "reviewed_at", "scope", "inventory_path", "inventory_sha256"):
        value = str(raw.get(key, "")).strip()
        if not value:
            raise ValueError(f"distribution_review is missing {key}")
        review[key] = value

    relative = Path(validate_relative_runtime_path(review["inventory_path"], "distribution inventory path"))
    source = target_dir / relative
    if not source.is_file():
        raise FileNotFoundError(f"staged distribution inventory is missing: {source}")
    source_bytes = source.read_bytes()
    actual = hashlib.sha256(source_bytes).hexdigest()
    if actual != review["inventory_sha256"].lower():
        raise ValueError("staged distribution inventory SHA-256 mismatch")
    source_inventory = json.loads(source_bytes.decode("utf-8"))
    if source_inventory.get("schema") != 1 or not isinstance(source_inventory.get("tools"), dict):
        raise ValueError("staged distribution inventory has an incompatible schema")

    staged_tools: dict[str, list[str]] = {}
    for item in status.get("files", []):
        tool = str(item.get("tool", "")).strip().lower()
        path = validate_relative_runtime_path(item.get("path"), "staged runtime path")
        staged_tools.setdefault(tool, []).append(path)
    staged_tools = {tool: sorted(paths) for tool, paths in sorted(staged_tools.items())}
    declared_tools = {
        str(tool).strip().lower(): sorted(
            validate_relative_runtime_path(path, f"distribution inventory {tool}")
            for path in paths
        )
        for tool, paths in source_inventory["tools"].items()
        if isinstance(paths, list)
    }
    if declared_tools != staged_tools:
        raise ValueError("staged runtime does not match the reviewed source inventory before profile filtering")

    profile_tools_map: dict[str, list[str]] = {}
    for item in entries:
        tool = str(item["tool"]).strip().lower()
        profile_tools_map.setdefault(tool, []).append(str(item["path"]))
    profile_inventory = {
        "schema": 1,
        "target": status.get("target"),
        "generated_by": "scripts/create_offline_runtime_bundle.py",
        "runtime_profile": profile,
        "tools": {tool: sorted(paths) for tool, paths in sorted(profile_tools_map.items())},
    }
    inventory_bytes = canonical_json(profile_inventory)
    review["inventory_path"] = f"_evidence/runtime-inventory-{profile}.json"
    review["inventory_sha256"] = hashlib.sha256(inventory_bytes).hexdigest()
    return review, inventory_bytes


def create_bundle(
    target: str,
    output_dir: Path,
    require_model: bool = False,
    require_supply_chain: bool = False,
    *,
    profile: str | None = None,
) -> tuple[Path, Path, dict[str, Any]]:
    """Build a deterministic runtime archive from an already verified stage.

    ``require_model`` remains for compatibility with older callers. New release
    entry points should pass ``profile`` explicitly. A core bundle filters
    semantic files out even when the local verified stage also contains them.
    """
    target = validate_target(target)
    selected_profile = profile or (FULL_PROFILE if require_model else CORE_PROFILE)
    selected_profile = normalize_profile(
        selected_profile,
        semantic_model_required=(selected_profile == FULL_PROFILE),
    )
    if require_model and selected_profile != FULL_PROFILE:
        raise ValueError("--require-semantic-model is incompatible with runtime profile 'core'")
    require_model = profile_requires_semantic(selected_profile)
    target_dir, status = load_verified_status(target, selected_profile, require_supply_chain)
    output_dir.mkdir(parents=True, exist_ok=True)
    entries: list[dict[str, Any]] = []
    for raw in status["files"]:
        if not include_tool(selected_profile, raw.get("tool")):
            continue
        relative = Path(validate_relative_runtime_path(raw["path"], "staged runtime path"))
        source = target_dir / relative
        actual = sha256_file(source)
        if actual != raw["sha256"]:
            raise ValueError(f"staged file changed after verification: {relative}")
        entry = {
            "tool": raw["tool"],
            "path": relative.as_posix(),
            "sha256": actual,
            "size_bytes": source.stat().st_size,
            "executable": bool(raw.get("executable", os.access(source, os.X_OK))),
        }
        for metadata_key in ("version", "source_url", "license", "license_path", "license_sha256"):
            if metadata_key in raw:
                entry[metadata_key] = raw[metadata_key]
        entries.append(entry)
    entries.sort(key=lambda item: item["path"])
    validate_profile_file_set(selected_profile, entries)

    license_entries = []
    seen_licenses = set()
    for item in entries:
        license_path = item.get("license_path")
        if not license_path or license_path in seen_licenses:
            continue
        seen_licenses.add(license_path)
        relative = Path(validate_relative_runtime_path(license_path, "staged license path"))
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
    distribution_review, inventory_bytes = bundled_distribution_review(
        target_dir, status, entries, selected_profile
    )
    sbom = {
        "schema": "dokkomplekt.offline-runtime.sbom.v1",
        "target": target,
        "runtime_profile": selected_profile,
        "network_used": False,
        "semantic_model_required": require_model,
        "supply_chain_locked": status.get("supply_chain_locked") is True,
        "files": entries,
        "license_notices": license_entries,
    }
    if distribution_review is not None:
        sbom["distribution_review"] = distribution_review
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
        if distribution_review is not None and inventory_bytes is not None:
            archive.writestr(
                zip_info(f"runtime/{target}/{distribution_review['inventory_path']}", False),
                inventory_bytes,
            )
    temporary.replace(output)
    payload = {
        "schema": "dokkomplekt.offline-runtime.signature.v1",
        "target": target,
        "runtime_profile": selected_profile,
        "bundle": output.name,
        "bundle_sha256": sha256_file(output),
        "bundle_size_bytes": output.stat().st_size,
        "sbom_sha256": hashlib.sha256(sbom_bytes).hexdigest(),
        "semantic_model_required": require_model,
        "supply_chain_locked": status.get("supply_chain_locked") is True,
        "distribution_review_bound": distribution_review is not None,
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
    parser.add_argument("--profile", choices=PROFILES)
    parser.add_argument("--require-semantic-model", action="store_true")
    parser.add_argument("--require-supply-chain", action="store_true")
    parser.add_argument("--signing-key", type=Path)
    parser.add_argument("--trusted-public-key", type=Path)
    parser.add_argument("--require-signature", action="store_true")
    args = parser.parse_args()

    selected_profile = args.profile or (FULL_PROFILE if args.require_semantic_model else CORE_PROFILE)
    if args.require_semantic_model and selected_profile != FULL_PROFILE:
        raise ValueError("--require-semantic-model is incompatible with --profile core")
    bundle, payload_path, payload = create_bundle(
        args.target,
        args.output_dir.resolve(),
        args.require_semantic_model,
        args.require_supply_chain,
        profile=selected_profile,
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
                "runtime_profile": payload["runtime_profile"],
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
