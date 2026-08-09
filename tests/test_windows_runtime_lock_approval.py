from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.rsa import generate_private_key


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "windows_runtime_lock_approval.py"


def load_module():
    spec = importlib.util.spec_from_file_location("windows_runtime_lock_approval", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_manifest(path: Path) -> None:
    path.write_text(
        json.dumps(
            {
                "schema": 1,
                "target": "windows-x86_64",
                "supply_chain_locked": True,
                "files": [
                    {
                        "tool": "tesseract",
                        "source": "C:/runtime/tesseract.exe",
                        "target": "tesseract/tesseract.exe",
                        "sha256": "0" * 64,
                        "version": "5.0-test",
                        "source_url": "https://example.invalid/tesseract",
                        "license": "Apache-2.0",
                        "license_file": "C:/runtime/LICENSE.txt",
                        "license_sha256": "1" * 64,
                    }
                ],
                "distribution_review": {
                    "complete_portable_tree": True,
                    "reviewer": "fixture",
                    "reviewed_at": "2026-01-01",
                    "scope": "fixture",
                    "inventory_file": "C:/runtime/inventory.json",
                    "inventory_sha256": "2" * 64,
                },
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def write_ed25519_pair(root: Path, prefix: str = "approval") -> tuple[Path, Path]:
    private_key = Ed25519PrivateKey.generate()
    private_path = root / f"{prefix}-private.pem"
    public_path = root / f"{prefix}-public.pem"
    private_path.write_bytes(
        private_key.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )
    )
    public_path.write_bytes(
        private_key.public_key().public_bytes(
            serialization.Encoding.PEM,
            serialization.PublicFormat.SubjectPublicKeyInfo,
        )
    )
    return private_path, public_path


def test_offline_approval_signs_exact_manifest_and_verifies() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        manifest = root / "windows-x86_64-manifest.json"
        write_manifest(manifest)
        private_key, public_key = write_ed25519_pair(root)
        signature = root / "windows-x86_64-manifest.json.sig"
        metadata = root / "windows-x86_64-manifest.json.approval.json"

        signed = module.sign_manifest(
            manifest, private_key, signature, metadata, "release-reviewer"
        )
        verified = module.verify_manifest(manifest, signature, public_key)

        assert signature.stat().st_size == 64
        assert signed["algorithm"] == "Ed25519"
        assert signed["private_key_present_on_runner"] is False
        assert signed["manifest_sha256"] == verified["manifest_sha256"]
        assert signed["approval_key_id"] == verified["approval_key_id"]
        assert verified["ok"] is True
        assert json.loads(metadata.read_text(encoding="utf-8"))["reviewer"] == "release-reviewer"


def test_manifest_tampering_after_approval_is_rejected() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        manifest = root / "windows-x86_64-manifest.json"
        write_manifest(manifest)
        private_key, public_key = write_ed25519_pair(root)
        signature = root / "lock.sig"
        metadata = root / "lock.approval.json"
        module.sign_manifest(manifest, private_key, signature, metadata, "reviewer")

        manifest.write_bytes(manifest.read_bytes() + b"\n")
        with pytest.raises(ValueError, match="signature verification failed"):
            module.verify_manifest(manifest, signature, public_key)


def test_wrong_trusted_public_key_is_rejected() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        manifest = root / "windows-x86_64-manifest.json"
        write_manifest(manifest)
        private_key, _ = write_ed25519_pair(root, "signer")
        _, wrong_public_key = write_ed25519_pair(root, "wrong")
        signature = root / "lock.sig"
        metadata = root / "lock.approval.json"
        module.sign_manifest(manifest, private_key, signature, metadata, "reviewer")

        with pytest.raises(ValueError, match="signature verification failed"):
            module.verify_manifest(manifest, signature, wrong_public_key)


def test_non_ed25519_approval_key_is_rejected() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        manifest = root / "windows-x86_64-manifest.json"
        write_manifest(manifest)
        rsa_key = generate_private_key(public_exponent=65537, key_size=2048)
        rsa_path = root / "rsa.pem"
        rsa_path.write_bytes(
            rsa_key.private_bytes(
                serialization.Encoding.PEM,
                serialization.PrivateFormat.PKCS8,
                serialization.NoEncryption(),
            )
        )
        with pytest.raises(ValueError, match="must be Ed25519"):
            module.sign_manifest(
                manifest,
                rsa_path,
                root / "lock.sig",
                root / "lock.approval.json",
                "reviewer",
            )
