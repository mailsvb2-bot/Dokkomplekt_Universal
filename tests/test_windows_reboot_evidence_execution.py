from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from datetime import datetime, timedelta, timezone

import pytest

ROOT = Path(__file__).resolve().parents[1]
VERIFY = ROOT / "tests/windows/verify_reboot_evidence.ps1"
SOURCE_SHA = "ab" * 32


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def iso(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def create_fixture(root: Path) -> tuple[Path, Path, dict[str, Path]]:
    program_data = root / "ProgramData"
    state = program_data / "DokkomplektE2E"
    watch = root / "watch"
    release = root / "release"
    for directory in (state, watch, release):
        directory.mkdir(parents=True, exist_ok=True)

    files = {
        "app": root / "Dokkomplekt.exe",
        "powershell": root / "powershell.exe",
        "post_script": state / "verify-after-reboot-0123456789abcdef0123456789abcdef.ps1",
        "payload": state / "payload-0123456789abcdef0123456789abcdef.docx",
        "destination": watch / "payload-0123456789abcdef0123456789abcdef.docx",
        "output": watch / "generated-result.docx",
        "archived": watch / "_обработано" / "payload-0123456789abcdef0123456789abcdef.docx",
        "receipt": watch / "_обработано" / "payload-0123456789abcdef0123456789abcdef.docx.dokkomplekt-receipt.json",
    }
    payload = b"source-document-payload"
    files["app"].write_bytes(b"signed-installed-application")
    files["powershell"].write_bytes(b"pinned-powershell")
    files["post_script"].write_text("# pinned post reboot script\n", encoding="utf-8")
    files["payload"].write_bytes(payload)
    files["destination"].write_bytes(payload)
    files["archived"].parent.mkdir(parents=True, exist_ok=True)
    files["archived"].write_bytes(payload)
    files["receipt"].write_text(json.dumps({"schema": 1, "original_name": files["destination"].name, "archived_name": files["archived"].name, "sha256": hashlib.sha256(payload).hexdigest(), "archived_at_unix": 1}), encoding="utf-8")
    files["output"].write_bytes(b"generated-output-document")

    now = datetime.now(timezone.utc)
    boot_before = now - timedelta(minutes=10)
    boot_after = now - timedelta(minutes=6)
    case_started = now - timedelta(minutes=4)
    output_written = now - timedelta(minutes=3)
    evidence_created = now - timedelta(minutes=2)
    os.utime(files["output"], (output_written.timestamp(), output_written.timestamp()))
    os.utime(files["receipt"], (output_written.timestamp(), output_written.timestamp()))

    evidence_path = release / "WINDOWS_REBOOT_E2E_RAW.json"
    pending = {
        "schema": "dokkomplekt.windows-reboot-e2e.pending.v2",
        "nonce": "0123456789abcdef0123456789abcdef",
        "source_tree_sha256": SOURCE_SHA,
        "boot_id_before": iso(boot_before),
        "application_path": str(files["app"]),
        "application_sha256": sha256(files["app"]),
        "source_document_path": str(root / "original.docx"),
        "source_document_sha256": sha256(files["payload"]),
        "payload_path": str(files["payload"]),
        "payload_sha256": sha256(files["payload"]),
        "watch_folder": str(watch),
        "evidence_path": str(evidence_path),
        "post_script_path": str(files["post_script"]),
        "post_script_sha256": sha256(files["post_script"]),
        "powershell_path": str(files["powershell"]),
        "powershell_sha256": sha256(files["powershell"]),
        "scheduled_task": "DokkomplektE2EAfterReboot",
    }
    evidence = {
        "schema": "dokkomplekt.windows-reboot-e2e.v2",
        "nonce": pending["nonce"],
        "source_tree_sha256": SOURCE_SHA,
        "boot_id_before": pending["boot_id_before"],
        "boot_id_after": iso(boot_after),
        "application_path": str(files["app"]),
        "application_sha256": sha256(files["app"]),
        "powershell_path": str(files["powershell"]),
        "powershell_sha256": sha256(files["powershell"]),
        "watcher_started_after_reboot": True,
        "watcher_process_id": 4242,
        "watcher_executable_path": str(files["app"]),
        "watcher_executable_sha256": sha256(files["app"]),
        "payload_path": str(files["payload"]),
        "payload_sha256": sha256(files["payload"]),
        "destination_path": str(files["destination"]),
        "destination_sha256": sha256(files["destination"]),
        "archive_receipt_path": str(files["receipt"]),
        "archive_receipt_sha256": sha256(files["receipt"]),
        "archive_receipt_last_write_utc": iso(output_written),
        "archived_source_path": str(files["archived"]),
        "archived_source_sha256": sha256(files["archived"]),
        "case_started_at_utc": iso(case_started),
        "post_reboot_case_completed": True,
        "post_reboot_output_path": str(files["output"]),
        "post_reboot_output_sha256": sha256(files["output"]),
        "post_reboot_output_size_bytes": files["output"].stat().st_size,
        "post_reboot_output_last_write_utc": iso(output_written),
        "evidence_created_at_utc": iso(evidence_created),
    }
    (state / "pending-reboot.json").write_text(json.dumps(pending), encoding="utf-8")
    evidence_path.write_text(json.dumps(evidence), encoding="utf-8")
    return program_data, evidence_path, files


def run_verifier(tmp_path: Path, *, tamper_output: bool) -> subprocess.CompletedProcess[str]:
    pwsh = shutil.which("pwsh")
    if pwsh is None:
        pytest.skip("PowerShell is not installed in this development environment")
    program_data, evidence_path, files = create_fixture(tmp_path)
    if tamper_output:
        files["output"].write_bytes(b"tampered-after-evidence")
    output_path = tmp_path / "verified.json"
    env = os.environ.copy()
    env["ProgramData"] = str(program_data)
    return subprocess.run(
        [
            pwsh,
            "-NoProfile",
            "-NonInteractive",
            "-File",
            str(VERIFY),
            "-EvidencePath",
            str(evidence_path),
            "-ExpectedSourceSha256",
            SOURCE_SHA,
            "-OutputPath",
            str(output_path),
        ],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


def test_verifier_accepts_bound_files_and_cleans_pending_state(tmp_path: Path) -> None:
    result = run_verifier(tmp_path, tamper_output=False)
    assert result.returncode == 0, result.stdout + result.stderr
    verified = json.loads((tmp_path / "verified.json").read_text(encoding="utf-8-sig"))
    assert verified["schema"] == "dokkomplekt.windows-reboot-e2e.verified.v2"
    assert verified["source_sha256"] == SOURCE_SHA
    assert verified["watcher_started_after_reboot"] is True
    assert verified["post_reboot_case_completed"] is True
    state = tmp_path / "ProgramData" / "DokkomplektE2E"
    assert not (state / "pending-reboot.json").exists()
    assert not any(state.glob("verify-after-reboot-*.ps1"))
    assert not any(state.glob("payload-*.docx"))


def test_verifier_rejects_output_changed_after_evidence(tmp_path: Path) -> None:
    result = run_verifier(tmp_path, tamper_output=True)
    assert result.returncode != 0
    assert "Post-reboot output SHA-256 mismatch" in result.stdout + result.stderr
    assert (tmp_path / "ProgramData" / "DokkomplektE2E" / "pending-reboot.json").exists()
