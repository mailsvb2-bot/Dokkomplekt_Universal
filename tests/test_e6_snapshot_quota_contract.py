from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_template_snapshot_quota_is_preflighted_visible_and_backup_safe() -> None:
    snapshots = read("src-tauri/src/template_snapshot.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")

    assert "TEMPLATE_SNAPSHOT_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024" in snapshots
    assert "TEMPLATE_SNAPSHOT_RETENTION_SECONDS: u64 = 24 * 60 * 60" in snapshots
    assert "TEMPLATE_SANITIZED_QUOTA_BYTES: u64 = 1024 * 1024 * 1024" in snapshots
    assert "TEMPLATE_SANITIZED_RETENTION_SECONDS: u64 = 24 * 60 * 60" in snapshots

    capture_block = snapshots[
        snapshots.index('join("template-snapshot-work")'):
        snapshots.index("fn capture_path")
    ]
    assert capture_block.index("enforce_ephemeral_workspace_quota(") < capture_block.index(
        "Self::capture_path("
    )
    assert "metadata" in capture_block
    assert ".len()" in capture_block
    assert ".saturating_add(TEMPLATE_SNAPSHOT_RESERVE_OVERHEAD_BYTES)" in capture_block

    sanitized_block = snapshots[
        snapshots.index('join("template-publication-sanitized-work")'):
        snapshots.index("self.publication = Some")
    ]
    assert sanitized_block.index("enforce_ephemeral_workspace_quota(") < sanitized_block.index(
        "materialize_sensitive_file("
    )
    assert "TEMPLATE_SANITIZED_RESERVE_OVERHEAD_BYTES" in sanitized_block

    assert '"template-snapshots"' in privacy
    assert '"Временные snapshot шаблонов"' in privacy
    assert '"template-sanitized-snapshots"' in privacy
    assert '"Временные очищенные копии шаблонов"' in privacy
    assert 'data_dir.join("template-snapshot-work")' in privacy
    assert 'data_dir.join("template-publication-sanitized-work")' in privacy
    assert "TEMPLATE_SNAPSHOT_QUOTA_BYTES" in privacy
    assert "TEMPLATE_SANITIZED_QUOTA_BYTES" in privacy

    cleanup = privacy[
        privacy.index("pub(crate) fn cleanup_intake_workspace"):
        privacy.index("pub(crate) fn start_periodic_intake_cleanup")
    ]
    assert "template-snapshot-work" in cleanup
    assert "template-publication-sanitized-work" in cleanup
    assert ".dokkomplekt-backups" not in cleanup
    assert "update-backups" not in cleanup
    assert "key.raw." not in cleanup
