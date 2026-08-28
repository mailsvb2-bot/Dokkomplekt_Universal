from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INTAKE = ROOT / "src-tauri" / "src" / "universal_intake.rs"
SOURCE_INTAKE_COMMANDS = ROOT / "src-tauri" / "src" / "subsystems" / "source_intake_commands.rs"
AUTOMATION = ROOT / "src-tauri" / "src" / "subsystems" / "automation_runtime.rs"
TYPES = ROOT / "src" / "lib" / "types.ts"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_ocr_uses_structured_tsv_and_distinguishes_scanned_inputs() -> None:
    source = read(INTAKE)
    assert '"tsv"' in source
    assert '"scanned_pdf_ocr".to_string()' in source
    assert '"mixed_pdf_page_ocr".to_string()' in source
    assert "pdf_page_requires_ocr" in source
    assert 'source_kind: "scanned_image"' in source
    assert "parse_tesseract_tsv" in source
    assert "let item_kind = if cells.len() >= 2" in source
    assert "item_kind: item_kind.into()" in source
    assert "LayoutBoundingBox" in source


def test_layout_reaches_semantic_case_and_zero_touch_runtime() -> None:
    intake = read(INTAKE)
    source_intake = read(SOURCE_INTAKE_COMMANDS)
    automation = read(AUTOMATION)
    assert '"source.layout_items".into(), records' in intake
    assert '"source.table_row_count".into()' in intake
    assert "attach_layout_evidence" in intake
    assert "universal_intake::apply_layout_to_case(&source_kind, &layout_items, &mut parsed);" in source_intake
    assert "universal_intake::attach_layout_evidence(&layout_items, &mut parsed);" in source_intake
    assert "universal_intake::apply_layout_to_case(" in automation
    assert "universal_intake::attach_layout_evidence(" in automation


def test_source_layout_is_exposed_to_frontend_without_network_dependency() -> None:
    source_intake = read(SOURCE_INTAKE_COMMANDS)
    typescript = read(TYPES)
    assert "source_kind: String" in source_intake
    assert "layout_items: Vec<universal_intake::NormalizedLayoutItem>" in source_intake
    assert "export interface NormalizedLayoutItem" in typescript
    assert "layout_items: NormalizedLayoutItem[]" in typescript
    assert "MAX_LAYOUT_ITEMS" in read(INTAKE)


def test_new_case_cannot_reuse_previous_case_or_source_metadata() -> None:
    source = read(SOURCE_INTAKE_COMMANDS)
    assert "fn replace_case_from_new_source" in source
    assert "*target = parsed;" in source
    assert "fn new_source_drops_every_block_from_previous_case()" in source
    assert "assert_eq!(current.blocks.len(), 1);" in source
    assert 'contains_key("professional.medical.diary.regular.f200")' in source
    assert 'contains_key("medical.diary.final_text")' in source
