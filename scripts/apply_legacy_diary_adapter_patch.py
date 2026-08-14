from pathlib import Path

lib = Path("crates/dokkomplekt-docx/src/lib.rs")
text = lib.read_text(encoding="utf-8")

if "mod legacy_diary_table;" not in text:
    marker = "//! strict placeholder checks before writing a rendered DOCX.\n\n"
    if marker not in text:
        raise SystemExit("module anchor missing")
    text = text.replace(marker, marker + "mod legacy_diary_table;\n\n", 1)

variant_anchor = '''    #[error("unsafe active or externally linked content in DOCX template: {0}")]
    UnsafeActiveContent(String),
'''
variant = variant_anchor + '''    #[error("legacy diary table cannot be rendered safely: {0}")]
    LegacyDiaryTable(String),
'''
if "LegacyDiaryTable(String)" not in text:
    if variant_anchor not in text:
        raise SystemExit("error variant anchor missing")
    text = text.replace(variant_anchor, variant, 1)

render_anchor = '''        let prepared = promote_table_row_loops(&stitch_split_placeholders(&xml));
        let result = render_docx_xml_template(&prepared, case, strict);
'''
render_replacement = '''        let mut prepared = promote_table_row_loops(&stitch_split_placeholders(&xml));
        if name == "word/document.xml" {
            let legacy = legacy_diary_table::fill_legacy_diary_tables(&prepared, case, strict)
                .map_err(DocxError::LegacyDiaryTable)?;
            if legacy.detected_tables > 0 {
                aggregate.warnings.push(format!(
                    "legacy_diary_table:tables={},rows={},filled={},removed_after_discharge={},final_rows={}",
                    legacy.detected_tables,
                    legacy.detected_rows,
                    legacy.filled_rows,
                    legacy.removed_after_discharge,
                    legacy.final_rows
                ));
                extend_unique(&mut aggregate.warnings, legacy.warnings);
            }
            prepared = legacy.xml;
        }
        let result = render_docx_xml_template(&prepared, case, strict);
'''
if render_replacement not in text:
    if render_anchor not in text:
        raise SystemExit("render anchor missing")
    text = text.replace(render_anchor, render_replacement, 1)
lib.write_text(text, encoding="utf-8")

adapter = Path("crates/dokkomplekt-docx/src/legacy_diary_table.rs")
a = adapter.read_text(encoding="utf-8")
modern_anchor = '''        let table_xml = &xml[range.0..range.1];
        let Some(layout) = detect_diary_table_layout(table_xml, strict)? else {
'''
modern_replacement = '''        let table_xml = &xml[range.0..range.1];
        // Modern collection-aware templates are handled by the canonical
        // template engine. Never rewrite the same table twice.
        if table_xml.contains("{{#each") || table_xml.contains("{{diary.") {
            continue;
        }
        let Some(layout) = detect_diary_table_layout(table_xml, strict)? else {
'''
if modern_replacement not in a:
    if modern_anchor not in a:
        raise SystemExit("modern table guard anchor missing")
    a = a.replace(modern_anchor, modern_replacement, 1)

# Remove a no-op cursor kept in the first draft.
a = a.replace("    let mut cursor = 0usize;\n", "")
a = a.replace("    cursor += report.filled_rows;\n    debug_assert_eq!(cursor, report.filled_rows);\n", "")
adapter.write_text(a, encoding="utf-8")
