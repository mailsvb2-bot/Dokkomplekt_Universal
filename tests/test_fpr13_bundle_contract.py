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


def test_fpr13_register_closes_only_with_exact_green_installed_evidence() -> None:
    register = __import__("json").loads(REGISTER.read_text(encoding="utf-8"))
    feature = next(item for item in register["features"] if item["id"] == "FPR-13")
    assert feature["status"] == "verified"
    assert feature["runtime_gap"] == ""
    assert any(
        "Quality Gate run 36621254590" in evidence
        and "commit 87809e6bb54931569470b8350eddd25393f1bd44" in evidence
        and "FPR-13 INSTALLED PASS" in evidence
        for evidence in feature["runtime_evidence"]
    )
