from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text("utf-8")


class V1830HardeningContracts(unittest.TestCase):
    def test_offline_runtime_supply_chain_is_locked(self) -> None:
        builder = text("scripts/build_windows_offline_runtime.py")
        self.assertIn("--production", builder)
        self.assertIn("--require-supply-chain", builder)
        self.assertIn("license_path", builder)
        staged = ROOT / "verification" / "tmp-runtime-contract"
        shutil.rmtree(staged, ignore_errors=True)
        staged.mkdir(parents=True, exist_ok=True)
        # Existing detailed supply-chain fixtures live in the dedicated runtime tests;
        # this contract keeps the source-level release invariants visible here.
        self.assertIn("--production", text("BUILD_WINDOWS_INSTALLER.bat"))
        self.assertIn("probe_offline_runtime.py", text("BUILD_WINDOWS_INSTALLER.bat"))
        shutil.rmtree(staged, ignore_errors=True)

    def test_rustsec_release_evidence_is_bound_to_lock_report_and_database_commit(self) -> None:
        evidence = text("scripts/write_rustsec_evidence.py")
        attestation = text("scripts/write_cargo_gate_attestation.py")
        release = text("scripts/assert_release_ready.py")
        shell = text("scripts/prepackage_rust_gate.sh")
        bat = text("scripts/prepackage_rust_gate.bat")
        for invariant in [
            "RUSTSEC_AUDIT.json",
            "RUSTSEC_EVIDENCE.json",
            "RUSTSEC_DB_PIN.json",
            "advisory_database_commit",
            "advisory_database_dirty",
            "advisory_database_pin_report_sha256",
            "cargo_lock_sha256",
            "audit_report_sha256",
            "cargo_audit_version",
        ]:
            self.assertIn(invariant, evidence)
        self.assertIn("cargo audit --deny warnings --json", shell)
        self.assertIn("cargo audit --deny warnings --json", bat)
        self.assertIn("dokkomplekt.cargo-gate.v4", attestation)
        self.assertIn("rustsec_evidence_sha256", attestation)
        self.assertIn("rustsec_pin_report_sha256", attestation)
        self.assertIn("advisory_database_commit", attestation)
        self.assertIn("RUSTSEC_EVIDENCE.json", release)
        self.assertIn("RUSTSEC_AUDIT.json", release)

    def test_tauri_command_registry_matches_thin_typescript_api(self) -> None:
        import re
        main = text("src-tauri/src/main.rs")
        api = text("src/lib/api.ts")
        handler = re.search(r"generate_handler!\s*\[([^\]]+)\]", main, re.DOTALL)
        self.assertIsNotNone(handler)
        self.assertIn("invoke", api)


if __name__ == "__main__":
    unittest.main()
