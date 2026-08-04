#!/usr/bin/env python3
"""One-shot, fail-closed rustfmt repair for PR #44."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).relative_to(ROOT).as_posix()
MAIN_RS = "src-tauri/src/main.rs"
MANIFEST = "SOURCE_MANIFEST_SHA256.txt"
ALLOWED_BEFORE_MANIFEST = {SCRIPT, MAIN_RS}
ALLOWED_FINAL = {SCRIPT, MAIN_RS, MANIFEST}


def run(*args: str, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=ROOT,
        check=check,
        text=True,
        capture_output=capture,
    )


def changed_paths() -> set[str]:
    result = run("git", "diff", "--name-only", capture=True)
    return {line.strip() for line in result.stdout.splitlines() if line.strip()}


def main() -> int:
    branch = os.environ.get("GITHUB_HEAD_REF") or run(
        "git", "branch", "--show-current", capture=True
    ).stdout.strip()
    if not branch:
        raise RuntimeError("cannot determine pull-request branch")

    run("git", "fetch", "origin", "main")
    run("cargo", "fmt", "--all")

    original = run(
        "git", "show", f"origin/main:{SCRIPT}", capture=True
    ).stdout
    (ROOT / SCRIPT).write_text(original, encoding="utf-8")

    before_manifest = changed_paths()
    if before_manifest != ALLOWED_BEFORE_MANIFEST:
        raise RuntimeError(
            f"rustfmt changed unexpected paths: {sorted(before_manifest)}"
        )

    candidate = ROOT / "verification" / "ci" / "SOURCE_MANIFEST_SHA256.generated.txt"
    report = ROOT / "verification" / "ci" / "source-manifest-report.json"
    candidate.parent.mkdir(parents=True, exist_ok=True)
    run(
        sys.executable,
        SCRIPT,
        "--candidate",
        str(candidate),
        "--json-report",
        str(report),
        check=False,
    )
    if not candidate.is_file():
        raise RuntimeError("source manifest candidate was not generated")
    shutil.copyfile(candidate, ROOT / MANIFEST)
    run(sys.executable, SCRIPT)

    final_changed = changed_paths()
    if final_changed != ALLOWED_FINAL:
        raise RuntimeError(
            f"repair produced unexpected paths: {sorted(final_changed)}"
        )

    run("git", "config", "user.name", "github-actions[bot]")
    run(
        "git",
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    )
    run("git", "add", MAIN_RS, MANIFEST, SCRIPT)
    run("git", "commit", "-m", "fix(ui): format native folder picker tests")
    run("git", "push", "origin", f"HEAD:{branch}")

    return run(sys.executable, SCRIPT, *sys.argv[1:], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
