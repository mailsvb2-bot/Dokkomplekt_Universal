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



def test_specialist_rule_uses_exact_decision_key_and_one_persistence_gate() -> None:
    source = read("src-tauri/src/subsystems/source_intake_commands.rs")
    automation = read("src-tauri/src/subsystems/automation_runtime.rs")

    load_start = source.index("fn load_specialist_kit_decision_from_repo(")
    persist_start = source.index("fn persist_specialist_kit_rule_in_repo(")
    claim_start = source.index("fn claim_bundle_exception_confirmation(")
    repo_resolver_start = source.index("fn resolve_document_bundle_for_case_with_repo(")
    resolver_start = source.index("fn resolve_document_bundle_for_case(")

    # Helpers called from an existing state transaction must never recursively
    # acquire the non-reentrant persistence mutex. The outer command/wrapper owns it.
    assert "persistence_gate" not in source[load_start:persist_start]
    assert "persistence_gate" not in source[persist_start:claim_start]
    assert "persistence_gate" in source[claim_start:repo_resolver_start]
    assert "persistence_gate" not in source[repo_resolver_start:resolver_start]
    assert "persistence_gate" in source[resolver_start:]
    assert "resolve_document_bundle_for_case_with_repo" in source[resolver_start:]

    # A stale process-local pack may fail to apply a remembered rule, but it must
    # not delete shared memory that another process just made valid.
    load_body = source[load_start:persist_start]
    assert "save_state_value" not in load_body
    assert "never deleted" in load_body

    # All three interactive import routes reuse the already-held transaction gate.
    assert source.count("resolve_document_bundle_for_case_with_repo(") == 5
    assert "specialist_rule_key_from_exception_details" in source
    assert "Option<KitRuleKey>), String>" in source
    assert '"specialist_rule_key": &specialist_rule_key' in automation
    assert "resolve_exception_and_save_state_value" in source
