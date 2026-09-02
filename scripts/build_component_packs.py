#!/usr/bin/env python3
"""Build deterministic optional component packs and a signed catalog.

No dependency is downloaded. Input files must already be staged and verified in
`src-tauri/resources/tools/<target>/sidecar-status.json`. The resulting catalog
uses the same Ed25519 update key as application update manifests.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from ed25519_compat import SigningKey

try:
    from scripts._release_policy import validate_public_https_url, validate_relative_runtime_path
except ModuleNotFoundError:
    from _release_policy import validate_public_https_url, validate_relative_runtime_path

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
FIXED_ZIP_TIME = (2020, 1, 1, 0, 0, 0)
SAFE_TARGET = re.compile(r"^[A-Za-z0-9_-]+$")
SAFE_COMPONENT = re.compile(r"^[a-z0-9-]+$")
MAX_PACK_BYTES = 4 * 1024 * 1024 * 1024

COMPONENTS: dict[str, dict[str, Any]] = {
    "ocr": {
        "label": "Распознавание сканов (OCR)",
        "description": "Tesseract rus+eng и Poppler для изображений и PDF без текстового слоя.",
        "tools": {"tesseract", "poppler"},
        "unlocks": ["tesseract", "pdftotext", "pdftoppm"],
    },
    "office": {
        "label": "Конвертация и печать",
        "description": "LibreOffice для PDF/XLS/ODS и SumatraPDF для управляемой печати PDF.",
        "tools": {"libreoffice", "sumatrapdf"},
        "unlocks": ["soffice", "sumatrapdf"],
    },
    "archive": {
        "label": "Распаковка входящих архивов",
        "description": "Проверенный локальный 7-Zip для ZIP/7Z/RAR без передачи документов в сеть.",
        "tools": {"7zip"},
        "unlocks": ["7z"],
    },
    "semantic": {
        "label": "Локальная модель понимания текста",
        "description": "llama.cpp server и проверенная GGUF-модель; документы не уходят в сеть.",
        "tools": {"llama_cpp", "semantic_model"},
        "unlocks": ["llama_cpp", "semantic_model"],
    },
}


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def zip_info(name: str, executable: bool = False) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, FIXED_ZIP_TIME)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = ((0o755 if executable else 0o644) & 0xFFFF) << 16
    info.create_system = 3
    return info


def safe_relative(raw: str) -> Path:
    return Path(validate_relative_runtime_path(raw, "unsafe component path"))


def load_status(target: str) -> tuple[Path, dict[str, Any]]:
    target_dir = (TOOLS_ROOT / target).resolve()
    target_dir.relative_to(TOOLS_ROOT.resolve())
    status_path = target_dir / "sidecar-status.json"
    if not status_path.is_file():
        raise FileNotFoundError(f"verified sidecar status not found: {status_path}")
    status = json.loads(status_path.read_text("utf-8"))
    if status.get("target") != target or not isinstance(status.get("files"), list):
        raise ValueError("sidecar-status.json does not match target")
    return target_dir, status


def component_entries(target_dir: Path, status: dict[str, Any], component_id: str) -> list[dict[str, Any]]:
    spec = COMPONENTS[component_id]
    entries: list[dict[str, Any]] = []
    for raw in status["files"]:
        if raw.get("tool") not in spec["tools"]:
            continue
        relative = safe_relative(str(raw["path"]))
        source = target_dir / relative
        if not source.is_file():
            raise FileNotFoundError(f"staged component file missing: {source}")
        actual = sha256_file(source)
        if actual.lower() != str(raw["sha256"]).lower():
            raise ValueError(f"staged file changed after verification: {relative}")
        entries.append(
            {
                "path": relative.as_posix(),
                "sha256": actual,
                "size_bytes": source.stat().st_size,
                "executable": bool(raw.get("executable", os.access(source, os.X_OK))),
            }
        )
    if not entries:
        raise ValueError(f"component {component_id} has no staged files")
    entries.sort(key=lambda item: item["path"])
    return entries


def build_pack(target: str, target_dir: Path, status: dict[str, Any], component_id: str, out: Path) -> dict[str, Any]:
    entries = component_entries(target_dir, status, component_id)
    files_manifest = {
        "schema": 1,
        "component_id": component_id,
        "target": target,
        "files": {item["path"]: item["sha256"] for item in entries},
    }
    manifest_bytes = canonical_bytes(files_manifest)
    output = out / f"{component_id}-{target}.zip"
    temporary = output.with_suffix(".zip.tmp")
    with zipfile.ZipFile(temporary, "w", allowZip64=True) as archive:
        archive.writestr(zip_info("component-files.json"), manifest_bytes)
        for item in entries:
            archive.writestr(
                zip_info(item["path"], item["executable"]),
                (target_dir / item["path"]).read_bytes(),
            )
    temporary.replace(output)
    size = output.stat().st_size
    if not 0 < size <= MAX_PACK_BYTES:
        raise ValueError(f"component pack has invalid size: {output} ({size})")
    return {
        "path": output,
        "sha256": sha256_file(output),
        "size_bytes": size,
        "files_manifest_sha256": sha256_bytes(manifest_bytes),
    }


def build_offline_bundle(target: str, catalog: Path, descriptors: list[dict[str, Any]], out: Path) -> Path:
    """Bundle the signed catalog and its exact component archives for local import.

    The outer ZIP adds no new trust anchor: the desktop verifies the catalog with
    its baked update key, then verifies every inner archive against the signed
    descriptor size/SHA-256 and component-files manifest hash.
    """
    expected: list[Path] = []
    for descriptor in descriptors:
        name = str(descriptor.get("archive_name", "")).strip()
        if not name or name != f"{descriptor['id']}-{target}.zip":
            raise ValueError(f"component descriptor has a non-canonical archive name: {name!r}")
        if descriptor.get("url"):
            parsed = urlparse(str(descriptor["url"]))
            if Path(parsed.path).name != name:
                raise ValueError("component URL and archive_name disagree")
        archive = out / name
        if not archive.is_file():
            raise FileNotFoundError(f"component archive is missing before offline bundling: {archive}")
        expected.append(archive)

    output = out / f"Dokkomplekt-components-offline-{target}.zip"
    temporary = output.with_suffix(".zip.tmp")
    with zipfile.ZipFile(temporary, "w", allowZip64=True) as archive:
        archive.writestr(zip_info("components-catalog.json"), catalog.read_bytes())
        for component in sorted(expected, key=lambda item: item.name):
            archive.writestr(zip_info(component.name), component.read_bytes())
    temporary.replace(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{sha256_file(output)}  {output.name}\n", "utf-8"
    )
    return output


def signing_key_from_environment() -> SigningKey:
    raw = os.environ.get("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64", "").strip()
    if not raw:
        raise ValueError("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64 is required")
    try:
        seed = base64.b64decode(raw, validate=True)
        if len(seed) != 32:
            raise ValueError
        return SigningKey(seed)
    except Exception as exc:
        raise ValueError("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64 must contain a 32-byte Ed25519 seed") from exc


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    parser.add_argument("--components", default="ocr,office,semantic,archive")
    parser.add_argument("--app-min-version", default="18.3.0")
    parser.add_argument("--base-url", default="", help="Optional HTTPS directory URL; omit for an offline-only signed bundle")
    parser.add_argument("--out", type=Path, default=ROOT / "release-components")
    parser.add_argument("--require-trusted-public-key", action="store_true", help="Require DOKKOMPLEKT_UPDATE_PUBKEY_B64 and verify it matches the signing key")
    args = parser.parse_args()

    target = args.target.strip()
    if not SAFE_TARGET.fullmatch(target):
        raise ValueError("unsafe target")
    selected = [item.strip() for item in args.components.split(",") if item.strip()]
    if not selected or any(not SAFE_COMPONENT.fullmatch(item) or item not in COMPONENTS for item in selected):
        raise ValueError("--components contains an unknown component")
    if len(selected) != len(set(selected)):
        raise ValueError("--components contains duplicate component ids")
    base_url = args.base_url.strip().rstrip("/")
    parsed = None
    if base_url:
        base_url = validate_public_https_url(base_url, "--base-url")
        parsed = urlparse(base_url)

    target_dir, status = load_status(target)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    descriptors: list[dict[str, Any]] = []
    for component_id in selected:
        built = build_pack(target, target_dir, status, component_id, out)
        spec = COMPONENTS[component_id]
        descriptors.append(
            {
                "id": component_id,
                "label": spec["label"],
                "description": spec["description"],
                "unlocks": spec["unlocks"],
                "target": target,
                "size_bytes": built["size_bytes"],
                "sha256": built["sha256"],
                "files_manifest_sha256": built["files_manifest_sha256"],
                "archive_name": built["path"].name,
                "url": f"{base_url}/{built['path'].name}" if base_url else "",
            }
        )

    payload = {
        "schema": 1,
        "app_min_version": args.app_min_version,
        "published_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "catalog_scope": "complete" if set(selected) == set(COMPONENTS) else "partial",
        "allowed_hosts": [parsed.hostname.lower()] if parsed and parsed.hostname else [],
        "components": sorted(descriptors, key=lambda item: (item["target"], item["id"])),
    }
    key = signing_key_from_environment()
    trusted_public_raw = os.environ.get("DOKKOMPLEKT_UPDATE_PUBKEY_B64", "").strip()
    if args.require_trusted_public_key and not trusted_public_raw:
        raise ValueError("DOKKOMPLEKT_UPDATE_PUBKEY_B64 is required for a release catalog")
    if trusted_public_raw:
        try:
            trusted_public = base64.b64decode(trusted_public_raw, validate=True)
        except Exception as exc:
            raise ValueError("DOKKOMPLEKT_UPDATE_PUBKEY_B64 must be valid base64") from exc
        if len(trusted_public) != 32 or trusted_public != bytes(key.verify_key):
            raise ValueError("component catalog signing key does not match DOKKOMPLEKT_UPDATE_PUBKEY_B64")
    signature = key.sign(canonical_bytes(payload)).signature
    document = {
        "payload": payload,
        "signature_alg": "ed25519",
        "signature": base64.b64encode(signature).decode("ascii"),
    }
    catalog = out / "components-catalog.json"
    catalog.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", "utf-8")
    catalog.with_suffix(".json.sig").write_bytes(signature)
    catalog.with_suffix(".json.sha256").write_text(f"{sha256_file(catalog)}  {catalog.name}\n", "utf-8")
    offline_bundle = build_offline_bundle(target, catalog, descriptors, out)
    print(json.dumps({
        "catalog": str(catalog),
        "offline_bundle": str(offline_bundle),
        "components": [item["id"] for item in descriptors],
        "public_key_b64": base64.b64encode(bytes(key.verify_key)).decode("ascii"),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
