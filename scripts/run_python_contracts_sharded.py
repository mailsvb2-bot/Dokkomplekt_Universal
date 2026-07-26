#!/usr/bin/env python3
"""Run each Python contract module in an isolated process.

Dokkomplekt's contract suite intentionally imports build/release scripts that mutate
module-level globals during tests. A single long-lived interpreter can therefore
leak state between historical contract generations. This runner makes isolation
explicit, enforces a per-module timeout, kills descendant processes on timeout,
and emits machine-readable evidence for release reports.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from source_fingerprint import source_fingerprint

ROOT = Path(__file__).resolve().parents[1]
PASSED_RE = re.compile(r"(?P<count>\d+) passed")
SKIPPED_RE = re.compile(r"(?P<count>\d+) skipped")


@dataclass
class ModuleResult:
    module: str
    result: str
    passed: int
    skipped: int
    duration_seconds: float
    returncode: int | None
    output_tail: str


def terminate_tree(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run_module(path: Path, timeout_seconds: int) -> ModuleResult:
    command = [sys.executable, "-m", "pytest", "-q", str(path.relative_to(ROOT))]
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        start_new_session=os.name != "nt",
        creationflags=creationflags,
    )
    try:
        output, _ = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        terminate_tree(process)
        output, _ = process.communicate()
        duration = time.monotonic() - started
        return ModuleResult(
            module=path.relative_to(ROOT).as_posix(),
            result="timeout",
            passed=0,
            skipped=0,
            duration_seconds=round(duration, 3),
            returncode=None,
            output_tail=output[-4000:],
        )
    duration = time.monotonic() - started
    passed_matches = PASSED_RE.findall(output)
    skipped_matches = SKIPPED_RE.findall(output)
    passed = sum(int(value) for value in passed_matches)
    skipped = sum(int(value) for value in skipped_matches)
    result = "passed" if process.returncode == 0 and passed > 0 else "failed"
    return ModuleResult(
        module=path.relative_to(ROOT).as_posix(),
        result=result,
        passed=passed,
        skipped=skipped,
        duration_seconds=round(duration, 3),
        returncode=process.returncode,
        output_tail=output[-4000:],
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pattern", default="test_*.py")
    parser.add_argument("--timeout-seconds", type=int, default=180)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if args.timeout_seconds < 5 or args.timeout_seconds > 3600:
        raise SystemExit("--timeout-seconds must be between 5 and 3600")

    modules = sorted((ROOT / "tests").glob(args.pattern))
    if not modules:
        raise SystemExit(f"no Python contract modules match {args.pattern!r}")

    source_before = source_fingerprint()
    results: list[ModuleResult] = []
    for path in modules:
        result = run_module(path, args.timeout_seconds)
        results.append(result)
        print(
            f"[{result.result.upper():7}] {result.module}: "
            f"passed={result.passed} skipped={result.skipped} "
            f"duration={result.duration_seconds:.3f}s",
            flush=True,
        )
        if result.result != "passed":
            print(result.output_tail, file=sys.stderr, flush=True)

    source_after = source_fingerprint()
    source_unchanged = source_before == source_after
    payload = {
        "schema": "dokkomplekt.python-contract-shards.v1",
        "result": "passed" if all(item.result == "passed" for item in results) and source_unchanged else "failed",
        "python": sys.version.split()[0],
        "source_sha256": source_after,
        "source_unchanged_during_run": source_unchanged,
        "module_count": len(results),
        "passed_test_count": sum(item.passed for item in results),
        "skipped_test_count": sum(item.skipped for item in results),
        "duration_seconds": round(sum(item.duration_seconds for item in results), 3),
        "modules": [asdict(item) for item in results],
    }
    if args.report:
        report = args.report if args.report.is_absolute() else ROOT / args.report
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(json.dumps({key: value for key, value in payload.items() if key != "modules"}, ensure_ascii=False))
    return 0 if payload["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
