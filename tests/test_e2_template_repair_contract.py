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


def test_zero_touch_requires_validated_automatic_template_proof() -> None:
    learning = read("src-tauri/src/subsystems/template_learning_commands.rs")
    automation = read("src-tauri/src/subsystems/automation_runtime.rs")

    assert '"validation_level": "validated_automatic"' in learning
    assert "fn learning_validation_allows_zero_touch(" in automation
    assert 'level == "validated_automatic"' in automation
    assert '"automatic_validation_missing"' in automation
    assert '"template_automation_admission"' in automation
    assert '"intake_blocked_template_admission"' in automation
    assert '"required_level": "validated_automatic"' in automation
    assert '"template_admission_contract": "validated-automatic-v1"' in automation

    selection = automation.index("let selected_document_ids = bundle_decision")
    admission = automation.index("zero_touch_template_admission_blockers(", selection)
    configured = automation.index("let mut configured = Vec::new();", admission)
    assert selection < admission < configured


def test_new_template_publication_has_capability_manifest_gate() -> None:
    docx = read("crates/dokkomplekt-docx/src/lib.rs")
    commands = read("src-tauri/src/subsystems/document_commands.rs")
    admission_owner = read("src-tauri/src/subsystems/template_capability_admission.rs")

    assert "pub struct DocxCapabilityManifest" in docx
    assert "pub fn inspect_docx_capabilities_file(" in docx
    assert '"custom_xml_requires_explicit_sanitization_policy"' in docx
    assert '"data_binding_not_supported_for_published_reference"' in docx
    assert '"revision_markup_requires_explicit_sanitization_policy"' in docx
    assert '"comments_require_explicit_sanitization_policy"' in docx
    assert 'include!("template_capability_admission.rs");' in commands
    assert "inspect_docx_capabilities_file(snapshot.path())" in admission_owner
    assert '"template_capability_admission_passed"' in admission_owner
    assert "custom_xml_only" in admission_owner
    assert "learning_validation_id.is_none()" in admission_owner
    assert "snapshot.sanitize_hidden_custom_xml_for_publication(app)?" in admission_owner

    admission = commands.index("admit_template_capabilities_for_publication(")
    pack_creation = commands.index(
        'create_pack_from_confirmations("incoming", "Новые шаблоны", &rows).pack',
        admission,
    )
    assert admission < pack_creation
