#!/usr/bin/env python3
"""Fail-closed preflight for the ephemeral GitHub-hosted Windows signing job."""
from __future__ import annotations

import argparse
import base64
import json
import os
import re
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

try:
    from scripts._release_policy import validate_public_https_url
    from scripts.release_environment_preflight import check as release_check
except ModuleNotFoundError:
    from _release_policy import validate_public_https_url
    from release_environment_preflight import check as release_check

URL_VARS = (
    "DOKKOMPLEKT_RUNTIME_BUNDLE_URL",
    "DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL",
    "DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL",
    "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL",
)
PEM_PUBLIC_VARS = (
    "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64",
    "DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64",
)
BASE_SIGNING_SECRET_VARS = ("DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",)
HANDOFF_SIGNING_SECRET = "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64"
SIGNING_BACKEND_VAR = "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND"
CERT_THUMBPRINT_VAR = "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT"
ALLOWED_PROVIDER_VAR = "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER"
LEGACY_PFX_VARS = (
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
    "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
)
THUMBPRINT_RE = re.compile(r"^[0-9A-Fa-f]{40,128}$")


def decode_ed25519_public_pem(value: str, label: str) -> None:
    try:
        raw = base64.b64decode(value, validate=True)
        key = serialization.load_pem_public_key(raw)
    except Exception as exc:
        raise ValueError(f"{label}: invalid base64 Ed25519 PEM") from exc
    if not isinstance(key, Ed25519PublicKey):
        raise ValueError(f"{label}: key must be Ed25519")


def validate_windows_signing_backend(env: dict[str, str]) -> tuple[list[str], list[str]]:
    checked = [SIGNING_BACKEND_VAR, CERT_THUMBPRINT_VAR, ALLOWED_PROVIDER_VAR]
    errors: list[str] = []

    backend = env.get(SIGNING_BACKEND_VAR, "").strip().lower()
    if backend != "certificate-store":
        errors.append(
            f"{SIGNING_BACKEND_VAR}: hosted production signing requires 'certificate-store'; "
            f"got {backend or '<empty>'!r}"
        )

    thumbprint = re.sub(r"\s+", "", env.get(CERT_THUMBPRINT_VAR, ""))
    if not THUMBPRINT_RE.fullmatch(thumbprint):
        errors.append(f"{CERT_THUMBPRINT_VAR}: missing or invalid certificate thumbprint")

    provider = env.get(ALLOWED_PROVIDER_VAR, "").strip()
    if not provider:
        errors.append(f"{ALLOWED_PROVIDER_VAR}: missing")
    elif provider.lower() in {
        "microsoft software key storage provider",
        "microsoft enhanced rsa and aes cryptographic provider",
        "microsoft enhanced cryptographic provider v1.0",
        "microsoft base cryptographic provider v1.0",
    }:
        errors.append(f"{ALLOWED_PROVIDER_VAR}: software-backed provider is forbidden")

    for name in LEGACY_PFX_VARS:
        checked.append(f"{name} absent")
        if env.get(name, "").strip():
            errors.append(
                f"{name}: forbidden on the hosted production signer; use a non-exportable HSM/KSP key"
            )

    return checked, errors


def check(env: dict[str, str], *, require_handoff_signing_key: bool = False) -> dict[str, object]:
    errors: list[str] = []
    checked: list[str] = []
    production = release_check("production-build", env)
    checked.extend(str(item) for item in production.get("checked", []))
    errors.extend(str(item) for item in production.get("errors", []))

    expected = {
        "GITHUB_ACTIONS": "true",
        "RUNNER_OS": "Windows",
        "RUNNER_ENVIRONMENT": "github-hosted",
    }
    for name, wanted in expected.items():
        checked.append(name)
        actual = env.get(name, "").strip()
        if actual.lower() != wanted.lower():
            errors.append(f"{name}: expected {wanted!r}, got {actual or '<empty>'!r}")

    if env.get("DOKKOMPLEKT_SIDECAR_MANIFEST_PATH", "").strip():
        errors.append("DOKKOMPLEKT_SIDECAR_MANIFEST_PATH: forbidden on ephemeral hosted signing runner")
    checked.append("DOKKOMPLEKT_SIDECAR_MANIFEST_PATH absent")

    for name in URL_VARS:
        checked.append(name)
        value = env.get(name, "").strip()
        if not value:
            errors.append(f"{name}: missing")
            continue
        try:
            validate_public_https_url(value, name)
        except ValueError as exc:
            errors.append(str(exc))

    for name in PEM_PUBLIC_VARS:
        checked.append(name)
        value = env.get(name, "").strip()
        if not value:
            errors.append(f"{name}: missing")
            continue
        try:
            decode_ed25519_public_pem(value, name)
        except ValueError as exc:
            errors.append(str(exc))

    signing_checked, signing_errors = validate_windows_signing_backend(env)
    checked.extend(signing_checked)
    errors.extend(signing_errors)

    required_secrets = list(BASE_SIGNING_SECRET_VARS)
    if require_handoff_signing_key:
        required_secrets.append(HANDOFF_SIGNING_SECRET)
    for name in required_secrets:
        checked.append(name)
        value = env.get(name, "").strip()
        if not value:
            errors.append(f"{name}: missing")
    if not require_handoff_signing_key:
        checked.append(f"{HANDOFF_SIGNING_SECRET} not required for release-only hosted signing")

    return {
        "schema": "dokkomplekt.windows-hosted-signing-preflight.v2",
        "ok": not errors,
        "ephemeral_hosted_windows": not errors or (
            env.get("GITHUB_ACTIONS", "").lower() == "true"
            and env.get("RUNNER_OS", "").lower() == "windows"
            and env.get("RUNNER_ENVIRONMENT", "").lower() == "github-hosted"
        ),
        "windows_signing_backend": env.get(SIGNING_BACKEND_VAR, "").strip().lower(),
        "handoff_signing_key_required": require_handoff_signing_key,
        "checked": checked,
        "errors": errors,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json-report", type=Path)
    parser.add_argument("--require-handoff-signing-key", action="store_true")
    args = parser.parse_args()
    report = check(dict(os.environ), require_handoff_signing_key=args.require_handoff_signing_key)
    payload = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(payload, encoding="utf-8")
    print(payload, end="")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
