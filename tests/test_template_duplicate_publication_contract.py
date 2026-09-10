import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def command_block(source: str, start_marker: str, end_marker: str) -> str:
    start = source.index(start_marker)
    end = source.index(end_marker, start)
    return source[start:end]


class TemplateDuplicatePublicationContractTests(unittest.TestCase):
    def test_duplicate_preflight_runs_before_template_archive_publication(self) -> None:
        runtime = text("src-tauri/src/subsystems/document_commands.rs")
        command = command_block(runtime, "fn confirm_template_setup(", "struct RenameDocumentButtonRequest")

        self.assertIn("document_pack_contains_template_source", command)
        self.assertIn("seen_sha256", command)
        self.assertIn("accepted_document_ids.is_empty()", command)
        self.assertLess(command.index("persistence_gate"), command.index("prepare_template_version_draft"))
        self.assertLess(command.index("document_pack_contains_template_source"), command.index("prepare_template_version_draft"))

    def test_atomic_publication_never_persists_orphan_template_version(self) -> None:
        runtime = text("src-tauri/src/subsystems/document_commands.rs")
        storage = text("crates/dokkomplekt-storage/src/lib.rs")
        locked = command_block(
            runtime,
            "fn publish_pack_with_template_versions_locked",
            "fn verify_published_template_version_file",
        )

        self.assertNotIn("effective_drafts", locked)
        self.assertIn("versions: drafts", locked)
        self.assertIn("is absent from candidate pack", storage)

    def test_confirm_reanalyzes_the_exact_snapshot_that_is_published(self) -> None:
        runtime = text("src-tauri/src/subsystems/document_commands.rs")
        compiler_runtime = text("src-tauri/src/subsystems/legacy_template_runtime.rs")
        command = command_block(runtime, "fn confirm_template_setup(", "struct RenameDocumentButtonRequest")
        helper = command_block(
            compiler_runtime,
            "fn reanalyze_confirmation_rows_from_snapshots",
            "#[cfg(test)]",
        )

        self.assertIn("snapshot.path()", helper)
        self.assertNotIn("resolve_user_path", helper)
        self.assertIn("domain_override_is_explicit", helper)
        self.assertIn("prepare_template_confirmations_with_existing_pack", helper)
        self.assertIn("if !row.popup_fields_edited", helper)
        self.assertIn("compiler_fields_by_document", helper)
        self.assertIn("merge_compiler_fields_into_analysis", helper)
        self.assertLess(command.index("TemplateSnapshot::capture"), command.index("reanalyze_confirmation_rows_from_snapshots"))
        self.assertLess(command.index("let existing_pack"), command.index("reanalyze_confirmation_rows_from_snapshots"))
        self.assertLess(command.index("reanalyze_confirmation_rows_from_snapshots"), command.index("create_pack_from_confirmations"))

    def test_generation_uses_the_effective_compiler_contract_end_to_end(self) -> None:
        runtime = text("src-tauri/src/subsystems/document_commands.rs")
        compiler_runtime = text("src-tauri/src/subsystems/legacy_template_runtime.rs")
        prepared = command_block(
            compiler_runtime,
            "struct PreparedMedicalRenderTemplate",
            "fn should_attempt_template_contract_compilation",
        )
        single = command_block(runtime, "fn render_docx(", "struct RenderDocxBatchRequest")
        batch = command_block(runtime, "fn render_docx_batch(", "struct OutputPlanRequest")

        self.assertIn("effective_document: DocumentTemplateSpec", prepared)
        self.assertIn("effective_document", single)
        self.assertIn("&effective_document.category", single)
        self.assertIn("&effective_document.role_id", single)
        self.assertIn("ensure_rendered_document_complete(", single)
        self.assertIn("effective_document,", single)
        self.assertNotIn("for (field_id, value) in &hydrated.case.values", batch)
        self.assertNotIn("report_case.values", batch)
        self.assertIn("let mut trust_document_evidence = Vec::new()", batch)
        self.assertIn("capture_trust_document_evidence(", batch)
        self.assertIn("&render_case", batch)
        self.assertIn("effective_document.placeholders.iter().cloned()", batch)
        self.assertIn("document_evidence: &trust_document_evidence", batch)
        self.assertLess(
            batch.index("ensure_rendered_document_complete("),
            batch.index("capture_trust_document_evidence("),
        )
        self.assertIn("&effective_document.category", batch)
        self.assertIn("&effective_document.role_id", batch)
        self.assertIn("ensure_rendered_document_complete(", batch)
        self.assertIn("effective_document,", batch)
        self.assertNotIn("flat_map(|document| document.placeholders", batch)

    def test_trust_evidence_is_preserved_per_document_in_manual_and_automation_paths(self) -> None:
        manual = text("src-tauri/src/subsystems/document_commands.rs")
        automation = text("src-tauri/src/subsystems/automation_runtime.rs")
        writer = text("src-tauri/src/subsystems/desktop_io.rs")

        manual_batch = command_block(manual, "fn render_docx_batch(", "struct OutputPlanRequest")
        self.assertIn("capture_trust_document_evidence(", manual_batch)
        self.assertIn("document_evidence: &trust_document_evidence", manual_batch)
        self.assertNotIn("report_case.values", manual_batch)

        self.assertIn("let mut trust_document_evidence = Vec::new()", automation)
        self.assertIn("evidence_case = render_case", automation)
        self.assertIn("capture_trust_document_evidence(", automation)
        self.assertIn("document_evidence: &trust_document_evidence", automation)
        self.assertNotIn("let mut report_case = planning_case.case.clone()", automation)

        self.assertIn("struct TrustDocumentEvidence", writer)
        self.assertIn("for evidence in document_evidence", writer)
        self.assertIn("Документ: {safe_document_name}", writer)
        self.assertIn("trust_report_preserves_conflicting_scoped_values_per_document", writer)

    def test_frontend_reports_duplicates_instead_of_claiming_full_success(self) -> None:
        app = text("src/App.tsx")
        support = text("src/lib/templateSetupSupport.ts")

        self.assertIn("previousDocumentIds", app)
        self.assertIn("templateSetupCompletionMessage(confirmedRows.length, createdCount)", app)
        self.assertIn("Повторяющихся шаблонов пропущено", support)
        self.assertIn("Новых кнопок не создано", support)


if __name__ == "__main__":
    unittest.main()
