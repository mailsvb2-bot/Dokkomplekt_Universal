from __future__ import annotations

from pathlib import Path
import unittest

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return project_text(path)


class V1810ProductionPilotContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.main = text("src-tauri/src/main.rs")
        cls.storage = text("crates/dokkomplekt-storage/src/lib.rs")
        cls.semantic = text("crates/dokkomplekt-core/src/semantic_llm.rs")
        cls.template = text("crates/dokkomplekt-core/src/template_engine.rs")
        cls.intake = text("src-tauri/src/universal_intake.rs")
        cls.api = text("src/lib/api.ts")
        cls.workspace = text("src/components/Workspace.tsx")

    def test_local_semantic_transport_is_loopback_only_and_evidence_bound(self) -> None:
        transport = text("src-tauri/src/semantic_model.rs")
        self.assertIn("is_loopback_host", transport)
        self.assertIn(".no_proxy()", transport)
        self.assertIn("Policy::none()", transport)
        self.assertIn("parse_model_extraction_with_source", self.semantic)
        self.assertIn("excerpt_is_present", self.semantic)
        self.assertIn("normalize_grounding_text", self.semantic)
        self.assertIn("value_is_supported_by_excerpt", self.semantic)
        self.assertIn("apply_model_consensus_with_source", self.semantic)
        self.assertIn("ValueSource::Model", self.semantic)
        self.assertIn("model_evidence_must_exist_in_normalized_source", self.semantic)

    def test_case_state_machine_recovers_interrupted_attempts(self) -> None:
        state_sources = self.main + "\n" + self.storage
        for state in ["received", "normalizing", "recognizing", "checking", "generating", "completed"]:
            self.assertIn(f'"{state}"', state_sources)
        self.assertIn("recover_interrupted_case_runs", self.storage)
        self.assertIn("recover_interrupted_case_runs", self.main)
        self.assertIn("retry_case_run", self.api)

    def test_template_versions_are_encrypted_and_rollback_publishes_new_version(self) -> None:
        self.assertIn("CREATE TABLE IF NOT EXISTS template_versions", self.storage)
        self.assertIn("encode_sensitive(template_path)", self.storage)
        self.assertIn("register_template_version", self.main)
        self.assertIn("rollback_template_version", self.main)
        self.assertIn("Rollback к версии", self.main)
        self.assertIn("archive_template_version_source", self.main)
        self.assertIn("template-versions", self.main)

    def test_sidecars_are_hash_verified_and_runtime_discoverable(self) -> None:
        staging = text("scripts/prepare_sidecars.py")
        self.assertIn("sha256_file", staging)
        self.assertIn("network_used", staging)
        self.assertNotIn("requests.get", staging)
        self.assertNotIn('resources/tools', text("src-tauri/tauri.conf.json"))
        self.assertIn('resources/tools', text("src-tauri/tauri.offline.conf.json"))
        self.assertIn("DOKKOMPLEKT_TOOLS_DIR", self.intake)
        for tool in ["tesseract", "pdftotext", "pdftoppm", "soffice"]:
            self.assertIn(tool, self.intake)

    def test_real_placeholder_parser_supports_escaping_and_images(self) -> None:
        self.assertIn("ESCAPED_OPEN_SENTINEL", self.template)
        self.assertIn("Node::Image", self.template)
        self.assertIn("template_image_requests", self.template)
        self.assertIn("DOKKOMPLEKT_IMAGE", self.template)
        self.assertIn("inject_word_image_assets", self.main)

    def test_hash_dedup_xlsx_printing_and_kedo_are_wired(self) -> None:
        self.assertIn("NOTE_SOURCE_SHA256_PREFIX", self.main)
        self.assertIn("note_matches_source_content", self.main)
        self.assertIn("prepare_mail_merge_file", self.main)
        self.assertIn("xlsx", (self.main + "\n" + self.intake).lower())
        self.assertIn("printer_name", self.main)
        self.assertIn("duplex_mode", self.main)
        self.assertIn("create_kedo_package", self.main)
        self.assertIn("kedo-manifest.xml", self.main)
        self.assertIn("SIGNATURES_REQUIRED.json", self.main)
        self.assertIn("КЭДО-пакет", self.workspace)

    def test_version_and_toolchain_are_reproducible(self) -> None:
        self.assertEqual(text("VERSION").strip(), "18.4.3")
        toolchain = text("rust-toolchain.toml")
        self.assertIn('channel = "1.97.1"', toolchain)
        self.assertIn('"rustfmt"', toolchain)
        self.assertIn('"clippy"', toolchain)


if __name__ == "__main__":
    unittest.main()
