#!/usr/bin/env python3
"""Verify that the checked-in source manifest matches the current source tree."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

from pr28_followup import apply_followup

apply_followup()

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
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        output = args.json_report.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["matches"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
