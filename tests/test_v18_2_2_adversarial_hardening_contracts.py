from __future__ import annotations

import base64
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.ed25519_compat import SigningKey

ROOT = Path(__file__).resolve().parents[1]


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


class AdversarialHardeningContracts(unittest.TestCase):
    def test_model_consensus_uses_distinct_sampling_and_strict_grounding(self) -> None:
        model = read("src-tauri/src/semantic_model.rs")
        grounding = read("crates/dokkomplekt-core/src/semantic_llm.rs")
        self.assertIn("ConsensusSamplingProfile", model)
        self.assertIn("consensus_sampling_profile", model)
        self.assertIn("0.15", model)
        self.assertIn("0.25", model)
        self.assertIn("partial_name_tokens_do_not_ground_a_wrong_patronymic", grounding)
        self.assertIn("address_grounding_rejects_a_different_house_or_flat", grounding)
        self.assertIn("numeric_tokens", grounding)

    def test_watcher_retries_transient_failures_without_full_hash_polling(self) -> None:
        watcher = (
            read("src-tauri/src/subsystems/document_commands.rs")
            + read("src-tauri/src/subsystems/watcher_commands.rs")
        )
        self.assertIn("UnreadableRetryPolicy", watcher)
        self.assertIn("retry_after_unix_ms", watcher)
        self.assertIn("unreadable_note_blocks_retry", watcher)
        self.assertIn("pending_paths", watcher)
        self.assertIn("Duration::from_secs(30)", watcher)
        self.assertNotIn("пересохраните его в Word", watcher)

    def test_resume_and_dedup_are_bound_to_plan_and_output_integrity(self) -> None:
        storage = read("crates/dokkomplekt-storage/src/lib.rs")
        automation = read("src-tauri/src/subsystems/automation_runtime.rs")
        resume = read("src-tauri/src/resume_engine.rs")
        for marker in ["processing_fingerprint", "output_sha256", "output_size_bytes"]:
            self.assertIn(marker, storage)
        self.assertIn("completed_case_exists_for_source_and_plan", storage)
        self.assertIn("automation_plan_fingerprint", automation)
        self.assertIn("processing_job_key", automation)
        self.assertIn("checkpoint.sha256", automation)
        self.assertIn("sha256_file", resume)

    def test_component_transactions_recover_and_stale_files_are_cleaned(self) -> None:
        manager = read("src-tauri/src/component_manager.rs")
        self.assertIn("recover_component_transactions", manager)
        self.assertIn("STALE_COMPONENT_TRANSACTION_AGE", manager)
        self.assertIn("transaction_name_matches", manager)
        self.assertIn("Не удалось восстановить предыдущую версию компонента", manager)

    def test_runtime_state_is_encrypted_and_audit_chain_is_keyed(self) -> None:
        storage = read("crates/dokkomplekt-storage/src/lib.rs")
        runtime = read("src-tauri/src/subsystems/automation_runtime.rs")
        self.assertIn("let stored = self.encode_sensitive(&json)?", storage)
        self.assertIn("self.decode_sensitive(&stored)?", storage)
        self.assertIn("authenticated_audit_hash", storage)
        self.assertIn("hmac:v1:", storage)

    def test_downloaded_semantic_runtime_is_started_and_health_checked(self) -> None:
        runtime = read("src-tauri/src/semantic_runtime.rs")
        self.assertIn('resolve_tool("llama_cpp")', runtime)
        self.assertIn('resolve_tool("semantic_model")', runtime)
        self.assertIn("TcpStream::connect_timeout", runtime)
        self.assertIn("runtime завершился до готовности", runtime)
        self.assertIn("impl Drop for ManagedSemanticRuntime", runtime)

    def test_real_docx_corpus_has_structural_and_visual_goldens(self) -> None:
        manifest = json.loads(read("tests/fixtures/docx/corpus-manifest.json"))
        visual = json.loads(read("tests/fixtures/docx/visual-golden.json"))
        self.assertGreaterEqual(len(manifest["fixtures"]), 7)
        self.assertEqual(set(manifest["fixtures"]), set(visual["fixtures"]))
        for name, entry in manifest["fixtures"].items():
            path = ROOT / "tests/fixtures/docx" / name
            self.assertTrue(path.is_file())
            self.assertGreater(entry["size_bytes"], 3000)
            self.assertGreaterEqual(len(visual["fixtures"][name]["pages"]), 1)
        verifier = read("scripts/verify_docx_visual_goldens.py")
        self.assertIn("Broken relationship", verifier)
        self.assertIn("dhash16", verifier)
        self.assertIn("Blank visual page", verifier)

    def test_windows_contracts_require_exact_app_and_spool_completion(self) -> None:
        installer = read("tests/installer/windows_installer_contract.ps1")
        hardware = read("tests/windows/windows_hardware_e2e.ps1")
        self.assertIn("Expected exactly one installed Dokkomplekt application executable", installer)
        self.assertIn("ProductName", installer)
        self.assertIn("OriginalFilename", installer)
        self.assertIn("Microsoft-Windows-PrintService/Operational", hardware)
        self.assertIn("Id=307", hardware)
        self.assertIn("operating_system_reboot_tested = $true", hardware)
        self.assertIn("verify_reboot_evidence.ps1", hardware)
        self.assertIn("post_reboot_case_completed", hardware)
        self.assertTrue((ROOT / "tests/windows/verify_reboot_evidence.ps1").is_file())

    def test_windows_batch_gate_is_non_mutating_and_signed(self) -> None:
        batch = read("scripts/prepackage_rust_gate.bat")
        static_gate = read("scripts/static_quality_gate.py")
        evidence = read("scripts/write_windows_release_evidence.ps1")
        self.assertNotIn("cargo fmt --all ||", batch)
        self.assertIn("cargo fmt --all -- --check", batch)
        self.assertNotIn('["cargo", "fmt", "--all"],', static_gate)
        self.assertIn('["cargo", "fmt", "--all", "--", "--check"],', static_gate)
        self.assertIn("write_cargo_gate_attestation.py", batch)
        self.assertNotIn("CARGO_GATE_PASSED.ok", batch)
        self.assertIn("CARGO_GATE_ATTESTATION.json", evidence)
        self.assertIn("CARGO_GATE_ATTESTATION.sig", evidence)

    def test_signed_gate_attestation_rejects_tampering(self) -> None:
        gate_dir = ROOT / ".cargo-gate"
        shutil.rmtree(gate_dir, ignore_errors=True)
        with tempfile.TemporaryDirectory() as tmp:
            bin_dir = Path(tmp)
            if os.name == "nt":
                cargo = bin_dir / "cargo.bat"
                rustc = bin_dir / "rustc.bat"
                cargo.write_text("@echo cargo 1.85.1\n", encoding="utf-8")
                rustc.write_text("@echo rustc 1.85.1\n", encoding="utf-8")
            else:
                cargo = bin_dir / "cargo"
                rustc = bin_dir / "rustc"
                cargo.write_text("#!/bin/sh\necho cargo 1.85.1\n", encoding="utf-8")
                rustc.write_text("#!/bin/sh\necho rustc 1.85.1\n", encoding="utf-8")
                cargo.chmod(0o755)
                rustc.chmod(0o755)
            key = SigningKey.generate()
            env = os.environ.copy()
            env["PATH"] = str(bin_dir) + os.pathsep + env.get("PATH", "")
            env["DOKKOMPLEKT_GATE_PRIVATE_KEY_B64"] = base64.b64encode(bytes(key)).decode("ascii")
            env["DOKKOMPLEKT_GATE_PUBKEY_B64"] = base64.b64encode(bytes(key.verify_key)).decode("ascii")
            env["GITHUB_REPOSITORY"] = "example/dokkomplekt"
            env["GITHUB_SHA"] = "a" * 40
            env["GITHUB_RUN_ID"] = "123"
            env["GITHUB_RUN_ATTEMPT"] = "1"
            gate_dir.mkdir(parents=True, exist_ok=True)
            report = gate_dir / "RUSTSEC_AUDIT.json"
            report.write_text(
                json.dumps({"database": {"advisory-count": 0}, "vulnerabilities": {"found": False, "list": []}}),
                encoding="utf-8",
            )
            pin = gate_dir / "RUSTSEC_DB_PIN.json"
            pin.write_text(
                json.dumps({
                    "repository": "https://github.com/RustSec/advisory-db",
                    "commit": "b" * 40,
                }),
                encoding="utf-8",
            )
            source_sha256 = subprocess.check_output(
                [os.sys.executable, "scripts/source_fingerprint.py"],
                cwd=ROOT,
                text=True,
            ).strip()
            evidence = {
                "schema": "dokkomplekt.rustsec-evidence.v2",
                "result": "passed",
                "source_sha256": source_sha256,
                "cargo_lock_sha256": hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(),
                "audit_report_sha256": hashlib.sha256(report.read_bytes()).hexdigest(),
                "audit_command": "cargo audit --db <exact-pinned-checkout> --no-fetch --deny warnings --json",
                "cargo_audit_version": "cargo-audit 0.test",
                "advisory_database_commit": "b" * 40,
                "advisory_database_origin": "https://github.com/RustSec/advisory-db",
                "advisory_database_dirty": False,
                "advisory_database_pin_report_sha256": hashlib.sha256(pin.read_bytes()).hexdigest(),
            }
            (gate_dir / "RUSTSEC_EVIDENCE.json").write_text(
                json.dumps(evidence), encoding="utf-8"
            )
            commercial_lock = gate_dir / "COMMERCIAL_CRATES_Cargo.lock"
            commercial_lock.write_text("# deterministic commercial test lock\n", encoding="utf-8")
            commercial_audit = gate_dir / "COMMERCIAL_CRATES_RUSTSEC_AUDIT.json"
            commercial_audit.write_text(
                json.dumps({"database": {"advisory-count": 0}, "vulnerabilities": {"found": False, "list": []}}),
                encoding="utf-8",
            )
            commercial = {
                "schema": "dokkomplekt.commercial-rust-gate.v1",
                "result": "passed",
                "source_sha256": source_sha256,
                "generated_lock_sha256": hashlib.sha256(commercial_lock.read_bytes()).hexdigest(),
                "audit_report_sha256": hashlib.sha256(commercial_audit.read_bytes()).hexdigest(),
            }
            (gate_dir / "COMMERCIAL_CRATES_EVIDENCE.json").write_text(
                json.dumps(commercial), encoding="utf-8"
            )
            subprocess.run([os.sys.executable, "scripts/write_cargo_gate_attestation.py"], cwd=ROOT, env=env, check=True, capture_output=True, text=True)
            subprocess.run([os.sys.executable, "scripts/assert_release_ready.py"], cwd=ROOT, env=env, check=True, capture_output=True, text=True)
            attestation = gate_dir / "CARGO_GATE_ATTESTATION.json"
            payload = json.loads(attestation.read_text(encoding="utf-8"))
            payload["result"] = "failed"
            attestation.write_text(json.dumps(payload), encoding="utf-8")
            failed = subprocess.run([os.sys.executable, "scripts/assert_release_ready.py"], cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertNotEqual(failed.returncode, 0)
        shutil.rmtree(gate_dir, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
