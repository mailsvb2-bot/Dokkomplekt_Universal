from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "windows_signed_handoff.py"


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def make_keys(tmp_path: Path) -> tuple[Path, Path]:
    private = Ed25519PrivateKey.generate()
    private_path = tmp_path / "private.pem"
    public_path = tmp_path / "public.pem"
    private_path.write_bytes(
        private.private_bytes(
            serialization.Encoding.PEM,
            serialization.PrivateFormat.PKCS8,
            serialization.NoEncryption(),
        )
    )
    public_path.write_bytes(
        private.public_key().public_bytes(
            serialization.Encoding.PEM,
            serialization.PublicFormat.SubjectPublicKeyInfo,
        )
    )
    return private_path, public_path


def test_signed_handoff_round_trip_and_tamper_rejection(tmp_path: Path) -> None:
    root = tmp_path / "handoff"
    (root / "installer").mkdir(parents=True)
    (root / "runtime").mkdir(parents=True)
    (root / "installer" / "setup.exe").write_bytes(b"signed-installer-fixture")
    (root / "runtime" / "runtime.zip").write_bytes(b"signed-runtime-fixture")
    private_key, public_key = make_keys(tmp_path)
    release_sha = "a" * 40
    request_id = "11111111-2222-3333-4444-555555555555"

    built = run(
        "build",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--signing-key",
        str(private_key),
    )
    assert built.returncode == 0, built.stderr
    assert (root / "SIGNED_HANDOFF.json").is_file()
    assert (root / "SIGNED_HANDOFF.json.sig").is_file()

    report = tmp_path / "verified.json"
    verified = run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
        "--json-report",
        str(report),
    )
    assert verified.returncode == 0, verified.stderr
    payload = json.loads(report.read_text(encoding="utf-8"))
    assert payload["ok"] is True
    assert payload["files"] == 2

    (root / "runtime" / "runtime.zip").write_bytes(b"tampered")
    rejected = run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
    )
    assert rejected.returncode != 0
    rejection = rejected.stdout + rejected.stderr
    assert "size mismatch" in rejection or "sha256 mismatch" in rejection


def test_signed_handoff_rejects_wrong_identity_and_unexpected_files(tmp_path: Path) -> None:
    root = tmp_path / "handoff"
    root.mkdir()
    (root / "artifact.bin").write_bytes(b"payload")
    private_key, public_key = make_keys(tmp_path)
    release_sha = "b" * 40
    request_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    assert (
        run(
            "build",
            str(root),
            "--release-sha",
            release_sha,
            "--request-id",
            request_id,
            "--signing-key",
            str(private_key),
        ).returncode
        == 0
    )

    wrong_sha = run(
        "verify",
        str(root),
        "--release-sha",
        "c" * 40,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
    )
    assert wrong_sha.returncode != 0
    assert "release_sha mismatch" in (wrong_sha.stdout + wrong_sha.stderr)

    (root / "unexpected.bin").write_bytes(b"unexpected")
    unexpected = run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
    )
    assert unexpected.returncode != 0
    assert "file set mismatch" in (unexpected.stdout + unexpected.stderr)
