from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import sys
import zipfile
from pathlib import Path
from unittest import mock

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from scripts.verify_windows_hosted_signing_runner import check as hosted_check
from scripts.windows_runtime_bundle_approval import sign_payload, verify_payload

ROOT = Path(__file__).resolve().parents[1]
STAGE = ROOT / "scripts" / "stage_signed_runtime_bundle.py"


def pem_private(key: Ed25519PrivateKey) -> bytes:
    return key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )


def pem_public(key: Ed25519PrivateKey) -> bytes:
    return key.public_key().public_bytes(
        serialization.Encoding.PEM,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    )


def b64(value: bytes) -> str:
    return base64.b64encode(value).decode("ascii")


def public_build_env() -> dict[str, str]:
    return {
        "DOKKOMPLEKT_GATE_PUBKEY_B64": b64(b"g" * 32),
        "DOKKOMPLEKT_LICENSE_PUBKEY_B64": b64(b"l" * 32),
        "DOKKOMPLEKT_UPDATE_PUBKEY_B64": b64(b"u" * 32),
        "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64": b64(b"t" * 32),
        "DOKKOMPLEKT_REFDATA_PUBKEY_B64": b64(b"r" * 32),
        "DOKKOMPLEKT_UPDATE_MANIFEST_URL": "https://updates.dokkomplekt.ru/update.json",
        "DOKKOMPLEKT_REFDATA_URL": "https://updates.dokkomplekt.ru/reference-data.json",
        "DOKKOMPLEKT_COMPONENTS_CATALOG_URL": "https://downloads.dokkomplekt.ru/catalog.json",
        "DOKKOMPLEKT_COMPONENTS_BASE_URL": "https://downloads.dokkomplekt.ru/components",
    }


def test_hosted_signing_preflight_requires_github_hosted_windows_and_no_runner_manifest() -> None:
    runtime_key = Ed25519PrivateKey.generate()
    approval_key = Ed25519PrivateKey.generate()
    env = {
        **public_build_env(),
        "GITHUB_ACTIONS": "true",
        "RUNNER_OS": "Windows",
        "RUNNER_ENVIRONMENT": "github-hosted",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_URL": "https://downloads.dokkomplekt.ru/runtime/runtime.zip",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL": "https://downloads.dokkomplekt.ru/runtime/runtime.zip.signing.json",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL": "https://downloads.dokkomplekt.ru/runtime/runtime.zip.signing.json.sig",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL": "https://downloads.dokkomplekt.ru/runtime/runtime.zip.signing.json.approval.sig",
        "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64": b64(pem_public(runtime_key)),
        "DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64": b64(pem_public(approval_key)),
        "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND": "certificate-store",
        "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT": "A" * 40,
        "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER": "SafeNet Key Storage Provider",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64": b64(pem_private(runtime_key)),
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64": b64(b"gate"),
    }
    assert hosted_check(env)["ok"] is True

    pfx_env = {
        **env,
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": b64(b"pfx"),
    }
    pfx_report = hosted_check(pfx_env)
    assert pfx_report["ok"] is False
    assert any("forbidden on the hosted production signer" in error for error in pfx_report["errors"])

    env["DOKKOMPLEKT_SIDECAR_MANIFEST_PATH"] = r"C:\ProgramData\DokkomplektRuntime\manifest.json"
    report = hosted_check(env)
    assert report["ok"] is False
    assert any("forbidden on ephemeral hosted signing runner" in error for error in report["errors"])


def test_offline_bundle_approval_rejects_payload_tampering(tmp_path: Path) -> None:
    private = Ed25519PrivateKey.generate()
    private_path = tmp_path / "approval-private.pem"
    public_path = tmp_path / "approval-public.pem"
    private_path.write_bytes(pem_private(private))
    public_path.write_bytes(pem_public(private))
    payload = tmp_path / "runtime.zip.signing.json"
    payload.write_text(
        json.dumps(
            {
                "schema": "dokkomplekt.offline-runtime.signature.v1",
                "target": "windows-x86_64",
                "bundle": "runtime.zip",
                "bundle_sha256": "a" * 64,
                "bundle_size_bytes": 123,
                "sbom_sha256": "b" * 64,
                "semantic_model_required": True,
                "supply_chain_locked": True,
                "distribution_review_bound": True,
            }
        ),
        encoding="utf-8",
    )
    signature = tmp_path / "approval.sig"
    metadata = tmp_path / "approval.json"
    sign_payload(payload, private_path, signature, metadata, "release-reviewer")
    assert verify_payload(payload, signature, public_path)["ok"] is True
    payload.write_text(payload.read_text(encoding="utf-8") + "\n", encoding="utf-8")
    with pytest.raises(ValueError, match="approval signature verification failed"):
        verify_payload(payload, signature, public_path)


