from pathlib import Path

path = Path("crates/dokkomplekt-core/src/source_parser.rs")
text = path.read_text(encoding="utf-8")
anchor = '''    for warning in engine_report.warnings {
        if !report.warnings.contains(&warning) {
            report.warnings.push(warning);
        }
    }

    if let Some((items, warnings)) = extract_items_table(text) {
'''
replacement = '''    for warning in engine_report.warnings {
        if !report.warnings.contains(&warning) {
            report.warnings.push(warning);
        }
    }

    // Multiple deterministic extractors may identify the same person name with
    // different confidence. Normalize the canonical value only after all source
    // extractors have merged so a high-confidence generic match cannot preserve
    // a demographic tail such as `, 1975 г.р.` in document headers/folder names.
    if let Some(current_name) = case.get("subject.name").map(str::to_owned) {
        if let Some(cleaned_name) = sanitize_subject_name(&current_name) {
            if cleaned_name != current_name {
                if let Some(value) = case.values.get_mut("subject.name") {
                    value.value = cleaned_name;
                }
            }
        }
    }

    if let Some((items, warnings)) = extract_items_table(text) {
'''
if text.count(anchor) != 1:
    raise SystemExit(f"identity finalization anchor mismatch: {text.count(anchor)}")
path.write_text(text.replace(anchor, replacement, 1), encoding="utf-8")
