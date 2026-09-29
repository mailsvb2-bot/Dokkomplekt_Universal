from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
UPDATE_RUNTIME = ROOT / "src-tauri" / "src" / "subsystems" / "update_runtime.rs"
MAIN_RS = ROOT / "src-tauri" / "src" / "main.rs"
API_TS = ROOT / "src" / "lib" / "api.ts"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr19_update_lifecycle_is_single_fail_closed_path() -> None:
    runtime = UPDATE_RUNTIME.read_text(encoding="utf-8")
    main = MAIN_RS.read_text(encoding="utf-8")
    api = API_TS.read_text(encoding="utf-8")

    required_runtime_markers = [
        "fn ensure_no_active_case_runs",
        "fn verify_downloaded_package",
        '"verified-updates"',
        '"update-backups"',
        '"update-recovery.json"',
        '"recoverable_failure"',
        "quick_integrity_check",
        "handle.stop.store(true, Ordering::SeqCst)",
        "fn reconcile_pending_update",
        "fn apply_verified_update",
        "app.exit(0)",
    ]
    for marker in required_runtime_markers:
        assert marker in runtime, marker

    assert "reconcile_pending_update(&handle)" in main
    assert "get_update_recovery_status," in main
    assert "apply_verified_update," in main

    assert "callRust('get_update_recovery_status')" in api
    assert "callRust('apply_verified_update'" in api
    assert "'get_update_recovery_status'," in api
    assert "'apply_verified_update'," in api


def test_fpr19_register_stays_open_until_installed_upgrade_and_recovery_proof() -> None:
    payload = json.loads(REGISTER.read_text(encoding="utf-8"))
    entry = next(item for item in payload["features"] if item["id"] == "FPR-19")

    assert entry["status"] == "needs-runtime-proof"
    assert entry["runtime_evidence"] == []
    assert "previous-version -> current-version" in entry["runtime_gap"]
    assert "recoverable" in entry["runtime_gap"]
    assert "tests/test_fpr19_update_recovery_contract.py" in entry["evidence_paths"]
