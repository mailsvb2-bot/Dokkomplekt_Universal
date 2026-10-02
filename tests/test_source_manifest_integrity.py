from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERIFY_PATH = ROOT / "scripts" / "verify_source_manifest.py"
SPEC = importlib.util.spec_from_file_location("verify_source_manifest", VERIFY_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_checked_in_source_manifest_matches_current_tree() -> None:
    report = MODULE.verify()
    assert report["matches"], report


def test_mutable_release_evidence_is_not_part_of_source_manifest() -> None:
    relative = {
        path.relative_to(ROOT).as_posix()
        for path in MODULE.source_archive.source_files()
    }
    authored_verification = {
        "verification/autopilot/feature-matrix.json",
        "verification/e2e/LIVE_USER_SCENARIOS.json",
        "verification/security/rustsec-advisory-db.json",
    }
    included_verification = {path for path in relative if path.startswith("verification/")}
    assert included_verification == authored_verification
    assert not any(path.startswith("build-evidence/") for path in relative)
