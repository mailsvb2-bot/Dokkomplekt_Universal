from __future__ import annotations

import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "verification" / "autopilot" / "feature-matrix.json"
WORKFLOW = ROOT / ".github" / "workflows" / "full-product-autopilot.yml"
SCRIPT = ROOT / "scripts" / "full_product_autopilot.py"


def load_autopilot_module():
    spec = importlib.util.spec_from_file_location("full_product_autopilot", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_full_product_matrix_is_machine_valid_and_has_no_manual_only_capability() -> None:
    module = load_autopilot_module()
    report = module.validate_matrix(ROOT, MATRIX)
    assert report["valid"], report["errors"]
    assert report["feature_count"] == report["mandatory_feature_count"]
    assert report["manual_features"] == []
    assert report["missing_evidence"] == {}
    assert report["scopes"]["software"] >= 25
    assert report["scopes"]["production-hardware"] >= 5


def test_matrix_covers_product_boundaries_not_just_unit_tests() -> None:
    data = json.loads(MATRIX.read_text(encoding="utf-8"))
    features = {item["id"]: item for item in data["features"]}
    assert features["button-creation"]["level"] == "installed-e2e"
    assert "tests/installer/windows_installer_contract.ps1" in features["button-creation"]["evidence"]
    assert features["docx-oracle"]["level"] == "golden"
    assert features["postgres-concurrency"]["level"] == "real-db"
    assert features["word-print"]["level"] == "hardware-e2e"
    assert features["reboot-watcher"]["level"] == "hardware-e2e"
    assert features["authenticode"]["scope"] == "production-hardware"


def test_one_button_workflow_dispatches_existing_authoritative_gates() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    source = SCRIPT.read_text(encoding="utf-8")
    assert "name: FULL DOKKOMPLEKT AUTOPILOT" in workflow
    assert "workflow_dispatch:" in workflow
    assert "production-hardware" in workflow
    assert "actions: write" in workflow
    assert "scripts/full_product_autopilot.py dispatch" in workflow
    for authoritative in (
        "quality-gate.yml",
        "source-provenance.yml",
        "macos-smoke.yml",
        "unsigned-preview.yml",
        "windows-hardware-e2e.yml",
    ):
        assert authoritative in source
    assert "FULL_AUTOPILOT_REPORT.json" in workflow
    assert "FULL_AUTOPILOT_REPORT.md" in workflow


def test_document_oracles_are_executed_not_merely_registered() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    assert "document-oracles:\n    name: Document visual oracle and synthetic corpus" in workflow
    assert "python scripts/verify_docx_visual_goldens.py" in workflow
    assert "cargo +1.97.1 run --locked -p dokkomplekt-core --example corpus_simulation -- 100" in workflow
    assert "cargo run --locked -p dokkomplekt-core --example corpus_simulation -- 100" not in workflow
    assert "python scripts/measure_domain.py" in workflow
    assert "assert len(corpus['entries']) == 500" in workflow
    assert "assert report['field_accuracy'] is not None and report['field_accuracy'] >= 0.75" in workflow
    assert "assert report['kit_completeness'] == 1.0" in workflow
    assert "needs: [coverage-contract, document-oracles]" in workflow


def test_first_main_landing_runs_software_autopilot_without_starting_hardware() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    assert "push:\n    branches: [main]" in workflow
    assert "github.event_name == 'workflow_dispatch' || github.event_name == 'push'" in workflow
    assert "github.event_name == 'workflow_dispatch' && inputs.scope || 'software'" in workflow
    # Write privilege is scoped to the orchestration job rather than the PR coverage/oracle jobs.
    assert "coverage-contract:\n    name: Capability coverage contract" in workflow
    assert "document-oracles:\n    name: Document visual oracle and synthetic corpus" in workflow
    assert "full-autopilot:\n    name: Dispatch, wait and aggregate full product verification" in workflow


def test_software_pass_cannot_be_misrepresented_as_production_hardware_pass() -> None:
    source = SCRIPT.read_text(encoding="utf-8")
    assert 'result_name = "SOFTWARE PASS"' in source
    assert 'result_name = "FULL PRODUCTION PASS"' in source
    assert 'args.scope == "production-hardware" and args.ref != "main"' in source
    assert "Physical Word/printer/reboot/production Authenticode acceptance was intentionally not claimed" in source


def test_hardware_gate_remains_real_physical_evidence_not_a_mock() -> None:
    hardware = (ROOT / "tests" / "windows" / "windows_hardware_e2e.ps1").read_text(encoding="utf-8")
    assert "winword.exe" in hardware
    assert "Get-Printer" in hardware
    assert "PrintService/Operational" in hardware
    assert "Id=307" in hardware
    assert "Get-AuthenticodeSignature" in hardware
    assert "verify_reboot_evidence.ps1" in hardware
    assert "post_reboot_output_sha256" in hardware
    assert "silent_uninstall_passed = $true" in hardware
