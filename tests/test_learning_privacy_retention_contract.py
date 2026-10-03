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
    document = read("src-tauri/src/subsystems/template_learning_commands.rs")
    assert 'join("template-learning-inputs")' in document
    assert "create_retained_workspace_session(&root)?" in document
    assert "let target = session_root.join(safe_name);" in document
    assert 'let work = session_root.join("normalized-work");' in document
    assert "refresh_retained_workspace_session(&learning_root, &path)?" in document
    assert document.count("lock_learning_workspace()?") >= 2


def test_zero_hour_active_lease_and_non_learning_isolation_have_rust_regressions() -> None:
    intake = read("src-tauri/src/universal_intake.rs")
    workspace_session = read("src-tauri/src/universal_intake/workspace_session.rs")
    assert "retained_learning_session_survives_zero_hour_cleanup_while_lease_is_active" in intake
    assert "zero_hour_cleanup_removes_released_learning_session_without_touching_other_root" in intake
    assert "retained_learning_lease_refresh_ignores_paths_outside_workspace" in intake
    assert "symlink_metadata(&session_root)" in workspace_session
    assert "metadata_is_link_like(&metadata)" in workspace_session


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


def test_learning_output_cannot_bypass_validation_via_manual_confirm() -> None:
    source = read("src-tauri/src/subsystems/document_commands.rs")
    confirm = source[source.index("fn confirm_template_setup("):source.index("fn rename_document_button(")]
    assert "template_learning_validation_by_output_sha256(snapshot.sha256())" in confirm
    assert "supplied != Some(validation.validation_id.as_str())" in confirm
    assert "validation proof" in confirm


def test_canon_24_4_exposes_technical_storage_size_and_manual_cleanup_covers_temp_sessions() -> None:
    privacy = read("src-tauri/src/privacy_runtime.rs")
    management = read("src-tauri/src/subsystems/automation_management.rs")
    main = read("src-tauri/src/main.rs")
    api = read("src/lib/api.ts")
    types = read("src/lib/types.ts")
    ui = read("src/components/AutomationControlCenter.tsx")

    assert "pub(crate) struct TechnicalStorageStatus" in privacy
    assert '"intake-work"' in privacy
    assert '"Временные исходники"' in privacy
    assert '"template-learning-inputs"' in privacy
    assert '"Входы обучения шаблонов"' in privacy
    assert '"template-learning-work"' in privacy
    assert '"Рабочие данные обучения"' in privacy
    assert 'key: "other-app-data".into()' in privacy
    assert "retention_managed: false" in privacy
    assert "fn get_technical_storage_status(" in management
    assert "collect_technical_storage_status(&app)" in management
    assert "cleanup_intake_workspace(&app)" in management
    assert "get_technical_storage_status," in main
    assert "getTechnicalStorageStatus" in api
    assert "'get_technical_storage_status'" in api
    assert "export interface TechnicalStorageStatus" in types
    assert "Учтённые технические данные" in ui
    assert "Под политикой хранения" in ui
    assert "не удаляется этой очисткой" in ui
    assert "Очистить сейчас" in ui
    assert "publication_metadata_is_link_or_reparse(&metadata)" in privacy
    assert "ссылкой/reparse point" in privacy
    assert "loadOptionalTechnicalStorage()" in ui
    assert "technicalStorageUnavailable" in ui
    assert "Остальные настройки и рабочие функции продолжают работать." in ui
    assert '"runtime-logs"' in privacy
    assert '"Журналы фонового агента"' in privacy
    assert "WATCHER_LOG_TOTAL_QUOTA_BYTES" in privacy
    assert "WATCHER_LOG_RETENTION_SECONDS" in privacy
    assert "quota_bytes: Option<u64>" in privacy
    assert "retention_seconds: Option<u64>" in privacy
    watcher_log = read("src-tauri/src/watcher_log.rs")
    assert "WATCHER_LOG_ACTIVE_QUOTA_BYTES: u64 = 4 * 1024 * 1024" in watcher_log
    assert "WATCHER_LOG_TOTAL_QUOTA_BYTES: u64 = 16 * 1024 * 1024" in watcher_log
    assert "WATCHER_LOG_RETENTION_SECONDS: u64 = 14 * 24 * 60 * 60" in watcher_log
    assert "A filename pattern is not ownership" in watcher_log
    assert "runtime_log_rotation_enforces_total_quota_and_archive_count" in watcher_log
    assert "cleanup_preserves_unknown_lookalike_file" in watcher_log
    assert "owned_watcher_log_bytes" in watcher_log
    assert "unsafe_archive_blocks_append_before_log_can_grow" in watcher_log
    assert "oversized_active_log_fails_cleanup_without_panicking_or_deleting" in watcher_log
    assert "initialize_active_log_atomically" in watcher_log
    assert "atomic_initialization_cleans_temp_when_publish_fails" in watcher_log
    assert "retire_legacy_watcher_log" in watcher_log
    assert "legacy_app_data_log_is_migrated_into_owned_bounded_archive" in watcher_log
    assert "unrecognized_legacy_file_is_preserved_and_not_claimed_as_owned" in watcher_log
    assert "owned_watcher_log_bytes(" in privacy
    assert "item.quota_bytes !== null" in ui
    assert "formatRetentionDuration(item.retention_seconds)" in ui
    assert "управляется политикой хранения" in ui
    universal = read("src-tauri/src/universal_intake.rs")
    workspace_session = read("src-tauri/src/universal_intake/workspace_session.rs")
    assert "mod workspace_session;" in universal
    assert "SESSION_OWNERSHIP_MARKER" in universal
    assert 'SESSION_OWNERSHIP_MARKER: &str = ".dokkomplekt-owned-session"' in workspace_session
    assert "session_has_verified_ownership" in workspace_session
    assert "validate_existing_workspace_root" in workspace_session
    assert "metadata_is_link_like(&metadata)" in workspace_session
    assert "two-pass validation must avoid partial cleanup" in universal
    assert "cleanup_preserves_unknown_workspace_entries_without_ownership_marker" in universal
