from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_final_workflow_plan_is_bounded_by_selected_template_fields() -> None:
    source = text("crates/dokkomplekt-core/src/workflow_engine.rs")
    assert "selected_document_fields(document, flags)" in source
    assert ".filter(|field_id| relevant.contains(field_id))" in source
    assert ".filter(|config| relevant.contains(&config.field_id))" in source
    assert "accounting_profile_does_not_force_fields_absent_from_selected_template" in source
    assert "explicitly_configured_popup_field_remains_in_final_plan" in source


def test_preflight_uses_selected_single_or_batch_generation_plan() -> None:
    app = text("src/App.tsx")
    workspace = text("src/components/Workspace.tsx")
    assert "const [preflightPlan, setPreflightPlan]" in app
    assert "loadWorkflowPlan(selectedDocIds)" in app
    assert "getWorkflowPlanBatch(documentIds, sickLeaveEnabled, parts)" in app
    assert "plan={preflightPlan}" in app
    assert "documentRevisionTokens: generationDocumentRevisionTokens(documents, selectedDocIds)" in app
    assert "generationDocumentRevisionsMatch(snapshot.documentRevisionTokens, documents)" in app
    assert "snapshot.sickLeaveEnabled" in app
    assert "snapshot.folderParts" in app
    assert "Сообщение о готовности появится только после расчёта финального generation-plan" in workspace
    assert "Финальный план проверен: обязательных уточнений" in workspace


def test_normal_popup_hides_technical_field_identifiers() -> None:
    source = text("src/components/RuntimePromptModal.tsx")
    assert "{prompt.title}" in source
    assert "<small>{prompt.field_id}</small>" not in source


def test_learning_and_approval_governance_is_user_accessible() -> None:
    source = text("src/components/LearningGovernancePanel.tsx")
    utility = text("src/components/UtilityPanel.tsx")
    for call in (
        "deleteLearnedScannerRule",
        "getLearnedKitDecision",
        "listTemplateApprovals",
        "revokeDocumentTemplateApproval",
    ):
        assert call in source
    assert "<LearningGovernancePanel" in utility


def test_postgres_integration_is_mandatory_and_cannot_silently_skip() -> None:
    workflow = text(".github/workflows/quality-gate.yml")
    config = text("crates/dokkomplekt-license-server/src/config.rs")
    assert "postgres:16-alpine" in workflow
    assert "DATABASE_URL: postgresql://" in workflow
    assert "DOKKOMPLEKT_REQUIRE_POSTGRES_TESTS: '1'" in workflow
    assert "test -f crates/dokkomplekt-license-server/Cargo.lock" in workflow
    assert "cargo test --locked --manifest-path crates/dokkomplekt-license-server/Cargo.toml" in workflow
    assert "postgres-integration" in workflow
    assert "DATABASE_URL is required because DOKKOMPLEKT_REQUIRE_POSTGRES_TESTS=1" in config


def test_oversized_modules_are_split_into_focused_submodules() -> None:
    assert len(text("src-tauri/src/universal_intake.rs").splitlines()) < 3000
    assert (ROOT / "src-tauri/src/universal_intake/archive.rs").is_file()
    assert (ROOT / "src-tauri/src/universal_intake/web.rs").is_file()
    assert len(text("src/App.tsx").splitlines()) < 1550
    assert (ROOT / "src/lib/appSupport.ts").is_file()


def test_platform_verification_now_includes_macos_and_native_webkit_pixels() -> None:
    macos = text(".github/workflows/macos-smoke.yml")
    linux = text("tests/installer/linux_installer_contract_core.sh")
    probe = text("scripts/verify_rendered_x11_window.py")
    golden = json.loads(text("tests/fixtures/ui/webkit-linux-golden.json"))
    assert "macos-14" in macos
    assert "tauri build --bundles app,dmg" in macos
    assert "verify_webkit_pixel_golden.py" in linux
    assert "--min-colors 64" in linux
    assert "--screenshot" in linux
    assert "minimum_colors == 1" not in probe
    assert golden["schema"] == "dokkomplekt.webkit-pixel-golden.v1"


def test_verification_status_does_not_freeze_historical_test_counts() -> None:
    status = text("CURRENT_VERIFICATION_STATUS.txt")
    readme = text("README.md")
    assert "212 passed" not in status
    assert "Repair date: 2026-07-26" not in status
    assert "What not declared ready in 18.3.2" not in readme
    assert "Что не объявляется готовым в 18.3.2" not in readme
