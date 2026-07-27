from __future__ import annotations

import json
import unittest
from pathlib import Path

import yaml

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return project_text(relative)


class V1812GroundingDistributedReleaseContracts(unittest.TestCase):
    def test_version_is_synchronized_and_rust_marker_is_not_forged(self) -> None:
        self.assertEqual(read("VERSION").strip(), "18.4.3")
        self.assertEqual(json.loads(read("package.json"))["version"], "18.4.3")
        self.assertEqual(json.loads(read("src-tauri/tauri.conf.json"))["version"], "18.4.3")
        self.assertFalse((ROOT / ".cargo-gate/CARGO_GATE_PASSED.ok").exists())

    def test_missing_core_export_and_pdf_recursion_regressions_are_closed(self) -> None:
        core = read("crates/dokkomplekt-core/src/lib.rs")
        semantic = read("crates/dokkomplekt-core/src/semantic_llm.rs")
        main = read("src-tauri/src/main.rs")
        self.assertIn("apply_model_output_with_source", core)
        self.assertIn("pub fn apply_model_output_with_source", semantic)
        start = main.index("fn pdf_print_settings")
        body = main[start: main.index("\n}\n", start) + 3]
        self.assertNotIn("pdf_print_settings(copies, preferences)", body)
        self.assertIn("ignore-pdf-print-settings", body)

    def test_model_grounding_normalizes_text_and_requires_value_locality(self) -> None:
        semantic = read("crates/dokkomplekt-core/src/semantic_llm.rs")
        for token in [
            "normalize_grounding_text",
            "excerpt_is_present",
            "value_is_supported_by_excerpt",
            "real_excerpt_with_unrelated_value_is_rejected",
            "one_model_pass_cannot_approve_a_high_risk_field",
            "high_risk_consensus_requires_two_equal_grounded_answers",
        ]:
            self.assertIn(token, semantic)
        self.assertIn("if high_risk && count < 2", semantic)
        transport = read("src-tauri/src/semantic_model.rs")
        self.assertIn("if !(2..=3).contains(&self.consistency_passes)", transport)

    def test_watcher_requires_two_byte_identical_reads(self) -> None:
        main = read("src-tauri/src/main.rs")
        self.assertIn("Entry::Vacant(slot)", main)
        self.assertIn("Ok(false)", main)
        self.assertIn("entry.sha256 == sha256", main)
        self.assertIn("entry.identical_observations.saturating_add(1)", main)
        self.assertIn("Ok(entry.identical_observations >= 1)", main)

    def test_shared_folder_queue_is_content_addressed_and_renewable(self) -> None:
        main = read("src-tauri/src/main.rs")
        for token in [
            ".dokkomplekt-queue",
            'join("claims")',
            'join("completed")',
            'format!("{source_sha256}.lock")',
            "heartbeat_stop",
            "REMOTE_LEASE_TIMEOUT",
            "mark_shared_completion",
        ]:
            self.assertIn(token, main)

    def test_calendar_update_is_signed_pinned_and_fail_closed(self) -> None:
        module = read("src-tauri/src/reference_data_update.rs")
        for token in [
            "Ed25519",
            "canonical_json_bytes",
            "https_only(true)",
            ".no_proxy()",
            "Policy::none()",
            "resolve_to_addrs",
            "install_production_calendar_override",
            "automatic_feed_configured",
        ]:
            self.assertIn(token, module)
        self.assertIn("update_reference_data", read("src/lib/api.ts"))

    def test_docx_golden_fixture_and_cross_platform_image_injection_exist(self) -> None:
        docx = read("crates/dokkomplekt-docx/src/lib.rs")
        fixture = ROOT / "tests/fixtures/docx/complex_realistic_template.docx"
        self.assertTrue(fixture.is_file())
        self.assertGreater(fixture.stat().st_size, 500)
        for token in [
            "pub fn inject_docx_images",
            "word/media/",
            "relationships/image",
            "golden_realistic_docx_preserves_parts_and_renders_every_story",
            "injects_images_into_body_and_header_without_word_com",
            "MAX_IMAGE_ASSET_BYTES",
        ]:
            self.assertIn(token, docx)
        self.assertIn("inject_docx_images(document, assets)", read("src-tauri/src/main.rs"))

    def test_local_domain_classifier_is_conservative(self) -> None:
        classifier = read("crates/dokkomplekt-core/src/source_classification.rs")
        self.assertIn("pub fn classify_source_domain", classifier)
        self.assertIn("score < 4", classifier)
        self.assertIn("runner_up_score", classifier)
        self.assertIn("one_ambiguous_word_does_not_route_the_case", classifier)
        self.assertIn("classify_source_domain(text, &case)", read("crates/dokkomplekt-core/src/source_parser.rs"))

    def test_release_fingerprint_covers_workflows_and_runtime_inputs(self) -> None:
        fingerprint = read("scripts/source_fingerprint.py")
        for token in [
            'ROOT / ".github" / "workflows"',
            'ROOT / "resources"',
            'ROOT / "sidecars"',
            'ROOT / "content-packs"',
            'ROOT / "VERSION"',
        ]:
            self.assertIn(token, fingerprint)

    def test_signed_runtime_does_not_trust_artifact_supplied_public_key(self) -> None:
        creator = read("scripts/create_offline_runtime_bundle.py")
        verifier = read("scripts/verify_offline_runtime_bundle.py")
        workflow = read(".github/workflows/build-installers.yml")
        self.assertIn("trusted-public-key", creator)
        self.assertIn("trust-on-first-use", creator)
        self.assertIn("a pinned --trusted-public-key is required", verifier)
        self.assertIn("DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64", workflow)

    def test_windows_pipeline_signs_app_before_nsis_and_has_hardware_gate(self) -> None:
        workflow = read(".github/workflows/build-installers.yml")
        self.assertLess(workflow.index("npx tauri build --no-bundle"), workflow.index("npx tauri bundle --bundles nsis"))
        self.assertIn("target/release/dokkomplekt-tauri.exe", workflow)
        hardware = read("tests/windows/windows_hardware_e2e.ps1")
        self.assertIn("Get-AuthenticodeSignature -FilePath $app", hardware)
        self.assertIn("windows_word_print_hardware_e2e", hardware)
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            parsed = yaml.safe_load(path.read_text("utf-8"))
            self.assertIsInstance(parsed, dict, path.name)

    def test_production_panic_shortcuts_are_source_gated(self) -> None:
        audit = read("scripts/audit_rust_production_panics.py")
        prepackage = read("scripts/prepackage_rust_gate.sh")
        self.assertIn("PRODUCTION RUST PANIC SHORTCUTS FOUND", audit)
        self.assertIn("audit_rust_production_panics.py", prepackage)
        self.assertIn("cargo audit --deny warnings", prepackage)

    def test_semantic_model_shadow_mode_is_non_mutating_and_metered(self) -> None:
        model = read("src-tauri/src/semantic_model.rs")
        main = read("src-tauri/src/main.rs")
        storage = read("crates/dokkomplekt-storage/src/lib.rs")
        ui = read("src/components/AutomationControlCenter.tsx")
        self.assertIn("pub shadow_mode: bool", model)
        self.assertIn("let mut shadow_case = case.clone()", main)
        self.assertIn("semantic_model_shadow_evaluated", main)
        self.assertIn('increment_metric(app, "shadow_model_runs", 1)', main)
        self.assertIn("shadow_model_agreements", storage)
        self.assertIn("Режим наблюдения: сравнивать результаты, но не изменять документы", ui)

    def test_ui_exposes_one_click_correction_and_roi(self) -> None:
        workspace = read("src/components/Workspace.tsx")
        app = read("src/App.tsx")
        control = read("src/components/AutomationControlCenter.tsx")
        self.assertIn("Здесь ошибка", workspace)
        self.assertIn("reportSemanticFieldError", app)
        for token in ["zero_touch_sources", "attention_resolutions", "model_grounding_rejections"]:
            self.assertIn(token, control)


if __name__ == "__main__":
    unittest.main()
