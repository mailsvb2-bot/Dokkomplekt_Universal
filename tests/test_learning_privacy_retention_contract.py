from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text("utf-8")


def test_destructive_cleanup_fails_closed_when_privacy_policy_cannot_load() -> None:
    privacy = read("src-tauri/src/privacy_runtime.rs")
    cleanup = privacy[privacy.index("pub(crate) fn cleanup_intake_workspace"):]
    assert "let privacy = load_privacy_preferences(app)?;" in cleanup
    assert "load_privacy_preferences(app).unwrap_or_default()" not in cleanup
    assert '"template-learning-inputs"' in cleanup
    assert '"template-learning-work"' in cleanup
    assert "lock_learning_workspace()?" in cleanup


def test_learning_imports_live_in_active_app_data_sessions() -> None:
    document = read("src-tauri/src/subsystems/document_commands.rs")
    assert 'join("template-learning-inputs")' in document
    assert "create_retained_workspace_session(&root)?" in document
    assert "let target = session_root.join(safe_name);" in document
    assert 'let work = session_root.join("normalized-work");' in document
    assert "refresh_retained_workspace_session(&learning_root, &path)?" in document
    assert document.count("lock_learning_workspace()?") >= 2


def test_zero_hour_active_lease_and_non_learning_isolation_have_rust_regressions() -> None:
    intake = read("src-tauri/src/universal_intake.rs")
    assert "retained_learning_session_survives_zero_hour_cleanup_while_lease_is_active" in intake
    assert "zero_hour_cleanup_removes_released_learning_session_without_touching_other_root" in intake
    assert "retained_learning_lease_refresh_ignores_paths_outside_workspace" in intake
    assert "symlink_metadata(&session_root)" in intake
    assert "metadata.file_type().is_symlink()" in intake


def test_startup_cleanup_failure_is_visible_not_silently_discarded() -> None:
    main = read("src-tauri/src/main.rs")
    assert "if let Err(error) = cleanup_intake_workspace(&handle)" in main
    assert "Очистка временных рабочих данных при запуске пропущена" in main
