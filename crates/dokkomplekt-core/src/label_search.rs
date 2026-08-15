//! Unicode-safe helpers for case-insensitive label lookup.
//!
//! Rust string slicing uses UTF-8 byte offsets. Lowercasing can change the
//! number of bytes (for example `İ` -> `i` + combining dot), therefore an
//! offset returned from a lowercased copy must never be applied directly to
//! the original string. This module keeps an explicit mapping back to valid
//! character boundaries of the source text.

/// Returns the byte offset in `raw_line` immediately after a case-insensitive
/// label match.
///
/// A label must start either at the beginning of a logical line/cell (ignoring
/// whitespace) or immediately after a structural separator. Merely finding a
/// word boundary inside narrative prose is not enough: e.g. `За время лечения`
/// must not be interpreted as the explicit field label `Лечение`. This rule is
/// profession-neutral and protects every domain from substring-like provenance
/// mistakes while still allowing compact forms such as
/// `Исполнитель: ООО Ромашка. Заказчик: ООО Север`.
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
                if has_structural_label_prefix(raw_line, start_original) {
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

fn has_structural_label_prefix(raw_line: &str, label_start: usize) -> bool {
    let prefix = raw_line.get(..label_start).unwrap_or_default().trim_end();
    let Some(last) = prefix.chars().next_back() else {
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
        let line = "İ — Дата: 12.05.2026";
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
    fn rejects_narrative_word_boundary_as_field_provenance() {
        assert_eq!(
            find_label_end("За время лечения состояние улучшилось", "лечение"),
            None
        );
        assert_eq!(
            find_label_end("Пациент находится на лечении амбулаторно", "лечение"),
            None
        );
    }

    #[test]
    fn accepts_explicit_label_at_line_or_cell_start() {
        let line = "  Лечение терапия, режим";
        let end = find_label_end(line, "лечение").expect("label");
        assert_eq!(&line[end..], " терапия, режим");

        let line = "Сведения — Лечение: терапия";
        let end = find_label_end(line, "лечение").expect("label after separator");
        assert_eq!(&line[end..], ": терапия");
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
