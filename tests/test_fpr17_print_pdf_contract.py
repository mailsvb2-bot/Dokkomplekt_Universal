from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src-tauri" / "src" / "main.rs"
DESKTOP_IO = ROOT / "src-tauri" / "src" / "subsystems" / "desktop_io.rs"
HARDWARE = ROOT / "tests" / "windows" / "windows_hardware_e2e.ps1"
REGISTER = ROOT / "docs" / "CANON_FEATURE_PRESERVATION_REGISTER.json"


def test_fpr17_installed_pdf_export_uses_production_converter_and_is_hardware_guarded() -> None:
    main = MAIN.read_text(encoding="utf-8")
    desktop_io = DESKTOP_IO.read_text(encoding="utf-8")
    assert '--e2e-export-pdf=' in main
    assert 'DOKKOMPLEKT_RUN_HARDWARE_E2E' in main
    assert 'run_fpr17_pdf_export_e2e(&source, &evidence_path)' in main
    assert 'fn run_fpr17_pdf_export_e2e(' in desktop_io
    assert 'convert_office_document_to_pdf(&source, false)' in desktop_io
    assert 'verify_pdf_signature(&pdf_evidence_path)' in desktop_io
    assert '"dokkomplekt.fpr17-pdf-export-e2e.v1"' in desktop_io
    assert '"production convert_office_document_to_pdf"' in desktop_io
    assert '"app_version": env!("CARGO_PKG_VERSION")' in desktop_io
    assert '"converter_sha256": converter_sha256' in desktop_io
    assert '"converter_version": converter_version' in desktop_io
    assert '"conversion_duration_ms": conversion_duration_ms' in desktop_io
    assert 'office_converter_identity()' in desktop_io


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
