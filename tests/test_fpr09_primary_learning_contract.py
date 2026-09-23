from __future__ import annotations

import json
import unittest

from source_helpers import project_text


class Fpr09PrimaryLearningContracts(unittest.TestCase):
    def test_primary_create_buttons_surface_owns_placeholder_free_learning(self) -> None:
        modal = project_text("src/components/TemplateSetupModal.tsx")
        rail = project_text("src/components/DocumentRail.tsx")
        handlers = project_text("src/lib/pendingTemplateIntelligence.ts")
        app = project_text("src/App.tsx")
        learning_commands = project_text("src-tauri/src/subsystems/template_learning_commands.rs")
        picker = project_text("src-tauri/src/subsystems/template_picker.rs")

        self.assertIn(">Создать свои кнопки</button>", rail)
        self.assertIn(">Добавить шаблоны</button>", rail)
        self.assertGreaterEqual(rail.count("onClick={props.onAdd}"), 2)
        self.assertIn("<h2>Создать свои кнопки</h2>", modal)
        self.assertIn("1. Источники (4–10)", modal)
        self.assertIn("2. Правильные результаты (4–10)", modal)
        self.assertIn("Последняя пара резервируется как независимая контрольная", modal)
        self.assertIn("onPickLearningFiles={pickPendingLearningFiles}", app)
        self.assertIn("pickLearningFiles(kind)", app)
        self.assertIn("onLearnPendingTemplate={learnPendingTemplateFromExamples}", app)
        self.assertIn("pair.source.staged_path", handlers)
        self.assertIn("pair.completed.staged_path", handlers)
        self.assertNotIn("readFileBytes(pair.source)", handlers)
        self.assertIn('"source" => pick_source_files_blocking(req.initial_path)', learning_commands)
        self.assertIn("fn pick_source_files_blocking", picker)
        self.assertIn("$dialog.Multiselect = $true", picker)
        self.assertIn("learnTemplateFromExamples({", handlers)
        self.assertIn("hasPublishableLearningProof(learned)", handlers)
        self.assertIn("Применить карту, подтверждённую примерами?", handlers)
        self.assertIn("applyTemplateLearningMap(", handlers)
        self.assertIn("learning_validation_id: learned.validation_id", handlers)

    def test_installed_primary_learning_proof_is_held_out_and_restart_bound(self) -> None:
        smoke = project_text("tests/installer/windows_installer_contract.ps1")

        self.assertIn("FPR-09 primary learning PASS", smoke)
        self.assertIn("FPR-09 INSTALLED PASS", smoke)
        self.assertIn("FPR-09 обученная кнопка", smoke)
        self.assertIn("fpr09-blank.docx", smoke)
        self.assertIn("New-E2LearningDocxFixture -Path $fpr09Blank -Inn '' -Blank", smoke)
        self.assertIn("Применить подтверждённую карту", smoke)
        self.assertIn("$fpr09Sources[3]", smoke)
        self.assertIn("$fpr09Subjects[3]", smoke)
        self.assertIn("-SubjectSlot", smoke)
        self.assertIn("FPR-09 learned button after restart", smoke)
        self.assertIn("FPR-09 held-out Source INN did not reach the learned output.", smoke)
        self.assertIn("FPR-09 held-out Source subject did not reach the learned output.", smoke)
        self.assertIn("FPR-09 learned output retained the blank training zone.", smoke)

    def test_feature_register_closes_fpr09_with_installed_evidence(self) -> None:
        register = json.loads(project_text("docs/CANON_FEATURE_PRESERVATION_REGISTER.json"))
        feature = next(item for item in register["features"] if item["id"] == "FPR-09")

        self.assertEqual(feature["status"], "verified")
        self.assertEqual(feature["runtime_gap"], "")
        self.assertTrue(any("windows_installer_contract.ps1#FPR-09" in item for item in feature["runtime_evidence"]))


if __name__ == "__main__":
    unittest.main()
