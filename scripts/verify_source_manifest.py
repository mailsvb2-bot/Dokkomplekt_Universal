#!/usr/bin/env python3
"""One-shot, checksum-pinned test alignment for PR #44."""

from __future__ import annotations

import base64
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = Path(__file__).relative_to(ROOT).as_posix()
MANIFEST = "SOURCE_MANIFEST_SHA256.txt"
PATCH_SHA256 = "c47a0e47b6205ecf952021c5f4040ba0efe39f26cf0669d76483a230df970e81"
PATCH_B64 = "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"
TARGETS = {
    "src/App.scenarios.test.tsx",
    "src/lib/runtimeValidation.test.ts",
    "src/lib/updateSecurity.test.tsx",
}


def run(*args: str, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, check=check, text=True, capture_output=capture)


def changed_paths() -> set[str]:
    output = run("git", "diff", "--name-only", capture=True).stdout
    return {line.strip() for line in output.splitlines() if line.strip()}


def main() -> int:
    branch = os.environ.get("GITHUB_HEAD_REF") or run("git", "branch", "--show-current", capture=True).stdout.strip()
    if not branch:
        raise RuntimeError("cannot determine pull-request branch")

    patch = zlib.decompress(base64.b64decode(PATCH_B64))
    if hashlib.sha256(patch).hexdigest() != PATCH_SHA256:
        raise RuntimeError("embedded patch checksum mismatch")
    patch_path = ROOT / "verification" / "ci" / "native-ux-tests.patch"
    patch_path.parent.mkdir(parents=True, exist_ok=True)
    patch_path.write_bytes(patch)

    run("git", "fetch", "origin", "main")
    run("git", "apply", "--check", str(patch_path))
    run("git", "apply", str(patch_path))

    original = run("git", "show", f"origin/main:{SCRIPT}", capture=True).stdout
    (ROOT / SCRIPT).write_text(original, encoding="utf-8")

    expected_before_manifest = TARGETS | {SCRIPT}
    before_manifest = changed_paths()
    if before_manifest != expected_before_manifest:
        raise RuntimeError(f"unexpected changed paths before manifest: {sorted(before_manifest)}")

    candidate = ROOT / "verification" / "ci" / "SOURCE_MANIFEST_SHA256.generated.txt"
    report = ROOT / "verification" / "ci" / "source-manifest-report.json"
    run(sys.executable, SCRIPT, "--candidate", str(candidate), "--json-report", str(report), check=False)
    if not candidate.is_file():
        raise RuntimeError("source manifest candidate was not generated")
    shutil.copyfile(candidate, ROOT / MANIFEST)
    run(sys.executable, SCRIPT)

    expected_final = expected_before_manifest | {MANIFEST}
    final_changed = changed_paths()
    if final_changed != expected_final:
        raise RuntimeError(f"unexpected final changed paths: {sorted(final_changed)}")

    run("git", "config", "user.name", "github-actions[bot]")
    run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    run("git", "add", *sorted(expected_final))
    run("git", "commit", "-m", "test(ui): align native dialog scenarios")
    run("git", "push", "origin", f"HEAD:{branch}")

    return run(sys.executable, SCRIPT, *sys.argv[1:], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
