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
        "reset_case",
        "parse_source",
        "parse_source_file",
        "parse_web_source",
        "apply_popup",
        "apply_popup_batch",
        "apply_scanner",
        "verify_rust_license_text",
    ]:
        assert "transact_default_state" in function_slice(document, command)

    automation = read("src-tauri/src/subsystems/automation_runtime.rs")
    assert "transact_default_state" in function_slice(automation, "semantic_extract")
    knowledge = read("src-tauri/src/subsystems/knowledge_registry.rs")
    assert "transact_default_state" in function_slice(knowledge, "apply_organization_knowledge")


def test_source_transients_are_changed_only_after_persisted_candidate_succeeds() -> None:
    document = read("src-tauri/src/subsystems/document_commands.rs")
    for command in ["reset_case", "parse_source", "parse_source_file", "parse_web_source"]:
        body = function_slice(document, command)
        transaction = body.index("transact_default_state")
        for token in ["retained_uploaded_source", "source_provenance"]:
            position = body.find(token)
            if position >= 0:
                assert position > transaction


def test_injected_persistence_failure_success_and_noop_have_rust_regressions() -> None:
    tx = read("src-tauri/src/state_transaction.rs")
    assert "persistence_failure_never_returns_a_candidate_for_publication" in tx
    assert 'Err("injected persistence failure".into())' in tx
    assert "only_durably_persisted_candidate_is_returned_for_publication" in tx
    assert "unchanged_mutation_does_not_touch_persistence" in tx
