from __future__ import annotations

import json
import unittest
import zipfile
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


class ProfessionalPopupContracts(unittest.TestCase):
    def read(self, relative: str) -> str:
        return project_text(relative)

    def test_one_merged_preflight_exists_for_whole_selected_set(self) -> None:
        workflow = self.read("crates/dokkomplekt-core/src/workflow_engine.rs")
        main = self.read("src-tauri/src/main.rs")
        app = self.read("src/App.tsx")
        preflight = self.read("src/hooks/useGenerationPreflight.ts")
        workspace = self.read("src/components/Workspace.tsx")
        self.assertIn("pub fn plan_workflow_batch", workflow)
        self.assertIn("fn get_workflow_plan_batch", main)
        self.assertIn("fn apply_popup_batch", main)
        self.assertIn("useGenerationPreflight({", app)
        self.assertIn("applyPopupBatch(ids, sickLeave, payload)", app)
        self.assertIn("requestWorkflowPlan(options.selectedDocumentIds)", preflight)
        self.assertIn("Проверить и создать", workspace)

    def test_invalid_required_value_keeps_preflight_visible(self) -> None:
        popup = self.read("crates/dokkomplekt-core/src/popup_engine.rs")
        preflight = self.read("src/hooks/useGenerationPreflight.ts")
        workspace = self.read("src/components/Workspace.tsx")
        self.assertIn("still_missing", popup)
        self.assertIn("if (!applied.accepted)", preflight)
        self.assertIn("Не заполнено обязательное поле", preflight)
        self.assertLess(
            preflight.index("if (!applied.accepted)"),
            preflight.index("setGenerationPreflightOpen(false)"),
        )
        self.assertIn("clientFields", workspace)

    def test_specialist_can_design_popup_without_editing_rust(self) -> None:
        editor = self.read("src/components/PopupFieldEditor.tsx")
        main = self.read("src-tauri/src/main.rs")
        self.assertIn("+ Добавить вопрос", editor)
        self.assertIn("Когда спрашивать", editor)
        self.assertIn("Тип ответа", editor)
        self.assertIn("fn update_document_popup_fields", main)

    def test_mouse_scanner_creates_same_semantic_field_for_popup(self) -> None:
        setup = self.read("src/components/TemplateSetupModal.tsx")
        self.assertIn("ensurePopupField", setup)
        self.assertIn("Выделите", setup)

    def test_profession_specific_defaults_cover_multiple_domains(self) -> None:
        profiles = self.read("crates/dokkomplekt-core/src/popup_profiles.rs")
        for marker in [
            "DomainKind::Medical",
            "DomainKind::Legal",
            "DomainKind::Hr",
            "DomainKind::Accounting",
            "DomainKind::Education",
            'add("contract.number", true)',
            'add("hr.order_number", true)',
            'add("accounting.invoice_number", true)',
            'add("education.student_name", true)',
        ]:
            self.assertIn(marker, profiles)

    def test_medical_donor_popup_fields_are_profile_scoped(self) -> None:
        profiles = self.read("crates/dokkomplekt-core/src/popup_profiles.rs")
        plan = self.read("crates/dokkomplekt-core/src/domains/medical_document_plan.rs")
        semantics = self.read("crates/dokkomplekt-core/src/domains/medical_semantics.rs")
        self.assertIn("build_medical_render_plan(", profiles)
        self.assertIn("for field_id in &plan.required_fields", profiles)
        for marker in [
            '"medical.case_number"',
            '"medical.discharge_date"',
            '"medical.diagnosis"',
            '"medical.treatment"',
            '"medical.rvk_commissariat"',
            '"medical.sick_leave_number"',
        ]:
            self.assertIn(marker, plan)
        for scoped in [
            '"medical.vk_mse.protocol_number"',
            '"medical.vk_mse.protocol_date"',
            '"medical.sick_leave_vk.protocol_number"',
            '"medical.sick_leave_vk.protocol_date"',
        ]:
            self.assertIn(scoped, semantics)
        neutral_core = "\n".join(
            path.read_text(encoding="utf-8").lower()
            for path in (ROOT / "crates/dokkomplekt-core/src/core").glob("*.rs")
        )
        self.assertNotIn("medical.", neutral_core)

    def test_linked_dates_copy_until_specialist_edits_them(self) -> None:
        profiles = self.read("crates/dokkomplekt-core/src/popup_profiles.rs")
        workspace = self.read("src/components/Workspace.tsx")
        editor = self.read("src/components/PopupFieldEditor.tsx")
        self.assertIn("VK_MSE_PROTOCOL_DATE => Some(VK_MSE_COMMISSION_DATE)", profiles)
        self.assertIn(
            "SICK_LEAVE_VK_PROTOCOL_DATE => Some(SICK_LEAVE_VK_COMMISSION_DATE)",
            profiles,
        )
        self.assertIn("linkedPrompt.linked_to !== prompt.field_id", workspace)
        self.assertIn("Повторять значение поля", editor)

    def test_popup_values_have_final_user_confirmed_priority(self) -> None:
        popup = self.read("crates/dokkomplekt-core/src/popup_engine.rs")
        self.assertIn("ValueSource::UserConfirmed", popup)
        main = self.read("src-tauri/src/main.rs")
        self.assertIn("snapshot.semantic_case = result.semantic_case.clone()", main)
        self.assertIn("transact_default_state(&app, &state", main)

    def test_dates_are_normalized_in_popup(self) -> None:
        popup = self.read("crates/dokkomplekt-core/src/popup_engine.rs")
        self.assertIn("parse_flexible_date", popup)
        self.assertIn("normalize_prompt_value", popup)

    def test_explicit_configuration_can_remove_optional_default_questions(self) -> None:
        types = self.read("crates/dokkomplekt-core/src/types.rs")
        profiles = self.read("crates/dokkomplekt-core/src/popup_profiles.rs")
        self.assertIn("pub popup_configured: bool", types)
        self.assertIn("if document.popup_configured", profiles)
        self.assertIn("Fail closed", profiles)


    def test_new_source_starts_a_fresh_case_without_previous_person_leakage(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        app = self.read("src/App.tsx")
        self.assertIn("fn replace_case_from_new_source", main)
        self.assertGreaterEqual(
            main.count("replace_case_from_new_source(&mut snapshot.semantic_case, parsed)"),
            2,
        )
        self.assertIn("transact_default_state(&app, &state", main)
        self.assertIn("fn reset_case", main)
        self.assertIn("Новый комплект", app)

    def test_bundled_templates_are_only_explicit_draft_starters(self) -> None:
        allowed_roots = ("content-packs/", "public/starter-packs/")
        found = []
        ignored_roots = {
            ".git",
            ".venv",
            "dist",
            "node_modules",
            "target",
            "verification",
        }
        for path in ROOT.rglob("*"):
            relative_parts = path.relative_to(ROOT).parts
            if any(part in ignored_roots or part.startswith(".") for part in relative_parts):
                continue
            if path.suffix.lower() not in {".docx", ".docm"} or "tests" in path.parts:
                continue
            relative = path.relative_to(ROOT).as_posix()
            self.assertTrue(relative.startswith(allowed_roots), relative)
            with zipfile.ZipFile(path) as archive:
                visible_xml = " ".join(
                    archive.read(name).decode("utf-8", errors="ignore")
                    for name in archive.namelist()
                    if name.startswith("word/") and name.endswith(".xml")
                )
            self.assertIn("STARTER-КАРКАС", visible_xml, relative)
            self.assertIn("НЕ ЯВЛЯЕТСЯ УТВЕРЖДЁННОЙ", visible_xml, relative)
            found.append(relative)
        self.assertGreaterEqual(len(found), 11)
        for manifest_path in (ROOT / "content-packs").glob("tier1-*/pack.json"):
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(manifest["status"], "starter")
            self.assertEqual(manifest["usage_mode"], "draft_only")
            self.assertTrue(manifest["requires_organization_review"])


if __name__ == "__main__":
    unittest.main()
