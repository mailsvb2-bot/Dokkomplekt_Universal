#!/usr/bin/env python3
"""Fail closed when annual reference data is about to become unusable.

Before October the next year's calendar may legitimately still be provisional. From
October 1 onward release builds must contain a complete next-year calendar. The
current year is always mandatory. This turns an easy-to-forget manual update into a
release contract without inventing government transfer days before publication.
"""
from __future__ import annotations

import argparse
import datetime as dt
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CALENDAR = ROOT / "resources" / "production_calendar_ru.tsv"


def statuses() -> dict[int, str]:
    result: dict[int, str] = {}
    for raw in CALENDAR.read_text("utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        left, kind = (part.strip() for part in line.split("\t", 1))
        if left.isdigit():
            result[int(left)] = kind
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--date", help="ISO date for deterministic CI tests")
    args = parser.parse_args()
    today = dt.date.fromisoformat(args.date) if args.date else dt.date.today()
    years = statuses()
    current = years.get(today.year)
    if current != "complete":
        raise SystemExit(
            f"REFERENCE DATA BLOCKED: production calendar {today.year} is {current or 'missing'}"
        )
    next_status = years.get(today.year + 1)
    if today.month >= 10 and next_status != "complete":
        raise SystemExit(
            f"REFERENCE DATA BLOCKED: production calendar {today.year + 1} must be complete from October 1"
        )
    print(
        f"REFERENCE DATA READY: {today.year}=complete; "
        f"{today.year + 1}={next_status or 'missing'}; next-year deadline=October 1"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
