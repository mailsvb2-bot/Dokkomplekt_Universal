from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"
PUBLICATION = ROOT / "src-tauri" / "src" / "generation_publication.rs"


def test_fpr22_installed_proof_covers_receipt_schema_and_diagnostics() -> None:
    script = INSTALLER.read_text(encoding="utf-8")
    required = [
        "FPR-22 RECEIPT PRIVACY PASS",
        "FPR-22 DIAGNOSTIC PRIVACY PASS",
        "allowedReceiptFields",
        "RedirectStandardOutput",
        "RedirectStandardError",
        "FPR-22 committed receipt exposes an unexpected field",
        "FPR-22 installed diagnostics leaked path/case data",
    ]
    for marker in required:
        assert marker in script, marker

    # The runtime proof must check concrete case/path sentinels, not merely
    # forbidden field names.
    for sentinel in [
        "ООО «Альфа»",
        "ООО «Бета»",
        "D-77",
        "Консультационные услуги",
        "Сидоров Сергей Сергеевич",
        "Орлова Анна Игоревна",
    ]:
        assert sentinel in script, sentinel


def test_completion_receipt_contract_stores_only_opaque_audit_identity() -> None:
    source = PUBLICATION.read_text(encoding="utf-8")
    assert "committed_generation_receipt_is_non_pii_and_bound_to_physical_output" in source
    assert "plan_binding_sha256" in source
    assert "proof_contract" in source
    assert "verifier_contract" in source
    for forbidden in [
        "output_path:",
        "source_path:",
        "patient:",
        "fio:",
    ]:
        # These fields must not exist in the GenerationCompletionReceipt struct.
        struct_start = source.index("struct GenerationCompletionReceipt")
        struct_end = source.index("}", struct_start)
        assert forbidden not in source[struct_start:struct_end], forbidden


def test_feature_register_requires_green_runtime_before_fpr22_verification() -> None:
    register = json.loads(REGISTER.read_text(encoding="utf-8"))
    feature = next(item for item in register["features"] if item["id"] == "FPR-22")
    assert feature["status"] == "needs-runtime-proof"
    assert "FPR-22 RECEIPT PRIVACY PASS" in feature["runtime_gap"]
    assert "FPR-22 DIAGNOSTIC PRIVACY PASS" in feature["runtime_gap"]
