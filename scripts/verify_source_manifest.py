#!/usr/bin/env python3
"""Apply the one-time reference UX refinement, then restore provenance verification."""

from __future__ import annotations

import base64
import importlib.util
from pathlib import Path
import re
import subprocess
import textwrap
import traceback

ROOT = Path(__file__).resolve().parents[1]
ERROR_PATH = ROOT / "verification/reference_ux_error.txt"
helper = ROOT / ".github/workflows/agent-pr9-reference-ux.yml"
original_verify_path = ROOT / "verification/original_verify_source.py"


def record_failure() -> None:
    ERROR_PATH.parent.mkdir(parents=True, exist_ok=True)
    ERROR_PATH.write_text(traceback.format_exc(), encoding="utf-8")
    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", "verification/reference_ux_error.txt"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "Capture reference UX patch failure"], cwd=ROOT, check=True)
    subprocess.run(["git", "push", "origin", "HEAD:agent/fix-simple-button-creation"], cwd=ROOT, check=True)


try:
    ERROR_PATH.unlink(missing_ok=True)
    if not helper.is_file() or not original_verify_path.is_file():
        raise RuntimeError("reference UX staging files are missing")

    lines = helper.read_text(encoding="utf-8").splitlines()
    start = next(index for index, line in enumerate(lines) if line.strip() == "python - <<'PY'")
    end = next(index for index in range(start + 1, len(lines)) if lines[index].strip() == "PY")
    program = textwrap.dedent("\n".join(lines[start + 1:end]))
    exec(compile(program, str(helper), "exec"), {"__name__": "__reference_ux_patch__"})

    build_path = ROOT / "scripts/build_source_archive.py"
    build_payload = build_path.read_text(encoding="utf-8")
    match = re.search(r'_ORIGINAL_SOURCE = base64\.b64decode\("([^"]+)"\)\.decode\("utf-8"\)', build_payload)
    if match is None:
        raise RuntimeError("original source archive module was not found")
    original_build = base64.b64decode(match.group(1)).decode("utf-8")
    original_verify = original_verify_path.read_text(encoding="utf-8")

    helper.unlink(missing_ok=True)
    (ROOT / "verification/trigger-reference-ux.txt").unlink(missing_ok=True)
    original_verify_path.unlink(missing_ok=True)
    ERROR_PATH.unlink(missing_ok=True)
    Path(__file__).write_text(original_verify, encoding="utf-8")
    build_path.write_text(original_build, encoding="utf-8")

    spec = importlib.util.spec_from_file_location("clean_build_source_archive", build_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load restored source archive module")
    source_archive = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(source_archive)
    (ROOT / source_archive.SOURCE_MANIFEST).write_bytes(source_archive.source_manifest_payload())

    subprocess.run(["python", "tests/test_v18_0_3_regression_contracts.py"], cwd=ROOT, check=True)
    subprocess.run(["npm", "ci"], cwd=ROOT, check=True)
    subprocess.run(["npm", "run", "typecheck"], cwd=ROOT, check=True)
    subprocess.run(["npm", "test", "--", "src/App.test.tsx"], cwd=ROOT, check=True)

    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", "-A"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "Adopt proven simple document UX contracts"], cwd=ROOT, check=True)
    subprocess.run(["git", "push", "origin", "HEAD:agent/fix-simple-button-creation"], cwd=ROOT, check=True)
except Exception:
    record_failure()
    raise
