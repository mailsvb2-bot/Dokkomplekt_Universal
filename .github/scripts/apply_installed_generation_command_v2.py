from __future__ import annotations

import base64
import gzip
import hashlib
import os
from pathlib import Path
import subprocess
import sys

EXPECTED_PARENT = "ecfe2eaed9f504b1350d8fe0b6e3e26ab485bc8d"
TRIGGER = Path(".github/ci-sync/installed-generation.trigger")
BRANCH_WORKFLOW = Path(".github/workflows/apply-installed-generation-command.yml")
PATCH_SHA256 = "e3b0192761217337693f0e60b5498477315ae9f9a603e350cca99f6c97a80be0"
GZIP_SHA256 = "0d0ed29c468cccf9de542c00f8c21e4302678150a50169042083c3e33d036076"
EXPECTED_BLOBS = {
    "src-tauri/src/main.rs": "c4ec4bf744a419049ca464e6853f05f97d73c7ea",
    "src-tauri/src/subsystems/hardware_e2e.rs": "a3e03db4703c9746e2596813ac65c8570d792e58",
    "tests/test_v18_4_6_installed_app_generation_hardware.py": "0212a93920676fadc608603bb2f44d48cb02c0f2",
    "SOURCE_MANIFEST_SHA256.txt": "ebf839f335764b2b2ae26e090e4aa196487d2653",
}
PATCH_GZIP_B64 = """
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
"""


def run(*args: str, capture: bool = False) -> str:
    result = subprocess.run(args, check=True, text=True, capture_output=capture)
    return result.stdout.strip() if capture else ""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def read_blob(sha: str) -> bytes:
    return subprocess.run(["git", "cat-file", "blob", sha], check=True, capture_output=True).stdout


def main() -> None:
    require(run("git", "rev-parse", "HEAD^", capture=True) == EXPECTED_PARENT, "unexpected parent")
    changed = run("git", "diff", "--name-only", "HEAD^", "HEAD", capture=True).splitlines()
    require(changed == [str(TRIGGER)], f"unexpected trigger paths: {changed}")
    require(TRIGGER.is_file() and BRANCH_WORKFLOW.is_file(), "trusted transport files missing")

    compressed = base64.b64decode("".join(PATCH_GZIP_B64.split()), validate=True)
    require(hashlib.sha256(compressed).hexdigest() == GZIP_SHA256, "gzip SHA-256 mismatch")
    patch = gzip.decompress(compressed)
    require(hashlib.sha256(patch).hexdigest() == PATCH_SHA256, "patch SHA-256 mismatch")
    patch_path = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "installed-generation-v2.patch"
    patch_path.write_bytes(patch)
    run("git", "apply", "--check", "--whitespace=error-all", str(patch_path))
    run("git", "apply", "--whitespace=error-all", str(patch_path))

    for path, sha in (
        ("src-tauri/src/subsystems/hardware_e2e.rs", EXPECTED_BLOBS["src-tauri/src/subsystems/hardware_e2e.rs"]),
        ("tests/test_v18_4_6_installed_app_generation_hardware.py", EXPECTED_BLOBS["tests/test_v18_4_6_installed_app_generation_hardware.py"]),
    ):
        target = Path(path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(read_blob(sha))

    TRIGGER.unlink()
    BRANCH_WORKFLOW.unlink()
    candidate = Path(os.environ.get("RUNNER_TEMP", "/tmp")) / "SOURCE_MANIFEST_SHA256.txt"
    subprocess.run([sys.executable, "scripts/verify_source_manifest.py", "--candidate", str(candidate)], check=False)
    require(candidate.is_file() and candidate.stat().st_size > 0, "manifest candidate missing")
    Path("SOURCE_MANIFEST_SHA256.txt").write_bytes(candidate.read_bytes())

    actual = {path: run("git", "hash-object", path, capture=True) for path in EXPECTED_BLOBS}
    require(actual == EXPECTED_BLOBS, f"product blob mismatch: {actual}")
    run(sys.executable, "-m", "pytest", "-q", "tests")
    run(sys.executable, "scripts/static_quality_gate.py", "--source-only")
    run(sys.executable, "scripts/verify_source_manifest.py")
    run("git", "diff", "--check")


if __name__ == "__main__":
    main()
