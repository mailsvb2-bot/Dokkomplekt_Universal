from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_e6_compiler_and_scanner_cache_quotas_are_bounded_and_ownership_safe() -> None:
    workspace = read("src-tauri/src/universal_intake/workspace_session.rs")
    universal = read("src-tauri/src/universal_intake.rs")
    legacy = read("src-tauri/src/subsystems/legacy_template_runtime.rs")
    documents = read("src-tauri/src/subsystems/document_commands.rs")
    privacy = read("src-tauri/src/privacy_runtime.rs")
    main = read("src-tauri/src/main.rs")

    assert "enforce_ephemeral_workspace_quota" in workspace
    assert "enforce_ephemeral_workspace_group_quota" in workspace
    assert "owned_workspace_bytes" in workspace
    assert "owned_workspace_group_bytes" in workspace
    assert "group_quota_is_shared_across_backing_workspaces" in workspace
    assert "quota_pressure_evicts_released_owned_cache_but_never_live_session" in workspace
    assert "quota_fails_closed_when_live_session_consumes_reserved_capacity" in workspace
    assert "unknown_workspace_entries_are_not_counted_or_deleted_by_cache_quota" in workspace
    assert "never candidates regardless of which backing workspace owns them" in workspace
    assert "create_owned_workspace_session" in universal

    assert "TEMPLATE_COMPILER_CACHE_QUOTA_BYTES: u64 = 1024 * 1024 * 1024" in legacy
    assert "TEMPLATE_COMPILER_CACHE_RETENTION_SECONDS: u64 = 24 * 60 * 60" in legacy
    assert "TEMPLATE_COMPILER_CACHE_MIN_RESERVE_BYTES: u64 = 64 * 1024 * 1024" in legacy
    assert "TEMPLATE_COMPILER_CACHE_EXPANSION_FACTOR: u64 = 8" in legacy
    assert legacy.count("ensure_template_compiler_cache_capacity(") >= 4
    assert "template_compiler_cache_roots" in legacy
    assert "enforce_ephemeral_workspace_group_quota" in legacy
    assert "create_template_compiler_workspace(app, \"template-contract-migration\")" in legacy
    assert "create_template_compiler_workspace(app, \"template-render-inference\")" in legacy
    assert '"template-inference-work"' in legacy

    assert "WORD_SCANNER_CACHE_QUOTA_BYTES: u64 = 256 * 1024 * 1024" in documents
    assert "WORD_SCANNER_CACHE_RETENTION_SECONDS: u64 = 24 * 60 * 60" in documents
    scanner_block = documents[
        documents.index('let original = if req.path.starts_with("dokkomplekt-upload://current/")'):
        documents.index("if !original.is_file()")
    ]
    assert scanner_block.index("enforce_ephemeral_workspace_quota(") < scanner_block.index(
        "retained.materialize(&workspace)?"
    )

    assert '"template-compiler-cache"' in privacy
    assert '"Кэш compiler шаблонов"' in privacy
    assert '"word-scanner-cache"' in privacy
    assert '"Кэш Word-сканера"' in privacy
    assert "TEMPLATE_COMPILER_CACHE_QUOTA_BYTES" in privacy
    assert "WORD_SCANNER_CACHE_QUOTA_BYTES" in privacy

    assert 'remove_dir_all(data_dir.join("word-scanner-work"))' not in main
    assert "unknown files are not app-owned cache" in main
