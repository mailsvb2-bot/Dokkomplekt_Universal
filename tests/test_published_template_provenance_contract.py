from pathlib import Path


def test_active_templates_bind_to_published_archives_and_update_uses_one_snapshot():
    source = Path("src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
    main = Path("src-tauri/src/main.rs").read_text(encoding="utf-8")
    startup = Path("src-tauri/src/subsystems/startup_state.rs").read_text(encoding="utf-8")

    assert "bind_loaded_pack_to_published_template_versions" in source
    assert "verify_published_template_version_file(&archived_path, &record)?" in source
    assert "document.template_path = draft.template_path.clone();" in source
    assert "let mut incoming = create_pack_from_confirmations" in source
    assert "let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(" in source
    assert "compare_candidate_to_published_template(" in source
    assert "candidate_snapshot.path()," in source
    assert "let template_sha256 = candidate_snapshot.sha256().to_string();" in source
    assert "updated.template_path = draft.template_path.clone();" in source
    assert "candidate_snapshot.ensure_current()?;" in source
    assert "load_state_from(\n    app: &tauri::AppHandle," in source
    assert "load_state_from(&app, &db_path, &state, false)?;" in source
    assert "ensure_default_state_loaded(&handle, &state)" in main
    assert "load_state_from_locked(app, &db_path, state, true)" in startup

    loader_start = source.index("fn load_state_from_locked(")
    loader_end = source.index("fn load_state_from(", loader_start)
    loader = source[loader_start:loader_end]
    assert "let loaded_pack = repo.load_pack" in loader
    assert "bind_loaded_pack_to_published_template_versions(app, &repo, &mut pack)?" in loader
    assert loader.index("let loaded_pack = repo.load_pack") < loader.index(
        "bind_loaded_pack_to_published_template_versions"
    )
    assert loader.index("bind_loaded_pack_to_published_template_versions") < loader.index(
        "let mut case_guard"
    )
    assert loader.index("bind_loaded_pack_to_published_template_versions") < loader.index(
        "let mut pack_guard"
    )


def test_mutable_live_path_is_not_reintroduced_by_registration_update_or_rollback():
    source = Path("src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")
    assert "updated.template_path = draft.template_path.clone();" in source
    assert "verify_published_template_version_file(&path, &record)?;" in source
    assert "compare_docx_structures(&previous_path, candidate_path)" in source