def build_runtime_fixture(tmp_path: Path) -> tuple[Path, Path, Path, Path, Path, Path]:
    runtime_key = Ed25519PrivateKey.generate()
    approval_key = Ed25519PrivateKey.generate()
    runtime_public = tmp_path / "runtime-public.pem"
    approval_public = tmp_path / "approval-public.pem"
    approval_private = tmp_path / "approval-private.pem"
    runtime_public.write_bytes(pem_public(runtime_key))
    approval_public.write_bytes(pem_public(approval_key))
    approval_private.write_bytes(pem_private(approval_key))

    runtime_file = b"MZ" + b"x" * 128
    license_data = b"license"
    inventory = json.dumps({"schema": 1, "tools": {"tesseract": ["tesseract/tesseract.exe"]}}, sort_keys=True).encode()
    inventory_path = "_evidence/runtime-inventory.json"
    sbom = {
        "schema": "dokkomplekt.offline-runtime.sbom.v1",
        "target": "windows-x86_64",
        "network_used": False,
        "semantic_model_required": True,
        "supply_chain_locked": True,
        "files": [
            {
                "tool": "tesseract",
                "path": "tesseract/tesseract.exe",
                "sha256": hashlib.sha256(runtime_file).hexdigest(),
                "size_bytes": len(runtime_file),
                "executable": True,
                "version": "fixture",
                "source_url": "https://downloads.dokkomplekt.ru/runtime/tesseract",
                "license": "fixture-license",
                "license_path": "_licenses/license.txt",
                "license_sha256": hashlib.sha256(license_data).hexdigest(),
            }
        ],
        "license_notices": [
            {
                "path": "_licenses/license.txt",
                "sha256": hashlib.sha256(license_data).hexdigest(),
                "size_bytes": len(license_data),
            }
        ],
        "distribution_review": {
            "complete_portable_tree": True,
            "reviewer": "release-reviewer",
            "reviewed_at": "2026-08-10",
            "scope": "fixture reviewed tree",
            "inventory_path": inventory_path,
            "inventory_sha256": hashlib.sha256(inventory).hexdigest(),
        },
    }
    sbom_bytes = (json.dumps(sbom, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
    bundle = tmp_path / "Dokkomplekt-offline-runtime-windows-x86_64.zip"
    with zipfile.ZipFile(bundle, "w") as archive:
        archive.writestr("runtime-sbom.json", sbom_bytes)
        archive.writestr("runtime/windows-x86_64/tesseract/tesseract.exe", runtime_file)
        archive.writestr("runtime/windows-x86_64/_licenses/license.txt", license_data)
        archive.writestr(f"runtime/windows-x86_64/{inventory_path}", inventory)
    payload_data = {
        "schema": "dokkomplekt.offline-runtime.signature.v1",
        "target": "windows-x86_64",
        "bundle": bundle.name,
        "bundle_sha256": hashlib.sha256(bundle.read_bytes()).hexdigest(),
        "bundle_size_bytes": bundle.stat().st_size,
        "sbom_sha256": hashlib.sha256(sbom_bytes).hexdigest(),
        "semantic_model_required": True,
        "supply_chain_locked": True,
        "distribution_review_bound": True,
    }
    payload = tmp_path / f"{bundle.name}.signing.json"
    payload.write_text(json.dumps(payload_data, ensure_ascii=False), encoding="utf-8")
    runtime_signature = tmp_path / f"{payload.name}.sig"
    runtime_signature.write_bytes(runtime_key.sign(payload.read_bytes()))
    approval_signature = tmp_path / f"{payload.name}.approval.sig"
    sign_payload(payload, approval_private, approval_signature, tmp_path / "approval.json", "release-reviewer")
    return bundle, payload, runtime_signature, runtime_public, approval_signature, approval_public


def test_stage_signed_bundle_requires_both_signatures_and_restores_review(tmp_path: Path) -> None:
    spec = importlib.util.spec_from_file_location("stage_signed_runtime_bundle", STAGE)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    bundle, payload, signature, runtime_public, approval_signature, approval_public = build_runtime_fixture(tmp_path)
    tools = tmp_path / "tools"
    report = tmp_path / "stage.json"
    argv = [
        str(STAGE),
        str(bundle),
        "--payload", str(payload),
        "--signature", str(signature),
        "--trusted-runtime-public-key", str(runtime_public),
        "--approval-signature", str(approval_signature),
        "--trusted-approval-public-key", str(approval_public),
        "--clean",
        "--json-report", str(report),
    ]
    with mock.patch.object(module, "TOOLS_ROOT", tools), mock.patch.object(sys, "argv", argv):
        assert module.main() == 0
    status = json.loads((tools / "windows-x86_64" / "sidecar-status.json").read_text(encoding="utf-8"))
    assert status["generated_by"] == "scripts/stage_signed_runtime_bundle.py"
    assert status["distribution_review"]["reviewer"] == "release-reviewer"
    assert report.is_file()

    approval_signature.write_bytes(b"0" * 64)
    with mock.patch.object(module, "TOOLS_ROOT", tmp_path / "tools-2"), mock.patch.object(sys, "argv", argv):
        with pytest.raises(ValueError, match="approval signature verification failed"):
            module.main()
