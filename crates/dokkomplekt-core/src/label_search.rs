//! Unicode-safe helpers for case-insensitive label lookup.
//!
//! Rust string slicing uses UTF-8 byte offsets. Lowercasing can change the
//! number of bytes (for example `İ` -> `i` + combining dot), therefore an
//! offset returned from a lowercased copy must never be applied directly to
//! the original string. This module keeps an explicit mapping back to valid
//! character boundaries of the source text.

/// Returns the byte offset in `raw_line` immediately after a case-insensitive
/// label match. The match must start and end on non-alphanumeric boundaries.
///
/// Most dictionary labels intentionally support inline/contextual forms such as
/// `Счёт № 148 от 21.02.2026`, `05.03.1980 г.р.` or
/// `работает в должности врача`. Tightening every label to line/cell starts
/// breaks those contracts. A very small policy table below is therefore used
/// only for bare labels that are known to be ambiguous in narrative prose.
/// Such labels additionally require structural provenance: line/cell start or
/// a separator before the label.
///
/// The returned offset is always a valid UTF-8 character boundary in
/// `raw_line`, even when Unicode lowercasing expands a character.
pub(crate) fn find_label_end(raw_line: &str, label: &str) -> Option<usize> {
    let label_lower = label.to_lowercase();
    if label_lower.is_empty() {
        return None;
    }

    let mut lowered = String::with_capacity(raw_line.len());
    // (end byte in lowered text, end byte in original text)
    let mut char_ends: Vec<(usize, usize)> = Vec::with_capacity(raw_line.chars().count());
    for (orig_pos, ch) in raw_line.char_indices() {
        for folded in ch.to_lowercase() {
            lowered.push(folded);
        }
        char_ends.push((lowered.len(), orig_pos + ch.len_utf8()));
    }

    let mut search_from = 0usize;
    while search_from <= lowered.len() {
        let rel = lowered.get(search_from..)?.find(&label_lower)?;
        let start = search_from + rel;
        let end = start + label_lower.len();
        let prev_is_word = lowered
            .get(..start)
            .and_then(|value| value.chars().next_back())
            .is_some_and(char::is_alphanumeric);
        let next_is_word = lowered
            .get(end..)
            .and_then(|value| value.chars().next())
            .is_some_and(char::is_alphanumeric);

        if !prev_is_word && !next_is_word {
            let start_original = original_boundary(start, &char_ends);
            let end_original = original_boundary(end, &char_ends);
            if let (Some(start_original), Some(end_original)) = (start_original, end_original) {
                let structural_required = requires_structural_provenance(&label_lower);
                if !structural_required || has_structural_label_prefix(raw_line, start_original) {
                    return Some(end_original);
                }
            }
        }

        // Move by one Unicode scalar, not by one byte, to avoid invalid slices.
        search_from = lowered
            .get(start..)
            .and_then(|value| value.chars().next())
            .map(|value| start + value.len_utf8())
            .unwrap_or(lowered.len().saturating_add(1));
    }
    None
}

fn original_boundary(lowered_boundary: usize, char_ends: &[(usize, usize)]) -> Option<usize> {
    if lowered_boundary == 0 {
        return Some(0);
    }
    char_ends
        .binary_search_by_key(&lowered_boundary, |entry| entry.0)
        .ok()
        .map(|index| char_ends[index].1)
}

/// Bare labels in this list are semantically useful as section headings but
/// common enough in prose that a word-boundary match alone is unsafe evidence.
/// Keep this list deliberately small: contextual aliases must retain their
/// historical inline matching semantics.
fn requires_structural_provenance(label_lower: &str) -> bool {
    matches!(label_lower.trim(), "лечение")
}

fn has_structural_label_prefix(raw_line: &str, label_start: usize) -> bool {
    let prefix = raw_line.get(..label_start).unwrap_or_default();
    if prefix.trim().is_empty() || prefix.ends_with('\t') {
        return true;
    }

    let trimmed = prefix.trim_end_matches([' ', '\u{00A0}']);
    let Some(last) = trimmed.chars().next_back() else {
        return true;
    };
    matches!(
        last,
        ':' | ';'
            | ','
            | '.'
            | '|'
            | '/'
            | '\\'
            | '-'
            | '—'
            | '–'
            | '('
            | '['
            | '"'
            | '\''
            | '«'
            | '•'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_lowercase_expansion_back_to_original_utf8_boundary() {
        let line = "İİİ дата: 12.05.2026";
        let end = find_label_end(line, "дата").expect("label");
        assert_eq!(&line[end..], ": 12.05.2026");
        assert!(line.is_char_boundary(end));
    }

    #[test]
    fn rejects_mid_word_matches() {
        assert_eq!(find_label_end("Отчество Иванович", "от"), None);
        assert_eq!(find_label_end("работодатель ООО", "тел"), None);
    }

    #[test]
    fn rejects_bare_treatment_inside_narrative_prose() {
        assert_eq!(
            find_label_end("Пациент продолжает лечение амбулаторно", "лечение"),
            None
        );
        assert_eq!(
            find_label_end(
                "Во время госпитализации лечение проводилось по схеме",
                "лечение"
            ),
            None
        );
    }

    #[test]
    fn accepts_explicit_treatment_label_at_line_or_cell_start() {
        let line = "  Лечение терапия, режим";
        let end = find_label_end(line, "лечение").expect("label");
        assert_eq!(&line[end..], " терапия, режим");

        let line = "Сведения — Лечение: терапия";
        let end = find_label_end(line, "лечение").expect("label after separator");
        assert_eq!(&line[end..], ": терапия");

        let line = "Сведения\tЛечение: терапия";
        let end = find_label_end(line, "лечение").expect("label after cell separator");
        assert_eq!(&line[end..], ": терапия");
    }

    #[test]
    fn preserves_contextual_inline_labels() {
        assert!(find_label_end("Счёт № 148 от 21.02.2026", "от").is_some());
        assert!(find_label_end("05.03.1980 г.р.", "г.р").is_some());
        assert!(find_label_end("работает в должности врача-терапевта", "в должности").is_some());
    }

    #[test]
    fn accepts_compact_multiple_structured_fields_on_one_line() {
        let line = "Исполнитель: ООО Ромашка. Заказчик: ООО Север";
        let end = find_label_end(line, "Заказчик").expect("second structured field");
        assert_eq!(&line[end..], ": ООО Север");
    }

    #[test]
    fn accepts_mixed_case_label() {
        let line = "НОМЕР: 42";
        let end = find_label_end(line, "Номер").expect("label");
        assert_eq!(&line[end..], ": 42");
    }
}
