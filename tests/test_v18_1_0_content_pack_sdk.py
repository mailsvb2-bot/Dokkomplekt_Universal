from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "validate_content_pack", ROOT / "scripts" / "validate_content_pack.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class V1810ContentPackSdkContracts(unittest.TestCase):
    def test_tier1_starter_packs_have_hash_verified_draft_templates(self) -> None:
        for folder in ["tier1-hr-ru", "tier1-legal-ru", "tier1-accounting-ru"]:
            pack_root = ROOT / "content-packs" / folder
            result = MODULE.validate(pack_root)
            self.assertEqual(result["status"], "starter")
            self.assertEqual(result["verified_templates"], result["slots"])
            manifest = json.loads((pack_root / "pack.json").read_text("utf-8"))
            self.assertEqual(manifest["usage_mode"], "draft_only")
            self.assertIs(manifest["requires_organization_review"], True)
            for slot in manifest["template_slots"]:
                template = pack_root / slot["template_path"]
                actual_fields = MODULE.extract_docx_field_references(template)
                self.assertEqual(set(slot["referenced_fields"]), actual_fields)
                self.assertEqual(set(slot["required_fields"]), actual_fields)
                with zipfile.ZipFile(template) as archive:
                    header = archive.read("word/header1.xml").decode("utf-8")
                self.assertIn("STARTER-КАРКАС", header)
                self.assertIn("НЕ ЯВЛЯЕТСЯ УТВЕРЖДЁННОЙ", header)

    def test_starter_pack_cannot_hide_missing_templates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = json.loads(
                (ROOT / "content-packs" / "tier1-hr-ru" / "pack.json").read_text("utf-8")
            )
            for slot in source["template_slots"]:
                slot["template_path"] = None
                slot["sha256"] = None
            (root / "pack.json").write_text(json.dumps(source, ensure_ascii=False), "utf-8")
            with self.assertRaisesRegex(ValueError, "requires every template slot"):
                MODULE.validate(root)


    def test_manifest_cannot_claim_fields_different_from_docx(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = ROOT / "content-packs" / "tier1-legal-ru"
            source = json.loads((source_root / "pack.json").read_text("utf-8"))
            for slot in source["template_slots"]:
                src = source_root / slot["template_path"]
                dst = root / slot["template_path"]
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(src.read_bytes())
            source["template_slots"][0]["referenced_fields"] = ["document.number"]
            source["template_slots"][0]["required_fields"] = ["document.number"]
            (root / "pack.json").write_text(json.dumps(source, ensure_ascii=False), "utf-8")
            with self.assertRaisesRegex(ValueError, "referenced_fields mismatch"):
                MODULE.validate(root)

    def test_approved_pack_requires_named_reviewer(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = ROOT / "content-packs" / "tier1-accounting-ru"
            source = json.loads((source_root / "pack.json").read_text("utf-8"))
            source["status"] = "approved"
            source["usage_mode"] = "production"
            source["publisher"]["reviewed_by"] = []
            for slot in source["template_slots"]:
                src = source_root / slot["template_path"]
                dst = root / slot["template_path"]
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(src.read_bytes())
            (root / "pack.json").write_text(json.dumps(source, ensure_ascii=False), "utf-8")
            with self.assertRaisesRegex(ValueError, "named reviewer"):
                MODULE.validate(root)


if __name__ == "__main__":
    unittest.main()
