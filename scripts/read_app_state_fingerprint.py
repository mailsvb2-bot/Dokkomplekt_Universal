#!/usr/bin/env python3
"""Read one app_state row fingerprint without decrypting or mutating the database."""

from __future__ import annotations

import argparse
import hashlib
import sqlite3
import time
from pathlib import Path


def fingerprint(database: Path, state_key: str) -> str | None:
    try:
        connection = sqlite3.connect(f"file:{database}?mode=ro", uri=True, timeout=0.2)
        try:
            row = connection.execute(
                "SELECT json FROM app_state WHERE state_key=?", (state_key,)
            ).fetchone()
        finally:
            connection.close()
    except (OSError, sqlite3.Error):
        return None
    if row is None:
        return None
    return hashlib.sha256(str(row[0]).encode("utf-8")).hexdigest()


def wait_for_fingerprint(
    database: Path, state_key: str, wait_seconds: float
) -> str | None:
    deadline = time.monotonic() + max(0.0, wait_seconds)
    while True:
        value = fingerprint(database, state_key)
        if value is not None:
            return value
        if time.monotonic() >= deadline:
            return None
        time.sleep(0.15)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database", type=Path, required=True)
    parser.add_argument("--state-key", required=True)
    parser.add_argument("--wait-seconds", type=float, default=0.0)
    args = parser.parse_args()

    value = wait_for_fingerprint(args.database, args.state_key, args.wait_seconds)
    if value is None:
        return 4
    print(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
