from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures" / "ocr"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def test_image_only_scanned_table_golden_is_hash_locked() -> None:
    expected = json.loads((FIXTURES / "scanned_table.expected.json").read_text("utf-8"))
    pdf = FIXTURES / expected["pdf"]
    tsv = FIXTURES / expected["tsv"]
    assert pdf.read_bytes().startswith(b"%PDF-")
    assert pdf.stat().st_size > 50_000
    assert sha256(pdf) == expected["pdf_sha256"]
    assert sha256(tsv) == expected["tsv_sha256"]
    assert expected["image_only"] is True
    assert expected["minimum_table_rows"] >= 4


def test_fixture_is_wired_to_rust_parser_and_windows_runtime_gate() -> None:
    intake = (ROOT / "src-tauri/src/universal_intake.rs").read_text("utf-8")
    verifier = (ROOT / "scripts/verify_scanned_pdf_fixture.py").read_text("utf-8")
    build = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")
    hardware = (ROOT / ".github/workflows/windows-hardware-e2e.yml").read_text("utf-8")
    assert 'include_str!("../../tests/fixtures/ocr/scanned_table.tesseract.tsv")' in intake
    assert "parse_tesseract_tsv(tsv, 0)" in intake
    for invariant in [
        "pdftoppm",
        "pdftotext",
        "tesseract",
        "rus+eng",
        "minimum_table_rows",
        "scanned-table-smoke.v1",
    ]:
        assert invariant in verifier
    command = "python scripts/verify_scanned_pdf_fixture.py --runtime-root src-tauri/resources/tools/windows-x86_64"
    assert command in build
    assert command in hardware
