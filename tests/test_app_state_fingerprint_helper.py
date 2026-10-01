from __future__ import annotations

import hashlib
import sqlite3
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts" / "read_app_state_fingerprint.py"


def test_helper_hashes_exact_app_state_payload_without_mutation(tmp_path: Path) -> None:
    database = tmp_path / "state.sqlite"
    connection = sqlite3.connect(database)
    connection.execute(
        "CREATE TABLE app_state(state_key TEXT PRIMARY KEY, json TEXT NOT NULL)"
    )
    connection.execute(
        "INSERT INTO app_state(state_key, json) VALUES (?, ?)",
        ("output_preferences_v2", "ciphertext-payload"),
    )
    connection.commit()
    connection.close()

    before = database.read_bytes()
    completed = subprocess.run(
        [
            sys.executable,
            str(HELPER),
            "--database",
            str(database),
            "--state-key",
            "output_preferences_v2",
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.strip() == hashlib.sha256(b"ciphertext-payload").hexdigest()
    assert database.read_bytes() == before


def test_helper_fails_closed_when_state_row_is_absent(tmp_path: Path) -> None:
    database = tmp_path / "state.sqlite"
    connection = sqlite3.connect(database)
    connection.execute(
        "CREATE TABLE app_state(state_key TEXT PRIMARY KEY, json TEXT NOT NULL)"
    )
    connection.commit()
    connection.close()

    completed = subprocess.run(
        [
            sys.executable,
            str(HELPER),
            "--database",
            str(database),
            "--state-key",
            "missing",
        ],
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 4
    assert completed.stdout == ""
