#!/usr/bin/env python3
"""Fail when production Rust paths contain direct panic shortcuts.

Tests may use unwrap/expect for concise assertions. Production code must return a
typed error instead. The scanner deliberately ignores Rust integration-test
folders, *_tests.rs modules and #[cfg(test)] mod blocks, then reports exact lines.
Clippy/cargo test remain mandatory; this is an additional source-level guard.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATTERNS = (
    re.compile(r"\.unwrap\s*\(\s*\)"),
    re.compile(r"\.expect\s*\("),
    re.compile(r"\bpanic!\s*\("),
    re.compile(r"\btodo!\s*\("),
    re.compile(r"\bunimplemented!\s*\("),
    re.compile(r"\bunreachable!\s*\("),
)


def test_only_file(path: Path) -> bool:
    relative = path.relative_to(ROOT)
    return (
        "tests" in relative.parts
        or path.name.endswith("_tests.rs")
        or path.name in {"flow_tests.rs", "http_integration_tests.rs"}
    )


def production_lines(path: Path):
    lines = path.read_text("utf-8", errors="replace").splitlines()
    cfg_test_pending = False
    test_depth: int | None = None
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped.startswith("#[cfg(test)]"):
            cfg_test_pending = True
            continue
        if cfg_test_pending and re.search(r"\bmod\s+tests?\b", stripped):
            test_depth = line.count("{") - line.count("}")
            cfg_test_pending = False
            continue
        if test_depth is not None:
            test_depth += line.count("{") - line.count("}")
            if test_depth <= 0:
                test_depth = None
            continue
        if stripped and not stripped.startswith("#"):
            cfg_test_pending = False
        yield number, line


def violations(root: Path = ROOT) -> list[str]:
    found = []
    for path in sorted(root.rglob("*.rs")):
        if (
            any(part in {".git", "vendor"} or part == "target" or part.startswith("target-") for part in path.parts)
            or test_only_file(path)
        ):
            continue
        for number, line in production_lines(path):
            if any(pattern.search(line) for pattern in PATTERNS):
                found.append(f"{path.relative_to(root)}:{number}: {line.strip()}")
    return found


def main() -> int:
    found = violations()
    if found:
        print("PRODUCTION RUST PANIC SHORTCUTS FOUND:", file=sys.stderr)
        print("\n".join(found), file=sys.stderr)
        return 1
    print("PRODUCTION RUST PANIC AUDIT PASSED: no unwrap()/expect()/panic shortcuts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
