from __future__ import annotations

import json
import unittest
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return project_text(path)


class PerDocumentPrintingContractTest(unittest.TestCase):
    def test_backend_accepts_typed_jobs_and_enforces_copy_limit(self) -> None:
        backend = text("src-tauri/src/main.rs")
        self.assertIn("struct PrintJobRequest", backend)
        self.assertIn("copies: u16", backend)
        self.assertIn("const MAX_PRINT_COPIES: u16 = 99", backend)
        self.assertIn("fn print_resolved_jobs", backend)
        self.assertIn("queued_copies", backend)
        self.assertIn("requested_copies", backend)
        self.assertIn("print_copies_by_document", backend)

    def test_frontend_persists_and_sends_copies_per_document(self) -> None:
        app = "\n".join((text("src/App.tsx"), text("src/lib/appSupport.ts")))
        api = text("src/lib/api.ts")
        rail = text("src/components/DocumentRail.tsx")
        self.assertIn("dokkomplekt.print-copies.v1", app)
        self.assertIn("jobsForItems", app)
        self.assertIn("printCopies[item.document_id]", app)
        self.assertIn("print_copies_by_document", api)
        self.assertIn("Количество копий для", rail)
        self.assertIn("min={0}", rail)
        self.assertIn("max={99}", rail)

    def test_generated_output_keeps_document_identity(self) -> None:
        backend = text("src-tauri/src/main.rs")
        types = text("src/lib/types.ts")
        self.assertIn("struct CreatedDocumentOutputDto", backend)
        self.assertIn("created_documents", backend)
        self.assertIn("interface CreatedDocumentOutput", types)
        self.assertIn("document_id", types)


class CursorScannerContractTest(unittest.TestCase):
    def test_source_selection_is_bound_to_semantic_field(self) -> None:
        workspace = text("src/components/Workspace.tsx")
        scanner = text("crates/dokkomplekt-core/src/scanner_engine.rs")
        self.assertIn("selectionStart", workspace)
        self.assertIn("selectionEnd", workspace)
        self.assertIn("Назначить выделение полю", workspace)
        self.assertIn("ValueSource::Scanner", scanner)
        self.assertIn("user_value_wins_over_scanner_mark", scanner)

    def test_template_selection_supports_replace_and_insert_after(self) -> None:
        modal = text("src/components/TemplateSetupModal.tsx")
        app = text("src/App.tsx")
        docx = text("crates/dokkomplekt-docx/src/lib.rs")
        self.assertIn("applyVisualMarkup", modal)
        self.assertIn("applyPendingVisualMarkup", modal)
        self.assertIn("Необязательно: настроить автоматическое заполнение", modal)
        self.assertIn("Ручная разметка", modal)
        self.assertIn("onMarkupPendingTemplate", modal)
        self.assertIn("applyTemplateMarkup", app)
        self.assertIn("insert_after", modal)
        self.assertIn("enum TemplateMarkupAction", docx)
        self.assertIn("InsertAfter", docx)


class MedicalProfileParityContractTest(unittest.TestCase):
    def test_medical_donor_fields_are_profile_scoped(self) -> None:
        registry = text("crates/dokkomplekt-core/src/field_registry.rs")
        for field_id in [
            "medical.protocol_number",
            "medical.commission_number",
            "medical.rvk_act_number",
            "medical.sick_leave_from",
            "medical.discharge_condition",
        ]:
            self.assertIn(field_id, registry)
        neutral_core = ROOT / "crates/dokkomplekt-core/src/core"
        for source in neutral_core.rglob("*.rs"):
            self.assertNotIn("medical.protocol_number", source.read_text(encoding="utf-8"))

    def test_missing_medical_roles_are_canonicalized(self) -> None:
        medical = text("crates/dokkomplekt-core/src/domains/medical.rs")
        pipeline = text("crates/dokkomplekt-core/src/universal_pipeline.rs")
        self.assertIn('"sick_leave_vk"', medical)
        self.assertIn('"reception"', medical)
        self.assertIn('"sick_leave_vk" => vec![', pipeline)
        self.assertIn('"reception" => vec![', pipeline)


class CriticalSecurityRegressionContractTest(unittest.TestCase):
    def test_untrusted_templates_archives_and_watcher_fail_closed(self) -> None:
        intake = "\n".join((
            text("src-tauri/src/universal_intake.rs"),
            text("src-tauri/src/universal_intake/archive.rs"),
            text("src-tauri/src/universal_intake/web.rs"),
        ))
        watcher = text("src-tauri/src/subsystems/watcher_commands.rs")
        automation = text("src-tauri/src/subsystems/automation_runtime.rs")
        docx = text("crates/dokkomplekt-docx/src/lib.rs")
        for invariant in [
            "resolve_to_addrs(&validated.host, &validated.addresses)",
            ".take(MAX_UPLOAD_BYTES as u64 + 1)",
            "validate_archive_relative_path",
            "archive_entry_is_symlink",
            "preflight_external_archive(path)?",
            "extract_external_archive_entry_bounded",
            "walk_files_bounded",
        ]:
            self.assertIn(invariant, intake)
        self.assertIn("validate_safe_template_bytes(&bytes)", automation)
        self.assertIn("UnsafeActiveContent", docx)
        self.assertIn("validate_safe_template_file(template_path)?", docx)
        self.assertIn("worker_panic; retry_blocked=true", watcher)
        self.assertIn("atomic_write_file", watcher)


class VersionContractTest(unittest.TestCase):
    def test_version_is_18_0_3_everywhere_primary(self) -> None:
        expected = text("VERSION").strip()
        self.assertEqual(expected, "18.4.3")
        self.assertEqual(json.loads(text("package.json"))["version"], expected)
        self.assertEqual(json.loads(text("src-tauri/tauri.conf.json"))["version"], expected)


if __name__ == "__main__":
    unittest.main()
