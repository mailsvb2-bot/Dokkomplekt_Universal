#!/usr/bin/env python3
"""Fetch the immutable production runtime inputs for ephemeral hosted signing.

All locations are protected repository/environment variables and must be public
HTTPS URLs.  This helper bounds metadata/signature sizes, streams the runtime
bundle, and re-validates the final redirect URL before files are consumed by the
cryptographic staging gate.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path

try:
    from scripts._release_policy import validate_public_https_url
except ModuleNotFoundError:
    from _release_policy import validate_public_https_url

MAX_BUNDLE_BYTES = 8 * 1024 * 1024 * 1024
MAX_METADATA_BYTES = 2 * 1024 * 1024
MAX_SIGNATURE_BYTES = 4096
CHUNK = 1024 * 1024


def fetch(url: str, output: Path, limit: int, label: str) -> dict[str, object]:
    validate_public_https_url(url, label)
    request = urllib.request.Request(url, headers={"User-Agent": "Dokkomplekt-release/1"})
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name + ".part")
    total = 0
    try:
        with urllib.request.urlopen(request, timeout=60) as response, temporary.open("wb") as stream:
            final_url = response.geturl()
            validate_public_https_url(final_url, f"{label} final URL")
            length = response.headers.get("Content-Length")
            if length:
                declared = int(length)
                if declared <= 0 or declared > limit:
                    raise ValueError(f"{label}: Content-Length outside allowed range")
            while True:
                chunk = response.read(CHUNK)
                if not chunk:
                    break
                total += len(chunk)
                if total > limit:
                    raise ValueError(f"{label}: response exceeds size limit")
                stream.write(chunk)
            stream.flush()
            os.fsync(stream.fileno())
        if total <= 0:
            raise ValueError(f"{label}: downloaded file is empty")
        os.replace(temporary, output)
        return {"url": url, "final_url": final_url, "path": str(output), "bytes": total}
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle-url", required=True)
    parser.add_argument("--payload-url", required=True)
    parser.add_argument("--signature-url", required=True)
    parser.add_argument("--approval-signature-url", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    out = args.out.resolve()
    bundle = out / "Dokkomplekt-offline-runtime-windows-x86_64.zip"
    payload = out / "Dokkomplekt-offline-runtime-windows-x86_64.zip.signing.json"
    signature = out / "Dokkomplekt-offline-runtime-windows-x86_64.zip.signing.json.sig"
    approval = out / "Dokkomplekt-offline-runtime-windows-x86_64.zip.signing.json.approval.sig"
    downloads = [
        fetch(args.bundle_url, bundle, MAX_BUNDLE_BYTES, "runtime bundle URL"),
        fetch(args.payload_url, payload, MAX_METADATA_BYTES, "runtime payload URL"),
        fetch(args.signature_url, signature, MAX_SIGNATURE_BYTES, "runtime signature URL"),
        fetch(args.approval_signature_url, approval, MAX_SIGNATURE_BYTES, "runtime approval signature URL"),
    ]
    report = {"schema": "dokkomplekt.hosted-runtime-fetch.v1", "ok": True, "downloads": downloads}
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
