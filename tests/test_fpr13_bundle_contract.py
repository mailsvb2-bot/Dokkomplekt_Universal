from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
E1 = ROOT / "tests" / "installer" / "windows_e1_accounting_contract.ps1"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr13_installed_bundle_proof_is_physical_and_shared() -> None:
    source = E1.read_text(encoding="utf-8-sig")
    for marker in (
        "FPR-13 INSTALLED PASS:",
        "$fpr13BundleFolders.Count -ne 1",
        "$fpr13PlanBindings.Count -ne 1",
        "Комплект создан: 2 документ(ов)",
        "2 distinct readable DOCX",
        "2 committed receipts",
    ):
        assert marker in source


def test_fpr13_register_names_installed_evidence_without_overclaiming_before_ci() -> None:
    source = REGISTER.read_text(encoding="utf-8")
    assert '"id": "FPR-13"' in source
    assert "tests/installer/windows_e1_accounting_contract.ps1#FPR-13" in source
    assert "Physical multi-document bundle evidence is implemented; a green installed Windows run is still required for final closure." in source
