from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_update_backup_quota_is_owned_bounded_and_recovery_safe() -> None:
    runtime = read("src-tauri/src/subsystems/update_runtime.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")

    assert 'UPDATE_BACKUP_OWNERSHIP_SCHEMA: &str = "dokkomplekt.update-backup.v1"' in runtime
    assert 'UPDATE_BACKUP_OWNERSHIP_FILE: &str = ".dokkomplekt-update-backup.json"' in runtime
    assert "UPDATE_BACKUP_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024" in runtime
    assert "UPDATE_BACKUP_MAX_ENTRIES: u64 = 8" in runtime
    assert "UPDATE_BACKUP_RETENTION_SECONDS: u64 = 180 * 24 * 60 * 60" in runtime

    assert "fn collect_owned_update_backups" in runtime
    assert "fn enforce_update_backup_policy_at" in runtime
    assert "fallback_protected_index" in runtime
    assert "protected_update_backup_path" in runtime
    assert "load_update_recovery_state(app)?" in runtime
    assert "write_update_backup_ownership_marker(" in runtime
    assert "create_new(true)" in runtime

    apply_start = runtime.index("fn apply_verified_update(")
    apply = runtime[apply_start:]
    assert apply.index("lock_update_backup_policy()?") < apply.index("backup_update_state(")
    assert apply.index("backup_update_state(") < apply.index("write_update_recovery_state(")
    assert apply.index("write_update_recovery_state(") < apply.index(
        "enforce_update_backup_storage_policy_unlocked"
    )
    assert "drop(_backup_policy_guard)" in apply

    for regression in (
        "update_backup_policy_preserves_active_recovery_and_unowned_legacy",
        "update_backup_policy_keeps_newest_as_sole_recovery_when_marker_is_absent",
        "malformed_update_backup_marker_blocks_destructive_cleanup",
    ):
        assert regression in runtime

    assert '"update-backups"' in privacy
    assert "Backups обновлений (активный/последний recovery защищён)" in privacy
    assert "UPDATE_BACKUP_QUOTA_BYTES" in privacy
    assert "UPDATE_BACKUP_RETENTION_SECONDS" in privacy
    assert "owned_update_backup_bytes(app)?" in privacy

    cleanup = privacy[
        privacy.index("pub(crate) fn cleanup_intake_workspace"):
        privacy.index("pub(crate) fn start_periodic_intake_cleanup")
    ]
    assert "enforce_update_backup_storage_policy(app)?" in cleanup
    assert 'remove_dir_all(&data_dir.join("update-backups"))' not in cleanup
