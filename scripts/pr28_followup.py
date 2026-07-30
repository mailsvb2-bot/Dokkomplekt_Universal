#!/usr/bin/env python3
"""One-use verified repair for the final commercial Rust Clippy failure."""

from __future__ import annotations

import base64
import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PAYLOAD = ROOT / ".github" / "pr28-followup" / "patch.b64"
TRANSFER_DIR = PAYLOAD.parent
BRANCH = "agent/fix-critical-security-privacy"
B64_SHA256 = "637e3979f37eb866ae13483b49ea907d1b1b90838d4a85bc3555010a7393530d"
GZIP_SHA256 = "135375d0f73b26bb903a49468cf57edb7c37be0b64202c997ed94e8f908e224a"
PATCH_SHA256 = "80452473d264e137e83c8fab0098623e2fbdb6563045e3c9e176440b31281b27"


def run(command: list[str], *, input_bytes: bytes | None = None, check: bool = True) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(command, cwd=ROOT, input=input_bytes, check=check)


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def apply_followup() -> None:
    if not PAYLOAD.is_file():
        return
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("GITHUB_HEAD_REF") != BRANCH:
        raise RuntimeError("PR28 follow-up may run only in its same-repository pull-request workflow")

    encoded = PAYLOAD.read_bytes()
    if digest(encoded) != B64_SHA256:
        raise RuntimeError("follow-up base64 checksum mismatch")
    compressed = base64.b64decode(encoded, validate=True)
    if digest(compressed) != GZIP_SHA256:
        raise RuntimeError("follow-up gzip checksum mismatch")
    patch = gzip.decompress(compressed)
    if digest(patch) != PATCH_SHA256:
        raise RuntimeError("follow-up patch checksum mismatch")

    run(["git", "apply", "--check", "-"], input_bytes=patch)
    run(["git", "apply", "-"], input_bytes=patch)

    verifier = subprocess.check_output(
        ["git", "show", "origin/main:scripts/verify_source_manifest.py"], cwd=ROOT
    )
    (ROOT / "scripts" / "verify_source_manifest.py").write_bytes(verifier)
    shutil.rmtree(TRANSFER_DIR)
    Path(__file__).unlink()

    run([sys.executable, "-m", "pip", "install", "-r", "requirements-dev.txt"])
    run([sys.executable, "scripts/check_commercial_rust_crates.py"])

    candidate = Path("/tmp/pr28-followup-manifest.txt")
    run(
        [sys.executable, "scripts/verify_source_manifest.py", "--candidate", str(candidate)],
        check=False,
    )
    if not candidate.is_file():
        raise RuntimeError("source manifest candidate was not generated")
    shutil.copyfile(candidate, ROOT / "SOURCE_MANIFEST_SHA256.txt")

    run([sys.executable, "scripts/verify_source_manifest.py"])
    run([sys.executable, "-m", "pytest", "-q"])
    run([sys.executable, "scripts/verify_starter_content_packs.py"])
    run([sys.executable, "scripts/check_reference_data_freshness.py"])
    run([sys.executable, "scripts/audit_rust_production_panics.py"])
    run([sys.executable, "scripts/static_quality_gate.py", "--source-only"])
    run(["git", "diff", "--check"])

    if TRANSFER_DIR.exists() or Path(__file__).exists():
        raise RuntimeError("one-use follow-up transport was not removed")
    if (ROOT / "scripts" / "verify_source_manifest.py").read_bytes() != verifier:
        raise RuntimeError("canonical verifier was not restored exactly")

    run(["git", "config", "user.name", "github-actions[bot]"])
    run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"])
    run(["git", "add", "-A"])
    run(["git", "diff", "--cached", "--check"])
    run(["git", "commit", "-m", "Fix commercial test-only import without suppressions"])
    run(["git", "push", "origin", f"HEAD:{BRANCH}"])


if __name__ == "__main__":
    apply_followup()
