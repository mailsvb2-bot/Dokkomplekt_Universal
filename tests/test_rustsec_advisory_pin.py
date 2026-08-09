from __future__ import annotations

import datetime as dt
import importlib.util
import json
import tempfile
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "verify_rustsec_advisory_pin.py"
POLICY = ROOT / "verification" / "security" / "rustsec-advisory-db.json"


def load_module():
    spec = importlib.util.spec_from_file_location("verify_rustsec_advisory_pin", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_checked_in_pin_is_current_and_known_bad_head_is_not_used() -> None:
    module = load_module()
    now = dt.datetime(2026, 8, 9, 14, 0, tzinfo=dt.timezone.utc)
    report = module.validate_policy(POLICY, now=now)
    assert report["commit"] == "309ad29d8fe448bf986019e05d47b9e0e29a2218"
    assert report["blocked_upstream_commit"] == "e11d6b330dd033a9ed7476de71029cfb8f2d1095"
    assert report["commit"] != report["blocked_upstream_commit"]
    assert report["max_age_hours"] <= 168


def test_pin_expires_fail_closed() -> None:
    module = load_module()
    data = json.loads(POLICY.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "pin.json"
        path.write_text(json.dumps(data), encoding="utf-8")
        committed = module.parse_utc(data["committed_at_utc"])
        now = committed + dt.timedelta(hours=data["max_age_hours"], seconds=1)
        with pytest.raises(ValueError, match="expired"):
            module.validate_policy(path, now=now)


def test_known_bad_commit_cannot_be_pinned() -> None:
    module = load_module()
    data = json.loads(POLICY.read_text(encoding="utf-8"))
    data["commit"] = data["blocked_upstream_commit"]
    with tempfile.TemporaryDirectory() as temporary:
        path = Path(temporary) / "pin.json"
        path.write_text(json.dumps(data), encoding="utf-8")
        with pytest.raises(ValueError, match="known-bad"):
            module.validate_policy(
                path,
                now=module.parse_utc(data["committed_at_utc"]),
            )
