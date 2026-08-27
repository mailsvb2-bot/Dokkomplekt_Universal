from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text("utf-8")


def function_slice(source: str, name: str, next_marker: str = "#[tauri::command]") -> str:
    needles = [f"fn {name}(", f"fn {name}<"]
    starts = [source.find(needle) for needle in needles]
    start = min(position for position in starts if position >= 0)
    end = source.find(next_marker, start + 4)
    return source[start:] if end < 0 else source[start:end]


def test_default_state_mutations_are_candidate_first_and_publish_after_durable_save() -> None:
    main = read("src-tauri/src/main.rs")
    tx_source = read("src-tauri/src/state_transaction.rs")
    assert "mod state_transaction;" in main
    assert "use state_transaction::transact_default_state;" in main
    assert "struct PersistedDesktopState" in tx_source
    assert "fn prepare_and_persist_state_mutation" in tx_source
    assert "fn transact_default_state" in tx_source
    tx = function_slice(tx_source, "transact_default_state", "#[cfg(test)]")
    gate = tx.index(".persistence_gate")
    clone = tx.index("let current = PersistedDesktopState")
    save = tx.index("save_desktop_snapshot")
    publish = tx.index("if let Some(next) = prepared.next_state")
    assert gate < clone < save < publish
    assert "SQLite I/O" in tx
    assert "fn persist_default_state(" not in main


def test_all_runtime_default_state_writes_use_transaction_boundary() -> None:
    source_root = ROOT / "src-tauri/src"
    offenders = []
    for path in source_root.rglob("*.rs"):
        text = path.read_text("utf-8")
        if "persist_default_state(" in text:
            offenders.append(str(path.relative_to(ROOT)))
    assert offenders == []

    document = read("src-tauri/src/subsystems/document_commands.rs")
    for command in [
        "rename_document_button",
        "remove_document_button",
        "update_document_popup_fields",
        "set_field",
        "apply_popup",
        "apply_popup_batch",
        "apply_scanner",
        "verify_rust_license_text",
    ]:
        assert "transact_default_state" in function_slice(document, command)

    intake = read("src-tauri/src/subsystems/source_intake_commands.rs")
    for command in ["reset_case", "parse_source", "parse_web_source"]:
        assert "transact_default_state" in function_slice(intake, command)

    # Both desktop source-file entry points intentionally share one canonical
    # byte-intake helper. The wrappers themselves do no state mutation; the
    # helper owns the same durable transaction boundary for both paths.
    shared_file_intake = function_slice(intake, "parse_source_file_bytes")
    assert "transact_default_state" in shared_file_intake
    wrappers = {
        "parse_source_file": intake[
            intake.index("fn parse_source_file(") : intake.index("#[tauri::command]\nasync fn pick_source_file")
        ],
        "parse_source_path": intake[
            intake.index("fn parse_source_path(") : intake.index("fn validate_source_path(")
        ],
    }
    for command, body in wrappers.items():
        assert "parse_source_file_bytes" in body, command
        assert "retained_uploaded_source" not in body, command
        assert "source_provenance" not in body, command

    automation = read("src-tauri/src/subsystems/automation_runtime.rs")
    assert "transact_default_state" in function_slice(automation, "semantic_extract")
    knowledge = read("src-tauri/src/subsystems/knowledge_registry.rs")
    assert "transact_default_state" in function_slice(knowledge, "apply_organization_knowledge")


def test_source_transients_are_changed_only_after_persisted_candidate_succeeds() -> None:
    intake = read("src-tauri/src/subsystems/source_intake_commands.rs")
    expected_mutations = {
        "reset_case": ["retained.take();", "provenance.take();"],
        "parse_source": ["retained.take();", "*source_provenance = Some(provenance);"],
        "parse_web_source": ["retained.take();", "*source_provenance = Some(provenance);"],
        "parse_source_file_bytes": [
            "*retained_slot = Some(retained_source);",
            "*provenance_slot = Some(provenance);",
        ],
    }
    for command, mutations in expected_mutations.items():
        body = function_slice(intake, command)
        lock = body.index("lock_source_session_state")
        transaction = body.index("transact_default_state")
        # Fallible mutex acquisition happens before SQLite commit, so a poisoned
        # transient lock can never make the command fail after switching durable case.
        assert lock < transaction
        # Actual transient values still change only after durable persistence succeeds.
        for mutation in mutations:
            assert body.index(mutation) > transaction

    wrappers = {
        "parse_source_file": intake[
            intake.index("fn parse_source_file(") : intake.index("#[tauri::command]\nasync fn pick_source_file")
        ],
        "parse_source_path": intake[
            intake.index("fn parse_source_path(") : intake.index("fn validate_source_path(")
        ],
    }
    for command, body in wrappers.items():
        assert "parse_source_file_bytes" in body, command
        assert "retained_uploaded_source" not in body, command
        assert "source_provenance" not in body, command


def test_injected_persistence_failure_success_and_noop_have_rust_regressions() -> None:
    tx = read("src-tauri/src/state_transaction.rs")
    assert "persistence_failure_never_returns_a_candidate_for_publication" in tx
    assert 'Err("injected persistence failure".into())' in tx
    assert "only_durably_persisted_candidate_is_returned_for_publication" in tx
    assert "unchanged_mutation_does_not_touch_persistence" in tx
