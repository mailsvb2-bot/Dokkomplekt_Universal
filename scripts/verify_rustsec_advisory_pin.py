#!/usr/bin/env python3
"""Validate the temporary RustSec advisory database pin.

The security gate may pin a known-good upstream advisory-db revision when
upstream `main` is temporarily structurally invalid. The pin is intentionally
short-lived: once it exceeds max_age_hours this verifier fails closed, forcing a
human-reviewed advance to a newer valid RustSec database revision.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import sys
from pathlib import Path

SCHEMA = "dokkomplekt.rustsec-advisory-db-pin.v1"
SHA40 = re.compile(r"^[0-9a-f]{40}$")
HTTPS_GITHUB = "https://github.com/RustSec/advisory-db.git"


def parse_utc(value: str) -> dt.datetime:
    raw = value.strip()
    if raw.endswith("Z"):
        raw = raw[:-1] + "+00:00"
    parsed = dt.datetime.fromisoformat(raw)
    if parsed.tzinfo is None:
        raise ValueError("committed_at_utc must include timezone information")
    return parsed.astimezone(dt.timezone.utc)


def validate_policy(path: Path, now: dt.datetime | None = None) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != SCHEMA:
        raise ValueError(f"unexpected RustSec pin schema: {data.get('schema')!r}")
    if data.get("repository") != HTTPS_GITHUB:
        raise ValueError("RustSec pin repository must be the canonical upstream advisory-db")
    commit = str(data.get("commit", "")).strip()
    blocked = str(data.get("blocked_upstream_commit", "")).strip()
    if not SHA40.fullmatch(commit):
        raise ValueError("RustSec pin commit must be a lowercase 40-character SHA")
    if not SHA40.fullmatch(blocked):
        raise ValueError("blocked_upstream_commit must be a lowercase 40-character SHA")
    if commit == blocked:
        raise ValueError("known-bad upstream commit cannot be used as the RustSec pin")
    max_age_hours = data.get("max_age_hours")
    if not isinstance(max_age_hours, int) or not (1 <= max_age_hours <= 168):
        raise ValueError("max_age_hours must be an integer in 1..168")
    reason = str(data.get("reason", "")).strip()
    if len(reason) < 40:
        raise ValueError("RustSec pin reason must document the upstream breakage")
    committed_at = parse_utc(str(data.get("committed_at_utc", "")))
    current = (now or dt.datetime.now(dt.timezone.utc)).astimezone(dt.timezone.utc)
    age = current - committed_at
    if age.total_seconds() < -300:
        raise ValueError("RustSec pin commit timestamp is unexpectedly in the future")
    if age > dt.timedelta(hours=max_age_hours):
        raise ValueError(
            "RustSec advisory DB pin expired; advance it to a newly validated upstream revision"
        )
    return {
        "schema": SCHEMA,
        "repository": HTTPS_GITHUB,
        "commit": commit,
        "blocked_upstream_commit": blocked,
        "committed_at_utc": committed_at.isoformat(),
        "max_age_hours": max_age_hours,
        "age_hours": round(max(age.total_seconds(), 0) / 3600, 3),
        "reason": reason,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "policy",
        nargs="?",
        type=Path,
        default=Path("verification/security/rustsec-advisory-db.json"),
    )
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    report = validate_policy(args.policy.resolve())
    print(json.dumps(report, ensure_ascii=False))
    if args.github_output:
        with args.github_output.open("a", encoding="utf-8") as stream:
            stream.write(f"repository={report['repository']}\n")
            stream.write(f"commit={report['commit']}\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
