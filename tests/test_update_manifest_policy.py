from __future__ import annotations

import argparse
from pathlib import Path
import sys

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from scripts import generate_update_manifest


@pytest.mark.parametrize(
    "url",
    (
        "https://downloads.example.com/releases/app.exe",
        "https://127.0.0.1/releases/app.exe",
        "https://10.0.0.9/releases/app.exe",
        "https://user:secret@downloads.dokkomplekt.ru/releases/app.exe",
        "https://downloads.dokkomplekt.ru/releases/app.exe#fragment",
        "http://downloads.dokkomplekt.ru/releases/app.exe",
    ),
)
def test_update_artifact_url_reuses_production_https_policy(tmp_path: Path, url: str) -> None:
    artifact = tmp_path / "app.exe"
    artifact.write_bytes(b"MZ-test-artifact")
    with pytest.raises(argparse.ArgumentTypeError):
        generate_update_manifest.parse_artifact(f"windows-x86_64={artifact}={url}")


def test_update_artifact_accepts_public_https_and_records_integrity(tmp_path: Path) -> None:
    artifact = tmp_path / "app.exe"
    payload = b"MZ-release-artifact"
    artifact.write_bytes(payload)
    parsed = generate_update_manifest.parse_artifact(
        f"windows-x86_64={artifact}=https://downloads.dokkomplekt.ru/releases/app.exe"
    )
    assert parsed["platform"] == "windows-x86_64"
    assert parsed["url"] == "https://downloads.dokkomplekt.ru/releases/app.exe"
    assert parsed["size_bytes"] == len(payload)
    assert len(parsed["sha256"]) == 64
