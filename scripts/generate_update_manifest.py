#!/usr/bin/env python3
"""Create a signed Dokkomplekt update manifest.

The private Ed25519 seed is read only from DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64.
The matching public key is compiled into the desktop application via
DOKKOMPLEKT_UPDATE_PUBKEY_B64. Neither key nor manifest URL is accepted from UI.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

try:
    from ed25519_compat import SigningKey
except ImportError as exc:  # pragma: no cover
    raise SystemExit("cryptography is required: python -m pip install -r requirements-dev.txt") from exc

try:
    from scripts._release_policy import validate_public_https_url
except ModuleNotFoundError:
    from _release_policy import validate_public_https_url

MAX_ARTIFACT_BYTES = 512 * 1024 * 1024
PLATFORMS = {
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
    "linux-aarch64",
    "macos-x86_64",
    "macos-aarch64",
}
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_artifact(raw: str) -> dict[str, Any]:
    try:
        platform, file_name, url = raw.split("=", 2)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("artifact must be PLATFORM=FILE=HTTPS_URL") from exc
    platform = platform.strip()
    path = Path(file_name).expanduser().resolve()
    url = url.strip()
    if platform not in PLATFORMS:
        raise argparse.ArgumentTypeError(f"unsupported platform: {platform}")
    if not path.is_file():
        raise argparse.ArgumentTypeError(f"artifact does not exist: {path}")
    try:
        url = validate_public_https_url(url, "artifact URL")
    except ValueError as exc:
        raise argparse.ArgumentTypeError(str(exc)) from exc
    size = path.stat().st_size
    if size <= 0 or size > MAX_ARTIFACT_BYTES:
        raise argparse.ArgumentTypeError(f"artifact size must be 1..{MAX_ARTIFACT_BYTES} bytes")
    return {
        "platform": platform,
        "url": url,
        "sha256": file_sha256(path),
        "size_bytes": size,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True, help="SemVer, for example 18.0.7")
    parser.add_argument("--artifact", action="append", required=True, type=parse_artifact,
                        help="PLATFORM=FILE=HTTPS_URL; repeat for every platform")
    parser.add_argument("--notes", default="")
    parser.add_argument("--output", type=Path, default=Path("update-manifest.json"))
    args = parser.parse_args()

    version = args.version.strip()
    if not SEMVER_RE.fullmatch(version):
        raise SystemExit("--version must be a valid SemVer value, for example 18.0.7")

    private_b64 = os.environ.get("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64", "").strip()
    if not private_b64:
        raise SystemExit("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64 is required")
    try:
        private_seed = base64.b64decode(private_b64, validate=True)
        signing_key = SigningKey(private_seed)
    except Exception as exc:
        raise SystemExit("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64 must contain a 32-byte Ed25519 seed") from exc

    payload = {
        "schema": "dokkomplekt.update.v1",
        "version": version,
        "published_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "notes": args.notes.strip() or None,
        "platforms": sorted(args.artifact, key=lambda item: item["platform"]),
    }
    signature = signing_key.sign(canonical_bytes(payload)).signature
    document = {
        "payload": payload,
        "signature_alg": "ed25519",
        "signature": base64.b64encode(signature).decode("ascii"),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    print("DOKKOMPLEKT_UPDATE_PUBKEY_B64=" + base64.b64encode(bytes(signing_key.verify_key)).decode("ascii"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
