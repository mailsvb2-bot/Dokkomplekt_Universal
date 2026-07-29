#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
from urllib.parse import urlparse

RUNTIME_REQUIRED = (
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
    "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
    "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64",
    "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
    "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
    "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
    "DOKKOMPLEKT_COMPONENTS_BASE_URL",
    "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH",
)
HARDWARE_REQUIRED = (
    "DOKKOMPLEKT_TEST_PRINTER",
    "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH",
)
BASE64_VARS = {
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
    "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
    "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64",
    "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
    "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
}
URL_VARS = {
    "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
    "DOKKOMPLEKT_COMPONENTS_BASE_URL",
}


def check(mode: str, env: dict[str, str]) -> dict[str, object]:
    required = RUNTIME_REQUIRED if mode == "windows-runtime" else HARDWARE_REQUIRED
    errors: list[str] = []
    checked: list[str] = []
    for name in required:
        value = env.get(name, "").strip()
        checked.append(name)
        if not value:
            errors.append(f"{name}: missing")
            continue
        if name in BASE64_VARS:
            try:
                decoded = base64.b64decode(value, validate=True)
                if not decoded:
                    errors.append(f"{name}: decoded value is empty")
            except Exception:
                errors.append(f"{name}: invalid base64")
        if name in URL_VARS:
            parsed = urlparse(value)
            if parsed.scheme != "https" or not parsed.netloc or parsed.hostname == "invalid" or value.endswith(".invalid"):
                errors.append(f"{name}: must be a real HTTPS URL")
        if name == "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH":
            path = Path(value)
            if not path.is_absolute():
                errors.append(f"{name}: must be an absolute runner-owned path")
            elif not path.is_file():
                errors.append(f"{name}: file does not exist")
        if name == "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH":
            path = Path(value)
            if not path.is_absolute():
                errors.append(f"{name}: must be an absolute path")
    timestamp = env.get("DOKKOMPLEKT_TIMESTAMP_SERVER", "").strip()
    if mode == "windows-runtime" and timestamp:
        parsed = urlparse(timestamp)
        if parsed.scheme != "https" or not parsed.netloc:
            errors.append("DOKKOMPLEKT_TIMESTAMP_SERVER: must use HTTPS")
    return {
        "schema": "dokkomplekt.release-environment-preflight.v1",
        "mode": mode,
        "ok": not errors,
        "checked": checked,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("windows-runtime", "windows-hardware"), required=True)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()
    report = check(args.mode, dict(os.environ))
    payload = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(payload, encoding="utf-8")
    print(payload, end="")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
