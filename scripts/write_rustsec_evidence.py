#!/usr/bin/env python3
"""Bind a successful cargo-audit report to Cargo.lock and the exact audited RustSec pin."""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GATE = ROOT / ".cargo-gate"
RAW_REPORT = GATE / "RUSTSEC_AUDIT.json"
PIN_REPORT = GATE / "RUSTSEC_DB_PIN.json"
EVIDENCE = GATE / "RUSTSEC_EVIDENCE.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True, stderr=subprocess.STDOUT).strip()


def load_audited_pin(path: Path = PIN_REPORT) -> dict[str, object]:
    if not path.is_file() or not path.read_bytes().strip():
        raise RuntimeError("RustSec audited pin report is missing or empty")
    payload = json.loads(path.read_text("utf-8"))
    if not isinstance(payload, dict):
        raise RuntimeError("RustSec audited pin report must be an object")
    repository = str(payload.get("repository", "")).strip()
    commit = str(payload.get("commit", "")).strip().lower()
    if not repository.startswith("https://"):
        raise RuntimeError("RustSec audited pin repository must be an HTTPS URL")
    if len(commit) != 40 or any(char not in "0123456789abcdef" for char in commit):
        raise RuntimeError("RustSec audited pin commit must be a full lowercase Git SHA")
    return payload


def main() -> int:
    if not RAW_REPORT.is_file() or not RAW_REPORT.read_bytes().strip():
        raise RuntimeError("cargo audit JSON report is missing or empty")
    report = json.loads(RAW_REPORT.read_text("utf-8"))
    if not isinstance(report, dict):
        raise RuntimeError("cargo audit JSON report must be an object")
    pin = load_audited_pin()
    source = command(sys.executable, "scripts/source_fingerprint.py")
    evidence = {
        "schema": "dokkomplekt.rustsec-evidence.v2",
        "result": "passed",
        "timestamp_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_sha256": source,
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "audit_command": "cargo audit --db <exact-pinned-checkout> --no-fetch --deny warnings --json",
        "cargo_audit_version": command("cargo", "audit", "--version"),
        "advisory_database_commit": str(pin["commit"]).lower(),
        "advisory_database_origin": str(pin["repository"]),
        "advisory_database_pin_report_sha256": sha256(PIN_REPORT),
        "audit_report_sha256": sha256(RAW_REPORT),
        "audit_report_size_bytes": RAW_REPORT.stat().st_size,
        "audit_report_top_level_keys": sorted(str(key) for key in report),
    }
    EVIDENCE.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(
        "RUSTSEC EVIDENCE WRITTEN: "
        f"db={evidence['advisory_database_commit'][:12]} report={evidence['audit_report_sha256'][:12]}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
