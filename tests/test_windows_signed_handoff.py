from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "windows_signed_handoff.py"
PRODUCER_HOST_ID = "1" * 64
CONSUMER_HOST_ID = "2" * 64


def run(*args: str, cwd: Path = ROOT) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        cwd=cwd,
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


def make_producer_repository(tmp_path: Path) -> Path:
    repository = tmp_path / "runtime-host"
    cargo_gate = repository / ".cargo-gate"
    release_gate = repository / ".release-gate"
    cargo_gate.mkdir(parents=True)
    release_gate.mkdir(parents=True)
    (cargo_gate / "CARGO_GATE_ATTESTATION.json").write_text(
        '{"schema":"fixture.cargo-gate"}\n', encoding="utf-8"
    )
    (cargo_gate / "CARGO_GATE_ATTESTATION.sig").write_bytes(b"fixture-gate-signature")
    (release_gate / "WINDOWS_SIGNED_BUILD_PASSED.json").write_text(
        '{"schema":"fixture.signed-build"}\n', encoding="utf-8"
    )
    return repository


def make_consumer_repository(tmp_path: Path) -> Path:
    repository = tmp_path / "hardware-host"
    repository.mkdir(parents=True)
    return repository


def build_handoff(
    root: Path,
    private_key: Path,
    release_sha: str,
    request_id: str,
    producer_repository: Path,
) -> subprocess.CompletedProcess[str]:
    return run(
        "build",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--signing-key",
        str(private_key),
        "--producer-host-id",
        PRODUCER_HOST_ID,
        cwd=producer_repository,
    )


def verify_handoff(
    root: Path,
    public_key: Path,
    release_sha: str,
    request_id: str,
    consumer_repository: Path,
    *extra: str,
) -> subprocess.CompletedProcess[str]:
    return run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
        "--consumer-host-id",
        CONSUMER_HOST_ID,
        *extra,
        cwd=consumer_repository,
    )


def test_signed_handoff_round_trip_restores_gate_evidence_and_rejects_tamper(
    tmp_path: Path,
) -> None:
    root = tmp_path / "handoff"
    (root / "installer").mkdir(parents=True)
    (root / "runtime").mkdir(parents=True)
    (root / "installer" / "setup.exe").write_bytes(b"signed-installer-fixture")
    (root / "runtime" / "runtime.zip").write_bytes(b"signed-runtime-fixture")
    producer_repository = make_producer_repository(tmp_path)
    consumer_repository = make_consumer_repository(tmp_path)
    private_key, public_key = make_keys(tmp_path)
    release_sha = "a" * 40
    request_id = "11111111-2222-3333-4444-555555555555"

    built = build_handoff(
        root, private_key, release_sha, request_id, producer_repository
    )
    assert built.returncode == 0, built.stderr
    assert (root / "SIGNED_HANDOFF.json").is_file()
    assert (root / "SIGNED_HANDOFF.json.sig").is_file()
    assert (
        root / "build-evidence" / "cargo-gate" / "CARGO_GATE_ATTESTATION.json"
    ).is_file()
    assert (
        root / "build-evidence" / "release-gate" / "WINDOWS_SIGNED_BUILD_PASSED.json"
    ).is_file()

    report = tmp_path / "verified.json"
    verified = verify_handoff(
        root,
        public_key,
        release_sha,
        request_id,
        consumer_repository,
        "--json-report",
        str(report),
    )
    assert verified.returncode == 0, verified.stderr
    payload = json.loads(report.read_text(encoding="utf-8"))
    assert payload["ok"] is True
    assert payload["files"] >= 5
    assert payload["producer_host_id"] == PRODUCER_HOST_ID
    assert payload["consumer_host_id"] == CONSUMER_HOST_ID
    assert payload["physical_host_separation"] is True
    assert payload["gate_evidence_restored"] is True
    assert (
        consumer_repository / ".cargo-gate" / "CARGO_GATE_ATTESTATION.json"
    ).is_file()
    assert (
        consumer_repository / ".release-gate" / "WINDOWS_SIGNED_BUILD_PASSED.json"
    ).is_file()
    assert (
        consumer_repository
        / "verification"
        / "release"
        / "cargo-gate"
        / "CARGO_GATE_ATTESTATION.sig"
    ).is_file()

    (root / "runtime" / "runtime.zip").write_bytes(b"tampered")
    second_consumer = tmp_path / "hardware-host-tamper"
    second_consumer.mkdir()
    rejected = verify_handoff(
        root, public_key, release_sha, request_id, second_consumer
    )
    assert rejected.returncode != 0
    rejection = rejected.stdout + rejected.stderr
    assert "size mismatch" in rejection or "sha256 mismatch" in rejection
    assert not (second_consumer / ".cargo-gate").exists()
    assert not (second_consumer / ".release-gate").exists()


