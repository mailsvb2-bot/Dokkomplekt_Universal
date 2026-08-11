from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def text(relative: str) -> str:
    return (ROOT / relative).read_text("utf-8")


class V1821ResumeQueueModularizationContracts(unittest.TestCase):
    def test_main_is_decomposed_into_named_subsystems(self) -> None:
        main_path = ROOT / "src-tauri/src/main.rs"
        main = main_path.read_text("utf-8")
        self.assertLess(len(main.splitlines()), 3000)
        for subsystem in [
            "update_runtime.rs",
            "desktop_io.rs",
            "document_commands.rs",
            "automation_runtime.rs",
        ]:
            self.assertIn(f'include!("subsystems/{subsystem}")', main)
            self.assertTrue((ROOT / "src-tauri/src/subsystems" / subsystem).is_file())

    def test_resume_is_per_document_and_content_addressed(self) -> None:
        storage = text("crates/dokkomplekt-storage/src/lib.rs")
        resume = text("src-tauri/src/resume_engine.rs")
        runtime = text("src-tauri/src/subsystems/automation_runtime.rs")
        for invariant in [
            "CREATE TABLE IF NOT EXISTS case_run_documents",
            "input_fingerprint",
            "reused_from_case_id",
            "upsert_case_document",
            "list_case_documents",
        ]:
            self.assertIn(invariant, storage)
        for invariant in [
            "document_input_fingerprint",
            "dokkomplekt-resume-v2",
            "template_collection_references",
            "template_block_references",
            "asset_sha256",
            "template_is_resume_safe",
            "persist_checkpoint",
            "reusable_checkpoint",
        ]:
            self.assertIn(invariant, resume)
        self.assertIn("resume_from_case_id", runtime)
        self.assertIn("reused_documents", runtime)
        self.assertIn("rerendered_documents", runtime)
        self.assertIn("counter", resume.lower())
        self.assertIn("working_days", resume)
        self.assertIn("watermark", resume)
        self.assertIn("unrelated_semantic_value_does_not_invalidate_document", resume)

    def test_central_queue_replaces_filesystem_advisory_lock_when_configured(self) -> None:
        queue = text("src-tauri/src/central_queue.rs")
        service = text("scripts/queue_mtls_service.py")
        runtime = text("src-tauri/src/subsystems/automation_runtime.rs")
        cargo = text("src-tauri/Cargo.toml")
        for invariant in [
            "DOKKOMPLEKT_QUEUE_MTLS_URL",
            "DOKKOMPLEKT_QUEUE_MTLS_CA_PEM",
            "DOKKOMPLEKT_QUEUE_MTLS_IDENTITY_PEM",
            "add_root_certificate",
            ".identity(identity)",
            ".https_only(true)",
            ".no_proxy()",
            "CentralQueueLease",
            "CONNECT_TIMEOUT",
            "connect_timeout",
        ]:
            self.assertIn(invariant, queue)
        self.assertIn("ssl.CERT_REQUIRED", service)
        self.assertIn("BEGIN IMMEDIATE", service)
        self.assertIn("status='completed'", service)
        self.assertIn("acquire_from_env", runtime)
        self.assertIn("central_queue_lease", runtime)
        self.assertNotIn("NoTls", queue)
        self.assertNotIn("postgres.workspace = true", cargo)

    def test_calendar_feed_is_automatic_but_rate_limited(self) -> None:
        source = text("src-tauri/src/reference_data_update.rs")
        main = text("src-tauri/src/main.rs")
        self.assertIn("AUTO_CHECK_INTERVAL", source)
        self.assertIn("maybe_auto_update", source)
        self.assertIn("production_calendar_auto_update.json", source)
        self.assertIn("reference_data_update::maybe_auto_update", main)

    def test_queue_and_resume_observability_reach_ui(self) -> None:
        api = text("src/lib/api.ts")
        types = text("src/lib/types.ts")
        ui = text("src/components/AutomationControlCenter.tsx")
        self.assertIn("get_queue_status", api)
        self.assertIn("interface QueueStatus", types)
        self.assertIn("Межкомпьютерная очередь", ui)
        self.assertIn("Использовано повторно", ui)

    def test_release_still_requires_real_rust_and_windows_gates(self) -> None:
        quality = text(".github/workflows/quality-gate.yml")
        installers = text(".github/workflows/build-installers.yml")
        public_bridge = text(".github/workflows/windows-hardware-e2e.yml")
        private_hardware = text("ops/private-hardware-validation/windows-hardware-e2e.yml")
        self.assertIn("cargo audit --deny warnings", quality)
        self.assertIn("prepackage_rust_gate", installers)
        self.assertIn("runs-on: ubuntu-latest", public_bridge)
        self.assertNotIn("runs-on: [self-hosted", public_bridge)
        self.assertIn("runs-on: windows-latest", private_hardware)
        self.assertNotIn("self-hosted, Windows, X64, dokkomplekt-runtime", private_hardware)
        self.assertIn("self-hosted, Windows, X64, dokkomplekt-hardware", private_hardware)
        self.assertEqual(private_hardware.count("runs-on: [self-hosted"), 1)
        self.assertIn("environment: windows-production-signing", private_hardware)
        self.assertIn("environment: windows-hardware-validation", private_hardware)
        self.assertIn("needs: signed-runtime-build", private_hardware)
        self.assertIn("stage_signed_runtime_bundle.py", private_hardware)
        self.assertFalse((ROOT / ".cargo-gate/CARGO_GATE_PASSED.ok").exists())


if __name__ == "__main__":
    unittest.main()
