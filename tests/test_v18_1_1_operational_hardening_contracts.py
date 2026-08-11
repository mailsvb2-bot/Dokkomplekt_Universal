import json
import unittest
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


class OperationalHardeningContracts(unittest.TestCase):
    def read(self, relative: str) -> str:
        return project_text(relative)

    def test_processed_sources_are_archived_and_legacy_markers_are_migrated(self) -> None:
        hygiene = self.read("src-tauri/src/workspace_hygiene.rs")
        self.assertIn('archive_folder_name: "_обработано".into()', hygiene)
        self.assertIn("pub fn processed_marker_candidates", hygiene)
        self.assertIn("cleanup_migrates_legacy_processed_source_into_archive", hygiene)
        self.assertIn("archived_processed_sources", hygiene)

    def test_processed_marker_identity_includes_full_source_name(self) -> None:
        hygiene = self.read("src-tauri/src/workspace_hygiene.rs")
        self.assertIn(".file_name()", hygiene)
        self.assertIn('format!("{name}{PROCESSED_SUFFIX}")', hygiene)

    def test_retained_sources_use_encrypted_case_history_without_adjacent_markers(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        storage = self.read("crates/dokkomplekt-storage/src/lib.rs")
        self.assertIn("completed_case_exists_for_source_hash", storage)
        self.assertIn("source_retained_and_tracked_in_case_history", main)
        self.assertIn("Publication is the business terminal point", main)
        self.assertNotIn("status=published_source_retained_by_privacy_policy", main)

    def test_network_processing_lock_uses_content_addressed_shared_lease(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        self.assertIn(".dokkomplekt-queue", main)
        self.assertIn("claims", main)
        self.assertIn("completed", main)
        self.assertIn("processing_lock_host_id", main)
        self.assertIn("heartbeat_stop", main)
        self.assertIn("REMOTE_LEASE_TIMEOUT", main)
        self.assertIn("std::fs::create_dir(&marker)", main)
        self.assertIn("source_sha256", main)

    def test_confirm_all_retry_bypasses_only_short_event_dedup(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        runtime = self.read("src-tauri/src/subsystems/automation_runtime.rs")
        self.assertIn("confirm_risk_exception_and_retry", main)
        self.assertIn(
            "if req.confirmed_fields.is_empty() && req.confirmed_document_ids.is_empty() && !req.force_reissue",
            runtime,
        )
        self.assertIn("atomic processing lock remains in force", runtime)

    def test_layout_fingerprint_scopes_learned_rules(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        self.assertIn("source_layout_fingerprint", main)
        self.assertIn("rule.layout_fingerprint.is_some() && !exact_layout", main)
        self.assertIn("0.999", main)

    def test_pdf_printing_has_deterministic_windows_sidecar(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        intake = self.read("src-tauri/src/universal_intake.rs")
        manifest = json.loads(self.read("sidecars/sidecar-manifest.example.json"))
        self.assertIn("print_pdf_with_sumatra", main)
        for token in ["-print-to", "-print-to-default", "-print-settings", "duplexlong", "bin={tray}"]:
            self.assertIn(token, main)
        self.assertIn("sumatrapdf", intake)
        self.assertTrue(any(item["tool"] == "sumatrapdf" for item in manifest["files"]))

    def test_workflow_has_one_canonical_core_contract(self) -> None:
        self.assertTrue((ROOT / "crates/dokkomplekt-core/src/core/workflow_contract.rs").is_file())
        self.assertFalse((ROOT / "crates/dokkomplekt-core/src/core/workflow_engine.rs").exists())
        pipeline = self.read("crates/dokkomplekt-core/src/universal_pipeline.rs")
        self.assertIn("workflow_contract::build_workflow", pipeline)

    def test_full_offline_installer_requires_verified_runtime_and_model(self) -> None:
        prepare = self.read("scripts/prepare_sidecars.py")
        verifier = self.read("scripts/assert_offline_runtime_ready.py")
        installer = self.read("BUILD_WINDOWS_INSTALLER.bat")
        workflow = self.read(".github/workflows/build-installers.yml")
        for tool in ["sumatrapdf", "llama_cpp", "semantic_model"]:
            self.assertIn(f'"{tool}"', prepare)
        for required in [
            "tessdata/rus.traineddata",
            "tessdata/eng.traineddata",
            "pdftotext.exe",
            "pdftoppm.exe",
            "soffice.exe",
            "sumatrapdf.exe",
            ".gguf",
        ]:
            self.assertIn(required, verifier.lower())
        self.assertIn("DOKKOMPLEKT_SIDECAR_MANIFEST", installer)
        self.assertIn("--require-semantic-model", installer)
        self.assertIn("Verify production runtime staged from immutable approved bundle", workflow)
        self.assertIn("stage_signed_runtime_bundle.py", workflow)

    def test_completed_case_can_be_reissued_without_deleting_previous_output(self) -> None:
        main = self.read("src-tauri/src/main.rs")
        ui = self.read("src/components/AutomationControlCenter.tsx")
        storage = self.read("crates/dokkomplekt-storage/src/lib.rs")
        self.assertIn("force_reissue", main)
        self.assertIn("preserve_source_after_success", main)
        self.assertIn("case_reissue_requested", main)
        self.assertIn("update_case_run_source_path", storage)
        self.assertIn("Переиздать", ui)

    def test_reference_calendar_deadline_is_release_enforced(self) -> None:
        freshness = self.read("scripts/check_reference_data_freshness.py")
        for gate in [
            "scripts/prepackage_rust_gate.sh",
            "scripts/prepackage_rust_gate.bat",
            "scripts/run_quality_gate.sh",
            "scripts/run_quality_gate.bat",
        ]:
            self.assertIn("check_reference_data_freshness.py", self.read(gate))
        self.assertIn("today.month >= 10", freshness)


if __name__ == "__main__":
    unittest.main()
