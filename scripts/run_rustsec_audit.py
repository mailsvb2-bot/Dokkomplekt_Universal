#!/usr/bin/env python3
"""Run cargo-audit against an exact, short-lived RustSec advisory DB revision.

This is a resilience boundary, not an advisory suppression mechanism. The pin
policy expires fail-closed within at most seven days. The helper fetches exactly
the approved upstream commit, verifies the checkout SHA, disables cargo-audit's
own database fetch, and keeps `--deny warnings` intact.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from scripts.verify_rustsec_advisory_pin import validate_policy
except ModuleNotFoundError:
    from verify_rustsec_advisory_pin import validate_policy

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "verification" / "security" / "rustsec-advisory-db.json"


def checked(command: list[str], *, cwd: Path | None = None, capture: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
        check=False,
    )
    if completed.returncode != 0:
        detail = ""
        if capture:
            detail = (completed.stderr or completed.stdout or "")[-4000:]
        raise RuntimeError(
            f"command failed with exit code {completed.returncode}: {' '.join(command)}"
            + (f"\n{detail}" if detail else "")
        )
    return completed


def build_audit_command(db: Path, json_output: bool) -> list[str]:
    command = [
        "cargo",
        "audit",
        "--db",
        str(db),
        "--no-fetch",
        "--deny",
        "warnings",
    ]
    if json_output:
        command.append("--json")
    return command


def checkout_database(repository: str, commit: str, destination: Path) -> None:
    checked(["git", "init", "--quiet", str(destination)])
    checked(["git", "-C", str(destination), "remote", "add", "origin", repository])
    checked(
        [
            "git",
            "-C",
            str(destination),
            "fetch",
            "--quiet",
            "--depth=1",
            "origin",
            commit,
        ]
    )
    checked(["git", "-C", str(destination), "checkout", "--quiet", "--detach", "FETCH_HEAD"])
    actual = checked(
        ["git", "-C", str(destination), "rev-parse", "HEAD"], capture=True
    ).stdout.strip()
    if actual != commit:
        raise RuntimeError(
            f"RustSec advisory DB checkout mismatch: expected {commit}, got {actual}"
        )


def atomic_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    os.replace(temporary, path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    parser.add_argument("--json-output", type=Path)
    parser.add_argument("--pin-report", type=Path)
    args = parser.parse_args()

    if shutil.which("git") is None:
        raise RuntimeError("git is required for the pinned RustSec advisory database")
    if shutil.which("cargo") is None:
        raise RuntimeError("cargo is required for cargo-audit")

    policy = validate_policy(args.policy.resolve())
    if args.pin_report:
        atomic_json(args.pin_report.resolve(), policy)

    with tempfile.TemporaryDirectory(prefix="dokkomplekt-rustsec-db-") as temporary:
        db = Path(temporary).resolve() / "advisory-db"
        checkout_database(policy["repository"], policy["commit"], db)
        command = build_audit_command(db, args.json_output is not None)
        if args.json_output:
            completed = checked(command, cwd=ROOT, capture=True)
            output = args.json_output.resolve()
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(completed.stdout, encoding="utf-8")
            if completed.stderr:
                print(completed.stderr, file=sys.stderr, end="")
        else:
            checked(command, cwd=ROOT, capture=False)

    print(
        "RUSTSEC AUDIT PASSED AGAINST PINNED DB: "
        f"commit={policy['commit']}; age_hours={policy['age_hours']}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
