from pathlib import Path

path = Path("crates/dokkomplekt-core/src/label_search.rs")
text = path.read_text(encoding="utf-8")
old = '''                let structural_required = requires_structural_provenance(&label_lower);
                if !structural_required || has_structural_label_prefix(raw_line, start_original) {
                    return Some(end_original);
                }
'''
new = '''                let structural_required = requires_structural_provenance(&label_lower);
                if !structural_required
                    || has_structural_label_prefix(raw_line, start_original)
                    || has_explicit_label_suffix(raw_line, end_original)
                {
                    return Some(end_original);
                }
'''
if text.count(old) != 1:
    raise SystemExit("label-search structural guard anchor mismatch")
text = text.replace(old, new, 1)
anchor = '''fn has_structural_label_prefix(raw_line: &str, label_start: usize) -> bool {
'''
helper = '''fn has_explicit_label_suffix(raw_line: &str, label_end: usize) -> bool {
    raw_line
        .get(label_end..)
        .unwrap_or_default()
        .trim_start()
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, ':' | '№' | '#'))
}

'''
if text.count(anchor) != 1:
    raise SystemExit("label-search helper anchor mismatch")
text = text.replace(anchor, helper + anchor, 1)
test_anchor = '''    #[test]
    fn accepts_explicit_treatment_label_at_line_or_cell_start() {
'''
test = '''    #[test]
    fn accepts_compact_treatment_label_when_the_suffix_is_explicit() {
        let line = "План обследования: ОАК Лечение: терапия";
        let end = find_label_end(line, "Лечение").expect("compact explicit treatment label");
        assert_eq!(&line[end..], ": терапия");
        assert_eq!(find_label_end("Пациент продолжает лечение амбулаторно", "лечение"), None);
    }

'''
if text.count(test_anchor) != 1:
    raise SystemExit("label-search test anchor mismatch")
text = text.replace(test_anchor, test + test_anchor, 1)
path.write_text(text, encoding="utf-8")
