from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_dropped_primary_is_unconditionally_published_with_patient_documents() -> None:
    runtime = (ROOT / "src-tauri/src/subsystems/automation_runtime.rs").read_text("utf-8")
    assert "std::fs::copy(source_snapshot.path(), stage.join(&source_target_name))" in runtime
    old_conditional_copy = "if privacy.copy_source_to_output {\n                    std::fs::copy(source_snapshot.path(), stage.join(&source_target_name))"
    assert old_conditional_copy not in runtime
    assert '"source_copied": true' in runtime


def test_service_trust_report_is_routed_outside_patient_stage() -> None:
    desktop_io = (ROOT / "src-tauri/src/subsystems/desktop_io.rs").read_text("utf-8")
    assert '.starts_with(".dokkomplekt-stage-")' in desktop_io
    assert '.starts_with(".dokkomplekt-manual-stage-")' in desktop_io
    assert 'join("_служебные_отчёты")' in desktop_io
    assert 'ПРОВЕРИТЬ_КОМПЛЕКТ-{}-{}.txt' in desktop_io


def test_background_drop_opens_or_activates_normal_ui_without_second_brain() -> None:
    watcher = (ROOT / "src-tauri/src/subsystems/watcher_commands.rs").read_text("utf-8")
    assert "launch_or_activate_watcher_ui" in watcher
    assert "spawn_silent_executable(&target, false)" in watcher
    assert "--background-watch" in watcher
    assert "CREATE_NO_WINDOW" in watcher
    assert "powershell" not in watcher.lower()


def test_watcher_update_handoff_is_sha_bound_drain_first_and_backward_compatible() -> None:
    watcher = (ROOT / "src-tauri/src/subsystems/watcher_commands.rs").read_text("utf-8")
    assert "struct WatcherHandoffOwner" in watcher
    assert "executable_sha256" in watcher
    assert "#[serde(default)]\n    handoff_owner: Option<WatcherHandoffOwner>" in watcher
    assert "owner.ready = true" in watcher
    assert "active == 0" in watcher
    assert "release_watcher_instance_lock" in watcher
    assert "handoff_watcher_to_successor" in watcher


def test_manual_and_watched_medical_runs_bind_the_exact_sick_leave_choice() -> None:
    manual = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text("utf-8")
    automation = (ROOT / "src-tauri/src/subsystems/automation_runtime.rs").read_text("utf-8")
    marker = "set_medical_sick_leave_choice("

    batch = manual[manual.index("fn render_docx_batch(") : manual.index("fn apply_scanner(")]
    assert marker in batch
    assert "&mut base_case" in batch
    assert "req.sick_leave_enabled" in batch

    intake = automation[
        automation.index("let flags = WorkflowFlags") : automation.index("let batch = plan_created_documents_batch")
    ]
    assert marker in intake
    assert "&mut case" in intake
    assert "document_required_input_fields" in intake


def test_derived_medical_outputs_are_replaced_by_real_input_facts() -> None:
    workflow = (ROOT / "crates/dokkomplekt-core/src/workflow_engine.rs").read_text("utf-8")
    profiles = (ROOT / "crates/dokkomplekt-core/src/popup_profiles.rs").read_text("utf-8")

    assert "profession_derived_field_sources" in workflow
    assert "document_required_input_fields" in workflow
    assert "MEDICAL_EXPERT_ANAMNESIS" in profiles
    assert "MEDICAL_WORK_POSITION" in profiles
    assert '"medical.sick_leave_number"' in profiles
