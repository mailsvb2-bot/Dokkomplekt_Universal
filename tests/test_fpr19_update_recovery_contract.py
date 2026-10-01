from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
UPDATE_RUNTIME = ROOT / "src-tauri" / "src" / "subsystems" / "update_runtime.rs"
MAIN_RS = ROOT / "src-tauri" / "src" / "main.rs"
API_TS = ROOT / "src" / "lib" / "api.ts"
APP_TSX = ROOT / "src" / "App.tsx"
UPDATE_HOOK = ROOT / "src" / "hooks" / "useUpdateLifecycle.ts"
LIVE_UPDATE = ROOT / "tests" / "windows" / "windows_live_update_e2e.ps1"
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
        "fn restore_update_backup_files",
        "fn restore_update_backup",
        "\"rolled_back\"",
        "fn apply_verified_update",
        "app.exit(0)",
    ]
    for marker in required_runtime_markers:
        assert marker in runtime, marker

    apply_start = runtime.index("fn apply_verified_update(")
    apply = runtime[apply_start:]
    assert "fn record_recoverable_update_failure" in runtime
    assert "Не удалось заблокировать watcher перед запуском обновления" in apply
    assert "record_recoverable_update_failure" in apply
    assert apply.index(".spawn()") < apply.index("watcher.take()")
    assert "Installer запущен, но не удалось зафиксировать installer_started" in apply
    assert "app.exit(1)" in apply

    assert "reconcile_pending_update(&handle)" in main
    assert main.index("reconcile_pending_update(&handle)") < main.index("ensure_default_state_loaded(&handle, &state)")
    assert "get_update_recovery_status," in main
    assert "apply_verified_update," in main

    assert "callRust('get_update_recovery_status')" in api
    assert "callRust('apply_verified_update'" in api
    assert "'get_update_recovery_status'," in api
    assert "'apply_verified_update'," in api


def test_fpr19_user_update_ui_closes_check_apply_recovery_loop() -> None:
    app = APP_TSX.read_text(encoding="utf-8")
    hook = UPDATE_HOOK.read_text(encoding="utf-8")
    live = LIVE_UPDATE.read_text(encoding="utf-8-sig")
    assert "useUpdateLifecycle" in app
    assert "checkAndApplyUpdate: checkUpdates" in app
    for marker in (
        "applyVerifiedUpdate",
        "getUpdateRecoveryStatus",
        "Установить обновление",
        "Установить и перезапустить",
        "apply_verified_update",
        "recoverable_failure",
        "rolled_back",
        "автоматически восстановлено",
        "Обновление до версии",
    ):
        assert marker in hook, marker
    assert "Проверить обновления" in live
    assert "Установить и перезапустить" in live
    assert "recovery.status -ne 'verified'" in live
    assert "FPR-19 LIVE UPDATE PASS:" in live


def test_fpr19_register_stays_open_until_installed_upgrade_and_recovery_proof() -> None:
    payload = json.loads(REGISTER.read_text(encoding="utf-8"))
    entry = next(item for item in payload["features"] if item["id"] == "FPR-19")

    assert entry["status"] == "needs-runtime-proof"
    assert entry["runtime_evidence"] == []
    assert "previous-version -> current-version" in entry["runtime_gap"]
    assert "recoverable" in entry["runtime_gap"]
    assert "tests/test_fpr19_update_recovery_contract.py" in entry["evidence_paths"]
