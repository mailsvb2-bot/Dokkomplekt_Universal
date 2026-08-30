#!/usr/bin/env python3
"""Compile and test Rust crates intentionally excluded from the desktop workspace.

The license HTTP server and Python binding have different deployment targets from
Tauri, so they remain outside the desktop workspace. This gate builds them in a
throw-away source workspace with its own build target, preserving the source tree
while still making compilation, tests, Clippy and dependency audit mandatory for a
release. Build outputs are isolated from the desktop target so a
workspace with an independently generated lockfile can never poison the packaged app.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from source_fingerprint import source_fingerprint

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / ".cargo-gate"
EVIDENCE = OUT_DIR / "COMMERCIAL_CRATES_EVIDENCE.json"
LOCK_EVIDENCE = OUT_DIR / "COMMERCIAL_CRATES_Cargo.lock"
AUDIT_EVIDENCE = OUT_DIR / "COMMERCIAL_CRATES_RUSTSEC_AUDIT.json"
CRATES = (
    "dokkomplekt-license-core",
    "dokkomplekt-license-server",
    "dokkomplekt-license-python",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(
    command: list[str],
    cwd: Path,
    *,
    target_dir: Path,
    stdout_path: Path | None = None,
) -> None:
    print("+", " ".join(command), flush=True)
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    if stdout_path is None:
        subprocess.run(command, cwd=cwd, env=env, check=True)
        return
    with stdout_path.open("wb") as stream:
        subprocess.run(command, cwd=cwd, env=env, stdout=stream, check=True)


def write_workspace_manifest(destination: Path) -> None:
    source = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    workspace_end = source.index("[workspace.package]")
    suffix = source[workspace_end:]
    members = "\n".join(f'  "crates/{name}",' for name in CRATES)
    prefix = f'''[workspace]\nresolver = "2"\nmembers = [\n{members}\n]\n\n'''
    (destination / "Cargo.toml").write_text(prefix + suffix, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-audit", action="store_true", help="Only for non-release local diagnosis.")
    args = parser.parse_args()

    for tool in ("cargo", "rustc"):
        if shutil.which(tool) is None:
            raise SystemExit(f"{tool} is required for commercial Rust crate verification")
    if not args.skip_audit and shutil.which("cargo-audit") is None:
        # cargo-audit is normally a Cargo subcommand, so also accept `cargo audit`.
        probe = subprocess.run(["cargo", "audit", "--version"], cwd=ROOT, capture_output=True)
        if probe.returncode != 0:
            raise SystemExit("cargo-audit is required for commercial Rust crate verification")

    source_before = source_fingerprint()
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="dokkomplekt-commercial-rust-") as raw_temp:
        temp = Path(raw_temp)
        commercial_target = temp / "target"
        commercial_target.mkdir()
        (temp / "crates").mkdir()
        write_workspace_manifest(temp)
        (temp / "rust-toolchain.toml").write_bytes((ROOT / "rust-toolchain.toml").read_bytes())
        for name in CRATES:
            shutil.copytree(ROOT / "crates" / name, temp / "crates" / name)

        commands = [
            ["cargo", "generate-lockfile"],
            ["cargo", "fmt", "--all", "--", "--check"],
            ["cargo", "check", "--workspace", "--all-targets", "--locked"],
            ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
            ["cargo", "test", "--workspace", "--locked"],
        ]
        for command in commands:
            run(command, temp, target_dir=commercial_target)
        lock = temp / "Cargo.lock"
        shutil.copy2(lock, LOCK_EVIDENCE)

        audit_command: list[str] | None = None
        if not args.skip_audit:
            audit_command = ["cargo", "audit", "--deny", "warnings", "--json", "--file", str(lock)]
            run(
                audit_command,
                temp,
                target_dir=commercial_target,
                stdout_path=AUDIT_EVIDENCE,
            )
        else:
            AUDIT_EVIDENCE.unlink(missing_ok=True)

        payload = {
            "schema": "dokkomplekt.commercial-rust-gate.v1",
            "result": "passed",
            "timestamp_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "source_sha256": source_before,
            "cargo": subprocess.check_output(["cargo", "--version"], text=True).strip(),
            "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
            "crates": list(CRATES),
            "generated_lock_sha256": sha256(LOCK_EVIDENCE),
            "audit_report_sha256": sha256(AUDIT_EVIDENCE) if AUDIT_EVIDENCE.exists() else None,
            "checks": [" ".join(command) for command in commands]
            + ([" ".join(audit_command)] if audit_command else []),
        }
        EVIDENCE.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    source_after = source_fingerprint()
    if source_before != source_after:
        raise SystemExit("commercial Rust gate mutated the source tree")
    print(EVIDENCE)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
