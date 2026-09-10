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
        self.assertIn("used_field_ids.extend(effective_document.placeholders.iter().cloned())", batch)
        self.assertIn("&effective_document.category", batch)
        self.assertIn("&effective_document.role_id", batch)
        self.assertIn("ensure_rendered_document_complete(", batch)
        self.assertIn("effective_document,", batch)
        self.assertNotIn("flat_map(|document| document.placeholders", batch)

    def test_frontend_reports_duplicates_instead_of_claiming_full_success(self) -> None:
        app = text("src/App.tsx")
        support = text("src/lib/templateSetupSupport.ts")

        self.assertIn("previousDocumentIds", app)
        self.assertIn("templateSetupCompletionMessage(confirmedRows.length, createdCount)", app)
        self.assertIn("Повторяющихся шаблонов пропущено", support)
        self.assertIn("Новых кнопок не создано", support)


if __name__ == "__main__":
    unittest.main()