def test_signed_handoff_rejects_same_physical_host(tmp_path: Path) -> None:
    root = tmp_path / "handoff"
    root.mkdir()
    (root / "artifact.bin").write_bytes(b"payload")
    producer_repository = make_producer_repository(tmp_path)
    consumer_repository = make_consumer_repository(tmp_path)
    private_key, public_key = make_keys(tmp_path)
    release_sha = "d" * 40
    request_id = "12345678-1234-5678-9abc-def012345678"
    assert (
        build_handoff(
            root, private_key, release_sha, request_id, producer_repository
        ).returncode
        == 0
    )

    same_host = run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
        "--consumer-host-id",
        PRODUCER_HOST_ID,
        cwd=consumer_repository,
    )
    assert same_host.returncode != 0
    assert "physical trust-domain separation is required" in (
        same_host.stdout + same_host.stderr
    )
    assert not (consumer_repository / ".cargo-gate").exists()


@pytest.mark.skipif(sys.platform != "win32", reason="requires Windows MachineGuid")
def test_windows_automatic_host_fingerprint_rejects_same_installation(tmp_path: Path) -> None:
    root = tmp_path / "handoff"
    root.mkdir()
    (root / "artifact.bin").write_bytes(b"payload")
    producer_repository = make_producer_repository(tmp_path)
    consumer_repository = make_consumer_repository(tmp_path)
    private_key, public_key = make_keys(tmp_path)
    release_sha = "e" * 40
    request_id = "87654321-4321-6789-abcd-0123456789ab"

    built = run(
        "build",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--signing-key",
        str(private_key),
        cwd=producer_repository,
    )
    assert built.returncode == 0, built.stderr

    same_windows_installation = run(
        "verify",
        str(root),
        "--release-sha",
        release_sha,
        "--request-id",
        request_id,
        "--trusted-public-key",
        str(public_key),
        cwd=consumer_repository,
    )
    assert same_windows_installation.returncode != 0
    assert "physical trust-domain separation is required" in (
        same_windows_installation.stdout + same_windows_installation.stderr
    )


def test_signed_handoff_rejects_wrong_identity_and_unexpected_files(tmp_path: Path) -> None:
    root = tmp_path / "handoff"
    root.mkdir()
    (root / "artifact.bin").write_bytes(b"payload")
    producer_repository = make_producer_repository(tmp_path)
    private_key, public_key = make_keys(tmp_path)
    release_sha = "b" * 40
    request_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    assert (
        build_handoff(
            root, private_key, release_sha, request_id, producer_repository
        ).returncode
        == 0
    )

    wrong_consumer = tmp_path / "hardware-host-wrong-sha"
    wrong_consumer.mkdir()
    wrong_sha = verify_handoff(
        root, public_key, "c" * 40, request_id, wrong_consumer
    )
    assert wrong_sha.returncode != 0
    assert "release_sha mismatch" in (wrong_sha.stdout + wrong_sha.stderr)
    assert not (wrong_consumer / ".cargo-gate").exists()

    (root / "unexpected.bin").write_bytes(b"unexpected")
    unexpected_consumer = tmp_path / "hardware-host-unexpected"
    unexpected_consumer.mkdir()
    unexpected = verify_handoff(
        root, public_key, release_sha, request_id, unexpected_consumer
    )
    assert unexpected.returncode != 0
    assert "file set mismatch" in (unexpected.stdout + unexpected.stderr)
    assert not (unexpected_consumer / ".cargo-gate").exists()
