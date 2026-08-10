from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "write_windows_hardware_evidence_index.ps1"
CANONICAL_REPOSITORY = "mailsvb2-bot/Dokkomplekt_Universal"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_bytes(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)


def test_windows_hardware_evidence_index_binds_artifacts_and_rejects_tampering() -> None:
    pwsh = shutil.which("pwsh")
    if pwsh is None:
        pytest.skip("PowerShell is not installed in this development environment")

    target = ROOT / "target"
    target.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="hardware-evidence-index-", dir=target) as temporary:
        root = Path(temporary)
        installer_root = root / "installer"
        runtime_root = root / "runtime"
        verification_root = root / "verification"
        cargo_root = root / "cargo-gate"
        release_root = root / "release-gate"

        application = root / "dokkomplekt-tauri.exe"
        installer = installer_root / "Dokkomplekt-setup.exe"
        runtime = runtime_root / "Dokkomplekt-offline-runtime-windows-x86_64.zip"
        runtime_payload = Path(f"{runtime}.signing.json")
        runtime_signature = Path(f"{runtime_payload}.sig")
        runtime_public_key = Path(f"{runtime_payload}.public.pem")
        trusted_public_key = verification_root / "runtime-trusted-public.pem"
        cargo_attestation = cargo_root / "CARGO_GATE_ATTESTATION.json"
        cargo_signature = cargo_root / "CARGO_GATE_ATTESTATION.sig"

        write_bytes(application, b"signed-application")
        write_bytes(installer, b"signed-installer")
        write_bytes(runtime, b"signed-runtime-zip")
        write_json(runtime_payload, {"schema": "fixture.runtime-signing.v1"})
        write_bytes(runtime_signature, b"runtime-signature")
        write_bytes(runtime_public_key, b"artifact-public-key")
        write_bytes(trusted_public_key, b"trusted-public-key")
        write_json(cargo_attestation, {"schema": "fixture.cargo-gate.v1"})
        write_bytes(cargo_signature, b"cargo-gate-signature")

        for name in (
            "production-build-preflight.json",
            "windows-runtime-preflight.json",
            "hardware-preflight.json",
            "sidecar-status.json",
            "SIDECAR_AUTHENTICODE.json",
            "offline-runtime-probe.log",
            "scanned-pdf-ocr.json",
        ):
            write_bytes(verification_root / name, f"fixture:{name}".encode())

        source_sha = subprocess.check_output(
            [os.environ.get("PYTHON", "python"), "scripts/source_fingerprint.py"],
            cwd=ROOT,
            text=True,
        ).strip()
        release_sha = subprocess.check_output(
            ["git", "rev-parse", "--verify", "HEAD"], cwd=ROOT, text=True
        ).strip()
        gui_path = release_root / "GUI_AND_CONSOLE_EVIDENCE.json"
        print_path = release_root / "PRINT_EVENT_307.json"
        authenticode_path = release_root / "AUTHENTICODE_SIGNATURES.json"
        write_json(
            gui_path,
            {
                "schema": "dokkomplekt.gui-console-evidence.v1",
                "application_sha256": sha256(application),
                "launches": [
                    {"visible_window_title": "Dokkomplekt", "unexpected_visible_console_windows": []},
                    {"visible_window_title": "Dokkomplekt", "unexpected_visible_console_windows": []},
                ],
            },
        )
        write_json(print_path, {"schema": "fixture.print-event.v1"})
        write_json(
            authenticode_path,
            {
                "schema": "dokkomplekt.authenticode-evidence.v1",
                "installed_application": {"sha256": sha256(application)},
            },
        )
        signed_build_path = release_root / "WINDOWS_SIGNED_BUILD_PASSED.json"
        signed_build = {
            "schema": "dokkomplekt.windows-signed-build.v1",
            "source_repository": CANONICAL_REPOSITORY,
            "release_sha": release_sha,
            "source_sha256": source_sha,
            "rust_gate_attestation_sha256": sha256(cargo_attestation),
            "rust_gate_signature_sha256": sha256(cargo_signature),
            "application": {"sha256": sha256(application)},
            "installers": [{"name": installer.name, "sha256": sha256(installer)}],
            "offline_runtime": {
                "sha256": sha256(runtime),
                "signature_sha256": sha256(runtime_signature),
                "public_key_sha256": sha256(runtime_public_key),
                "trusted_public_key_sha256": sha256(trusted_public_key),
            },
        }
        write_json(signed_build_path, signed_build)
        reboot_path = release_root / "WINDOWS_REBOOT_E2E_PASSED.json"
        write_json(
            reboot_path,
            {
                "schema": "dokkomplekt.windows-reboot-e2e.verified.v2",
                "source_sha256": source_sha,
                "application_sha256": sha256(application),
                "watcher_executable_sha256": sha256(application),
            },
        )
        hardware_path = release_root / "WINDOWS_HARDWARE_E2E_PASSED.json"
        write_json(
            hardware_path,
            {
                "schema": "dokkomplekt.windows-hardware-e2e.v3",
                "source_sha256": source_sha,
                "installer_sha256": sha256(installer),
                "word_available": True,
                "watcher_autostart_found": True,
                "application_restart_passed": True,
                "gui_window_observed": True,
                "unexpected_console_windows_observed": False,
                "operating_system_reboot_tested": True,
                "watcher_started_after_reboot": True,
                "post_reboot_case_completed": True,
                "print_spooler_completion_observed": True,
                "installed_application_signature_valid": True,
                "silent_uninstall_passed": True,
                "gui_console_evidence_sha256": sha256(gui_path),
                "print_event_evidence_sha256": sha256(print_path),
                "authenticode_evidence_sha256": sha256(authenticode_path),
            },
        )
        for name in (
            "WATCHER_INSTALL.json",
            "WATCHER_UNINSTALL.json",
        ):
            write_json(release_root / name, {"schema": f"fixture.{name}.v1"})

        output = release_root / "WINDOWS_HARDWARE_EVIDENCE_INDEX.json"
        env = os.environ.copy()
        # Deliberately simulate the private workflow's own repository SHA. The
        # public release identity must ignore it and come from this checkout.
        env["GITHUB_SHA"] = "a" * 40
        command = [
            pwsh,
            "-NoProfile",
            "-NonInteractive",
            "-File",
            str(SCRIPT),
            "-InstallerRoot",
            str(installer_root),
            "-RuntimeRoot",
            str(runtime_root),
            "-ApplicationPath",
            str(application),
            "-VerificationRoot",
            str(verification_root),
            "-CargoGateRoot",
            str(cargo_root),
            "-ReleaseGateRoot",
            str(release_root),
            "-OutputPath",
            str(output),
        ]
        result = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True, check=False)
        assert result.returncode == 0, result.stdout + result.stderr
        index = json.loads(output.read_text(encoding="utf-8-sig"))
        assert index["schema"] == "dokkomplekt.windows-hardware-evidence-index.v1"
        assert index["source_repository"] == CANONICAL_REPOSITORY
        assert index["release_sha"] == release_sha
        assert index["release_sha"] != env["GITHUB_SHA"]
        assert index["source_sha256"] == source_sha
        assert index["record_count"] == len(index["records"])
        assert index["record_count"] == 24
        paths = {record["path"] for record in index["records"]}
        assert len(paths) == index["record_count"]
        assert all("\\" not in path for path in paths)

        signed_build["release_sha"] = "b" * 40
        write_json(signed_build_path, signed_build)
        wrong_release = subprocess.run(
            command, cwd=ROOT, env=env, text=True, capture_output=True, check=False
        )
        assert wrong_release.returncode != 0
        assert "not bound to the checked-out release SHA" in wrong_release.stdout + wrong_release.stderr
        signed_build["release_sha"] = release_sha
        write_json(signed_build_path, signed_build)

        application.write_bytes(b"tampered-application")
        failed = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True, check=False)
        assert failed.returncode != 0
        assert "Signed application SHA-256 mismatch" in failed.stdout + failed.stderr


def test_repository_boundary_requires_separator() -> None:
    source = SCRIPT.read_text(encoding="utf-8")
    assert "$repoPrefix = $repoRoot.TrimEnd" in source
    assert "[IO.Path]::DirectorySeparatorChar" in source
    assert "$resolved.StartsWith($repoPrefix" in source
    assert "$resolved.StartsWith($repoRoot" not in source
    assert "Read-RequiredJson $rebootPath 'dokkomplekt.windows-reboot-e2e.verified.v2'" in source
    assert "Reboot evidence application" in source
    assert "Reboot watcher executable" in source
    assert "@{ Path = $rebootPath; Kind = 'hardware-evidence' }" in source
