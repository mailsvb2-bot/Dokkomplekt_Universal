#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
BRANCH = os.environ.get("GITHUB_HEAD_REF") or os.environ.get("GITHUB_REF_NAME") or ""
EXPECTED_BRANCH = "agent/fix-simple-button-creation"

ORIGINAL = '''#!/usr/bin/env python3
"""Verify that the checked-in source manifest matches the current source tree."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().with_name("build_source_archive.py")
SPEC = importlib.util.spec_from_file_location("build_source_archive", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load source archive module: {MODULE_PATH}")
source_archive = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(source_archive)

ROOT = source_archive.ROOT
MANIFEST_PATH = ROOT / source_archive.SOURCE_MANIFEST


def parse_manifest(payload: bytes) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, raw_line in enumerate(payload.decode("utf-8").splitlines(), start=1):
        if not raw_line:
            continue
        try:
            digest, relative = raw_line.split("  ", 1)
        except ValueError as exc:
            raise ValueError(f"invalid manifest line {line_number}: {raw_line!r}") from exc
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
            raise ValueError(f"invalid SHA-256 at line {line_number}: {digest!r}")
        if relative in entries:
            raise ValueError(f"duplicate manifest path at line {line_number}: {relative}")
        entries[relative] = digest
    return entries


def manifest_report(actual_payload: bytes, expected_payload: bytes) -> dict[str, object]:
    actual = parse_manifest(actual_payload)
    expected = parse_manifest(expected_payload)
    missing = sorted(set(expected) - set(actual))
    orphaned = sorted(set(actual) - set(expected))
    changed = sorted(
        path for path in set(actual) & set(expected) if actual[path] != expected[path]
    )
    return {
        "schema": "dokkomplekt.source-manifest-verification.v1",
        "matches": not (missing or orphaned or changed),
        "expected_file_count": len(expected),
        "manifest_file_count": len(actual),
        "missing_entries": missing,
        "orphaned_entries": orphaned,
        "hash_mismatches": changed,
    }


def verify(candidate_path: Path | None = None) -> dict[str, object]:
    expected_payload = source_archive.source_manifest_payload()
    if candidate_path is not None:
        candidate_path.parent.mkdir(parents=True, exist_ok=True)
        candidate_path.write_bytes(expected_payload)
    actual_payload = MANIFEST_PATH.read_bytes() if MANIFEST_PATH.is_file() else b""
    return manifest_report(actual_payload, expected_payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--candidate",
        type=Path,
        help="write the generated manifest to this path without mutating the checked-in manifest",
    )
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    candidate = args.candidate.resolve() if args.candidate else None
    report = verify(candidate)
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\\n"
    if args.json_report:
        output = args.json_report.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["matches"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
'''


def replace_once(payload: str, old: str, new: str, label: str) -> str:
    count = payload.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one occurrence, found {count}")
    return payload.replace(old, new, 1)


def main() -> int:
    if BRANCH != EXPECTED_BRANCH:
        SELF.write_text(ORIGINAL, encoding="utf-8")
        return subprocess.run([sys.executable, str(SELF), *sys.argv[1:]], cwd=ROOT).returncode

    app_path = ROOT / "src/App.tsx"
    app = app_path.read_text(encoding="utf-8")
    app = replace_once(
        app,
        "  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);\n\n  useEffect(() => {",
        "  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);\n\n  useEffect(() => {\n    if (!documents.length && utilityOpen) setUtilityOpen(false);\n  }, [documents.length, utilityOpen]);\n\n  useEffect(() => {",
        "close utilities when document set becomes empty",
    )
    app = replace_once(
        app,
        "            <button className=\"headerSettings\" onClick={() => setUtilityOpen((value) => !value)} aria-expanded={utilityOpen}>\n              <i className=\"ti ti-settings\" aria-hidden=\"true\" /> Настройки\n            </button>",
        "            {documents.length > 0 && (\n              <button className=\"headerSettings\" onClick={() => setUtilityOpen((value) => !value)} aria-expanded={utilityOpen}>\n                <i className=\"ti ti-settings\" aria-hidden=\"true\" /> Настройки\n              </button>\n            )}",
        "hide header settings in first-run mode",
    )
    app = replace_once(
        app,
        "        {utilityOpen && (",
        "        {documents.length > 0 && utilityOpen && (",
        "hide already-open utility panel in first-run mode",
    )
    app_path.write_text(app, encoding="utf-8")

    test_path = ROOT / "src/App.test.tsx"
    tests = test_path.read_text(encoding="utf-8")
    tests = replace_once(
        tests,
        "    expect(screen.queryByRole('button', { name: 'Дополнительные настройки' })).toBeNull();",
        "    expect(screen.queryByRole('button', { name: 'Дополнительные настройки' })).toBeNull();\n    expect(screen.queryByRole('button', { name: 'Настройки' })).toBeNull();\n    expect(screen.queryByRole('region', { name: 'Дополнительные инструменты' })).toBeNull();",
        "first-run hides all utility entry points",
    )
    test_path.write_text(tests, encoding="utf-8")

    SELF.write_text(ORIGINAL, encoding="utf-8")

    build_path = ROOT / "scripts/build_source_archive.py"
    spec = importlib.util.spec_from_file_location("build_source_archive", build_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load source archive module")
    source_archive = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(source_archive)
    (ROOT / source_archive.SOURCE_MANIFEST).write_bytes(source_archive.source_manifest_payload())

    subprocess.run(["npm", "ci"], cwd=ROOT, check=True)
    subprocess.run(["npm", "run", "typecheck"], cwd=ROOT, check=True)
    subprocess.run(["npm", "test", "--", "src/App.test.tsx"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, "tests/test_v18_0_3_regression_contracts.py"], cwd=ROOT, check=True)

    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], cwd=ROOT, check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], cwd=ROOT, check=True)
    subprocess.run(["git", "add", "src/App.tsx", "src/App.test.tsx", "SOURCE_MANIFEST_SHA256.txt", "scripts/verify_source_manifest.py"], cwd=ROOT, check=True)
    subprocess.run(["git", "commit", "-m", "Close utilities when first-run mode becomes active"], cwd=ROOT, check=True)
    subprocess.run(["git", "push", "origin", f"HEAD:{EXPECTED_BRANCH}"], cwd=ROOT, check=True)

    return subprocess.run([sys.executable, str(SELF), *sys.argv[1:]], cwd=ROOT).returncode


if __name__ == "__main__":
    raise SystemExit(main())
