from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def load_register() -> dict[str, object]:
    return json.loads(REGISTER.read_text(encoding="utf-8"))


def test_feature_preservation_register_has_exact_unique_fpr01_to_fpr23() -> None:
    payload = load_register()
    features = payload["features"]
    ids = [entry["id"] for entry in features]
    assert ids == [f"FPR-{number:02d}" for number in range(1, 24)]
    assert len(ids) == len(set(ids))


def test_feature_status_and_runtime_gap_cannot_contradict_each_other() -> None:
    payload = load_register()
    for entry in payload["features"]:
        status = entry["status"]
        runtime_gap = entry.get("runtime_gap", "").strip()
        runtime_evidence = entry.get("runtime_evidence", [])

        assert status in {"verified", "needs-runtime-proof"}, entry["id"]
        if status == "verified":
            assert runtime_gap == "", entry["id"]
            assert runtime_evidence, entry["id"]
        else:
            assert runtime_gap != "", entry["id"]


def test_final_register_closure_requires_every_feature_verified() -> None:
    payload = load_register()
    all_verified = all(entry["status"] == "verified" for entry in payload["features"])
    assert payload["final_register_closed"] is all_verified
