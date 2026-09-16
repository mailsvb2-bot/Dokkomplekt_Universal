from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text("utf-8")


def test_learning_backed_update_requires_fresh_exact_candidate_validation() -> None:
    source = read("src-tauri/src/subsystems/document_commands.rs")
    start = source.index("fn update_document_template(")
    end = source.index("fn archive_template_version_source(", start)
    update = source[start:end]

    assert "learning_validation_id: Option<String>" in source
    assert "template_learning_validation_by_output_sha256(candidate_snapshot.sha256())" in update
    assert "current_requires_revalidation" in update
    assert 'validation.status != "ready_to_publish"' in update
    assert "supplied_validation_id != Some(validation.validation_id.as_str())" in update
    assert "Replay + Intervention + Held-out validation" in update
    assert "learning_validation_id.clone()" in update
    assert '"template_repair_published"' in update
    assert '"repair_of_version_id"' in update


def test_regression_check_records_drift_before_repair_publication() -> None:
    source = read("src-tauri/src/subsystems/document_commands.rs")
    start = source.index("fn check_template_regression(")
    end = source.index("fn update_document_template(", start)
    check = source[start:end]

    assert '"template_drift_detected"' in check
    assert "candidate_snapshot.ensure_current()?;" in check
    assert '"candidate_sha256"' in check
    assert '"issues"' in check


def test_ui_stages_revalidated_repair_instead_of_overwriting_published() -> None:
    source = read("src/components/AdvancedToolsPanel.tsx")
    assert "stageLearnedRepair" in source
    assert "setReplacementValidationId(learningReport.validation_id)" in source
    assert "selectedVersionUsesLearningProof && !replacementValidationId" in source
    assert "updateDocumentTemplate(versionedDocument.id, replacementPath, acknowledge, validationId)" in source
    assert "предыдущая становится Superseded" in source


def test_legacy_template_migration_preserves_non_learning_version_contract() -> None:
    legacy = read("src-tauri/src/subsystems/legacy_template_runtime.rs")
    anchor = "Автоматическая миграция старого doctor-owned шаблона"
    anchor_index = legacy.index(anchor)
    call_start = legacy.rfind("prepare_template_version_draft(", 0, anchor_index)
    call_end = legacy.index(")?;", anchor_index) + 3
    call = legacy[call_start:call_end]
    assert "None," in call
    assert "Some(" not in call
