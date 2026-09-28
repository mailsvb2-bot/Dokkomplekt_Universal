from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src-tauri" / "src" / "main.rs"
HARDWARE = ROOT / "tests" / "windows" / "windows_hardware_e2e.ps1"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr17_installed_pdf_export_uses_production_converter_and_is_hardware_guarded() -> None:
    main = MAIN.read_text(encoding="utf-8")
    assert '--e2e-export-pdf=' in main
    assert 'DOKKOMPLEKT_RUN_HARDWARE_E2E' in main
    assert 'convert_office_document_to_pdf(&source, false)' in main
    assert 'verify_pdf_signature(&pdf_evidence_path)' in main
    assert '"dokkomplekt.fpr17-pdf-export-e2e.v1"' in main
    assert '"production convert_office_document_to_pdf"' in main


def test_fpr17_hardware_lane_requires_real_pdf_bytes_hash_and_spooler_completion() -> None:
    script = HARDWARE.read_text(encoding="utf-8-sig")
    for marker in (
        "FPR-17 PDF INSTALLED PASS:",
        "FPR-17 PRINT INSTALLED PASS:",
        "Windows PrintService Event 307",
        "dokkomplekt.fpr17-pdf-export-e2e.v1",
        "Get-FileHash -LiteralPath $fpr17Pdf -Algorithm SHA256",
        "[Text.Encoding]::ASCII.GetString($fpr17Header, 0, 5) -ne '%PDF-'",
        "fpr17_pdf_export_verified = $true",
    ):
        assert marker in script
    assert "COM submission alone is not accepted" in script


def test_fpr17_register_stays_open_until_private_hardware_evidence_is_green() -> None:
    register = REGISTER.read_text(encoding="utf-8")
    assert '"id": "FPR-17"' in register
    assert '"status": "needs-runtime-proof"' in register
    assert "hardware" in register.lower() or "installed" in register.lower()
