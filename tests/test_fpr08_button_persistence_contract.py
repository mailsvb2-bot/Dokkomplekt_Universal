from __future__ import annotations

import unittest

from source_helpers import project_text


class Fpr08ButtonPersistenceContracts(unittest.TestCase):
    def test_native_selection_state_is_loaded_and_persisted(self) -> None:
        commands = project_text("src-tauri/src/subsystems/document_commands.rs")
        backend = project_text("src-tauri/src/subsystems/document_selection.rs")
        main = project_text("src-tauri/src/main.rs")
        api = project_text("src/lib/api.ts")
        bootstrap = project_text("src/hooks/useWorkspaceBootstrap.ts")
        app = project_text("src/App.tsx")
        selection_hook = project_text("src/hooks/useDocumentSelectionPersistence.ts")

        self.assertIn('DOCUMENT_SELECTION_STATE_KEY: &str = "document_selection_v1"', backend)
        self.assertIn("selected_document_ids: Vec<String>", backend)
        self.assertIn('include!("document_selection.rs")', commands)
        self.assertIn("fn set_document_selection(", backend)
        self.assertIn(".save_state_value(DOCUMENT_SELECTION_STATE_KEY, &normalized)", backend)
        self.assertIn("set_document_selection,", main)
        self.assertIn("setDocumentSelection(documentIds: string[])", api)
        self.assertIn("res.selected_document_ids", bootstrap)
        self.assertIn("useDocumentSelectionPersistence", app)
        self.assertIn("persistenceChain", selection_hook)
        self.assertIn("setDocumentSelection(requested)", selection_hook)

    def test_post_restart_source_routing_uses_idempotent_user_action(self) -> None:
        smoke = project_text("tests/installer/windows_installer_contract.ps1")
        app = project_text("src/App.tsx")
        rail = project_text("src/components/DocumentRail.tsx")
        segment = smoke[
            smoke.index("# Source routing may propose a different bundle after restart."):
            smoke.index("Invoke-UiElementPhysically -Element $restartGenerationAction")
        ]
        self.assertIn("FPR-08 open restored document button after source routing", segment)
        self.assertIn("Find-ReadyButtonByNames -Root $currentAppWindow -Names @($expectedTemplateButtonName)", segment)
        self.assertIn("FPR-08 one-document generation after explicit restore", segment)
        self.assertNotIn("FPR-08 clear stale selection", segment)
        self.assertNotIn("Invoke-UiElementPhysically -Element $restartCheckbox", segment)
        self.assertIn("previous.includes(doc.id) ? previous : [...previous, doc.id]", app)
        self.assertIn("onSelect={selectDocument}", app)
        self.assertIn("onClick={() => props.onSelect(document)}", rail)
        self.assertIn("FPR-08 INSTALLED PASS: selection + button name + published template binding survived restart -> physical DOCX", smoke)

    def test_installed_restart_proof_covers_selection_name_and_binding(self) -> None:
        smoke = project_text("tests/installer/windows_installer_contract.ps1")
        self.assertIn("FPR-08 durable selection PASS", smoke)
        self.assertIn("FPR-08 selection restart PASS", smoke)
        self.assertIn("FPR-08 INSTALLED PASS: selection + button name + published template binding survived restart -> physical DOCX", smoke)
        self.assertIn("document_selection_v1", smoke)
        self.assertIn("Выписной эпикриз", smoke)
        self.assertIn("Петров Пётр Петрович", smoke)


if __name__ == "__main__":
    unittest.main()
