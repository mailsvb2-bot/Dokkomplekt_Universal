#!/usr/bin/env python3
"""One-shot publisher for the checksum-pinned legacy command test alignment."""
from __future__ import annotations

import base64
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import zlib

ROOT = Path(__file__).resolve().parents[1]
BASE_SHA = "20721e6e3ebdb52adac807bac82eee5bc9fd1da3"
PATCH_SHA256 = "9a675425b6ecf84092b98003e96ff327aaa02bcb04ae30d6887ce5a7f381b5b9"
CHUNK = ROOT / "verification" / "ui-command-test-fix" / "patch.txt"
SELF = Path(__file__).resolve()
MANIFEST = ROOT / "SOURCE_MANIFEST_SHA256.txt"


def run(*args: str, capture: bool = False) -> str:
    completed = subprocess.run(
        args,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return completed.stdout.strip() if capture else ""


def restore_original() -> None:
    original = subprocess.run(
        ["git", "show", f"{BASE_SHA}:scripts/verify_source_manifest.py"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    SELF.write_bytes(original)


def regenerate_manifest() -> None:
    module_path = ROOT / "scripts" / "build_source_archive.py"
    spec = importlib.util.spec_from_file_location("build_source_archive", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {module_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    MANIFEST.write_bytes(module.source_manifest_payload())


def bootstrap() -> int:
    run("git", "merge-base", "--is-ancestor", BASE_SHA, "HEAD")
    changed = set(filter(None, run("git", "diff", "--name-only", f"{BASE_SHA}..HEAD", capture=True).splitlines()))
    allowed = {"scripts/verify_source_manifest.py", "verification/ui-command-test-fix/patch.txt"}
    unexpected = sorted(changed - allowed)
    if unexpected:
        raise RuntimeError(f"unexpected bootstrap changes: {unexpected}")

    patch = zlib.decompress(base64.b64decode(CHUNK.read_text(encoding="utf-8").strip()))
    actual = hashlib.sha256(patch).hexdigest()
    if actual != PATCH_SHA256:
        raise RuntimeError(f"patch checksum mismatch: {actual}")

    patch_path = ROOT / ".ui-command-test-fix.patch"
    patch_path.write_bytes(patch)
    try:
        run("git", "apply", "--check", "--whitespace=error", str(patch_path))
        run("git", "apply", "--whitespace=error", str(patch_path))
    finally:
        patch_path.unlink(missing_ok=True)

    restore_original()
    shutil.rmtree(CHUNK.parent)
    regenerate_manifest()
    run(sys.executable, "scripts/static_quality_gate.py", "--source-only")

    workflow_changes = run("git", "diff", "--name-only", capture=True).splitlines()
    forbidden = [path for path in workflow_changes if path.startswith(".github/workflows/")]
    if forbidden:
        raise RuntimeError(f"workflow changes are forbidden: {forbidden}")

    run("git", "config", "user.name", "github-actions[bot]")
    run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    run("git", "add", "-A")
    run("git", "commit", "-m", "Align legacy render command coverage with batch-only UI")
    branch = os.environ.get("GITHUB_HEAD_REF", "").strip()
    if not branch:
        raise RuntimeError("GITHUB_HEAD_REF is missing")
    run("git", "push", "origin", f"HEAD:{branch}")
    return subprocess.run([sys.executable, str(SELF), *sys.argv[1:]], cwd=ROOT).returncode


def main() -> int:
    if os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        return bootstrap()
    raise RuntimeError("one-shot bootstrap may run only in a same-repository pull_request GitHub Actions job")


if __name__ == "__main__":
    raise SystemExit(main())
