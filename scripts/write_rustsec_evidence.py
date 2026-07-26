#!/usr/bin/env python3
"""Bind a successful cargo-audit JSON report to Cargo.lock and advisory DB HEAD."""
from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RAW_REPORT = ROOT / ".cargo-gate" / "RUSTSEC_AUDIT.json"
EVIDENCE = ROOT / ".cargo-gate" / "RUSTSEC_EVIDENCE.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(*args: str, cwd: Path = ROOT) -> str:
    return subprocess.check_output(args, cwd=cwd, text=True, stderr=subprocess.STDOUT).strip()


def advisory_db_candidates() -> list[Path]:
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")).expanduser()
    candidates: list[Path] = []
    configured = os.environ.get("RUSTSEC_ADVISORY_DB", "").strip()
    if configured:
        candidates.append(Path(configured).expanduser())
    candidates.append(cargo_home / "advisory-db")
    candidates.extend(sorted((cargo_home / "advisory-dbs").glob("*")))
    return candidates


def find_advisory_db() -> Path:
    for candidate in advisory_db_candidates():
        if not candidate.is_dir():
            continue
        try:
            command("git", "rev-parse", "--is-inside-work-tree", cwd=candidate)
            return candidate.resolve()
        except (OSError, subprocess.CalledProcessError):
            continue
    raise RuntimeError(
        "cargo-audit advisory database Git checkout was not found; release evidence cannot prove the audited database revision"
    )


def main() -> int:
    if not RAW_REPORT.is_file() or not RAW_REPORT.read_bytes().strip():
        raise RuntimeError("cargo audit JSON report is missing or empty")
    report = json.loads(RAW_REPORT.read_text("utf-8"))
    if not isinstance(report, dict):
        raise RuntimeError("cargo audit JSON report must be an object")
    database = find_advisory_db()
    head = command("git", "rev-parse", "HEAD", cwd=database)
    if len(head) != 40 or any(char not in "0123456789abcdef" for char in head.lower()):
        raise RuntimeError("advisory database HEAD is not a full Git commit")
    dirty = bool(command("git", "status", "--porcelain", cwd=database))
    if dirty:
        raise RuntimeError("advisory database checkout is dirty")
    try:
        origin = command("git", "remote", "get-url", "origin", cwd=database)
    except subprocess.CalledProcessError:
        origin = ""
    source = command(sys.executable, "scripts/source_fingerprint.py")
    evidence = {
        "schema": "dokkomplekt.rustsec-evidence.v1",
        "result": "passed",
        "timestamp_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_sha256": source,
        "cargo_lock_sha256": sha256(ROOT / "Cargo.lock"),
        "audit_command": "cargo audit --deny warnings --json",
        "cargo_audit_version": command("cargo", "audit", "--version"),
        "advisory_database_commit": head.lower(),
        "advisory_database_origin": origin,
        "advisory_database_dirty": False,
        "audit_report_sha256": sha256(RAW_REPORT),
        "audit_report_size_bytes": RAW_REPORT.stat().st_size,
        "audit_report_top_level_keys": sorted(str(key) for key in report),
    }
    EVIDENCE.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(
        f"RUSTSEC EVIDENCE WRITTEN: db={head[:12]} report={evidence['audit_report_sha256'][:12]}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
