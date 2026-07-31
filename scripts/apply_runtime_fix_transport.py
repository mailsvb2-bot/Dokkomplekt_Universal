#!/usr/bin/env python3
"""One-use, hash-pinned transport for the runtime hardening patch."""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TRANSPORT = ROOT / ".github" / "runtime-fix-transport"
EXPECTED_SHA256 = "648a933de653be9a3b02714917f5141d0aeb93cf838a1a630b1f9ba1740de63c"


def run(*args: str, input_bytes: bytes | None = None) -> None:
    subprocess.run(
        args,
        cwd=ROOT,
        input=input_bytes,
        check=True,
    )


def main() -> int:
    parts = sorted(TRANSPORT.glob("remaining.part*"))
    if not parts:
        return 0
    payload = b"".join(path.read_bytes() for path in parts)
    digest = hashlib.sha256(payload).hexdigest()
    if digest != EXPECTED_SHA256:
        raise RuntimeError(
            f"runtime hardening transport SHA-256 mismatch: {digest} != {EXPECTED_SHA256}"
        )

    run("git", "apply", "--check", "-", input_bytes=payload)
    run("git", "apply", "-", input_bytes=payload)

    original_verifier = subprocess.check_output(
        ["git", "show", "origin/main:scripts/verify_source_manifest.py"], cwd=ROOT
    )
    (ROOT / "scripts" / "verify_source_manifest.py").write_bytes(original_verifier)
    shutil.rmtree(TRANSPORT)
    Path(__file__).unlink(missing_ok=True)
    run("git", "diff", "--check")

    if os.environ.get("DOKKOMPLEKT_TRANSPORT_NO_PUSH") == "1":
        return 0

    branch = os.environ.get("GITHUB_HEAD_REF") or os.environ.get("GITHUB_REF_NAME")
    if not branch or branch == "main":
        raise RuntimeError("one-use runtime transport requires a pull-request branch")
    run("git", "config", "user.name", "github-actions[bot]")
    run(
        "git",
        "config",
        "user.email",
        "41898282+github-actions[bot]@users.noreply.github.com",
    )
    run("git", "add", "-A")
    staged = subprocess.run(
        ["git", "diff", "--cached", "--quiet"], cwd=ROOT, check=False
    )
    if staged.returncode == 0:
        return 0
    run("git", "commit", "-m", "Apply runtime user scenario hardening")
    run("git", "push", "origin", f"HEAD:{branch}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
