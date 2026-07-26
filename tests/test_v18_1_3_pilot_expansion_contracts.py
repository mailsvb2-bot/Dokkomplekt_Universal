from __future__ import annotations

import hashlib
import json
import unittest
import zipfile
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


class PilotExpansionContracts(unittest.TestCase):
    def text(self, relative: str) -> str:
        return project_text(relative)

    def test_version_is_synchronized(self) -> None:
        self.assertEqual(self.text("VERSION").strip(), "18.4.3")
        self.assertEqual(json.loads(self.text("package.json"))["version"], "18.4.3")
        self.assertEqual(json.loads(self.text("src-tauri/tauri.conf.json"))["version"], "18.4.3")

    def test_starter_packs_have_real_hash_verified_draft_templates(self) -> None:
        total = 0
        for pack_path in sorted((ROOT / "content-packs").glob("tier1-*/pack.json")):
            pack = json.loads(pack_path.read_text(encoding="utf-8"))
            self.assertEqual(pack["status"], "starter")
            self.assertEqual(pack["usage_mode"], "draft_only")
            self.assertTrue(pack["requires_organization_review"])
            self.assertGreaterEqual(len(pack["template_slots"]), 3)
            for slot in pack["template_slots"]:
                source = pack_path.parent / slot["template_path"]
                public = ROOT / "public" / "starter-packs" / pack_path.parent.name / slot["template_path"]
                self.assertTrue(source.is_file(), source)
                self.assertTrue(public.is_file(), public)
                self.assertEqual(source.read_bytes(), public.read_bytes())
                self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), slot["sha256"])
                with zipfile.ZipFile(source) as archive:
                    visible = " ".join(
                        archive.read(name).decode("utf-8", errors="ignore")
                        for name in archive.namelist()
                        if name.startswith("word/") and name.endswith(".xml")
                    )
                self.assertIn("STARTER-КАРКАС", visible)
                total += 1
        self.assertEqual(total, 11)

    def test_starter_pack_ui_verifies_hash_and_uses_existing_rust_setup_route(self) -> None:
        panel = self.text("src/components/AdvancedToolsPanel.tsx")
        self.assertIn("sha256Hex(bytes)", panel)
        self.assertIn("prepareTemplateSetup(candidates)", panel)
        self.assertIn("confirmTemplateSetup", panel)
        self.assertIn("draft_only", panel)
        fingerprint = self.text("scripts/source_fingerprint.py")
        self.assertIn('ROOT / "public" / "starter-packs"', fingerprint)
        data = self.text("src/data/starterPacks.ts")
        for pack_id in ["tier1.hr.ru", "tier1.legal.ru", "tier1.accounting.ru"]:
            self.assertIn(pack_id, data)

    def test_nonmedical_domains_are_real_profiles_not_three_line_stubs(self) -> None:
        expected = {
            "crates/dokkomplekt-core/src/domains/hr.rs": ["employee.hire_date", "employment_order"],
            "crates/dokkomplekt-core/src/domains/legal.rs": ["contract.subject", "legal.claim_subject"],
            "crates/dokkomplekt-core/src/domains/accounting.rs": ["amount.total", "reconciliation"],
            "crates/dokkomplekt-core/src/domains/education.rs": ["education.institution", "grade_report"],
        }
        for relative, tokens in expected.items():
            source = self.text(relative)
            self.assertGreater(len(source.splitlines()), 35, relative)
            for token in tokens:
                self.assertIn(token, source, relative)
        pipeline = self.text("crates/dokkomplekt-core/src/universal_pipeline.rs")
        self.assertIn("nonmedical_role_fields", pipeline)
        self.assertIn("UniversalDomain::Accounting", pipeline)

    def test_onboarding_has_dry_run_and_organization_review_warning(self) -> None:
        panel = self.text("src/components/AdvancedToolsPanel.tsx")
        self.assertIn("Мастер первого запуска: 3 шага", panel)
        self.assertIn("Сухой прогон без печати", panel)
        self.assertIn("обязательная проверка организацией", panel)
        self.assertIn("не являются утверждёнными", panel)

    def test_pilot_security_and_kedo_boundaries_are_documented(self) -> None:
        required = [
            "docs/pilot-security-pack/README.md",
            "docs/pilot-security-pack/01_DATA_FLOW_REGISTER.md",
            "docs/pilot-security-pack/02_ROLE_MATRIX.csv",
            "docs/pilot-security-pack/03_THREAT_MODEL_WORKSHEET.md",
            "docs/pilot-security-pack/04_RETENTION_AND_DELETION.md",
            "docs/pilot-security-pack/05_INCIDENT_RESPONSE.md",
            "docs/pilot-security-pack/06_PILOT_ACCEPTANCE_CHECKLIST.md",
            "docs/KEDO_AND_SIGNATURE_INTEGRATION.md",
        ]
        for relative in required:
            self.assertTrue((ROOT / relative).is_file(), relative)
        security = self.text("docs/pilot-security-pack/README.md")
        self.assertIn("не является сертификатом", security.lower())
        kedo = self.text("docs/KEDO_AND_SIGNATURE_INTEGRATION.md")
        self.assertIn("не создаёт юридически значимую подпись", kedo.lower())

    def test_signed_calendar_feed_is_automatic_and_fail_closed(self) -> None:
        update = self.text("src-tauri/src/reference_data_update.rs")
        main = self.text("src-tauri/src/main.rs")
        self.assertIn("Ed25519", update)
        self.assertIn("resolve_to_addrs", update)
        self.assertIn("automatic_feed_configured", main)
        self.assertIn("download_and_install", main)
        self.assertIn("REFERENCE DATA BLOCKED", self.text("scripts/check_reference_data_freshness.py"))


if __name__ == "__main__":
    unittest.main()
