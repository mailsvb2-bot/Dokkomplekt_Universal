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
            // A match ending inside an expanded lowercase character is not a
            // valid mapping. `binary_search` only succeeds at a source-char
            // boundary.
            if let Ok(index) = char_ends.binary_search_by_key(&end, |entry| entry.0) {
                return Some(char_ends[index].1);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_lowercase_expansion_back_to_original_utf8_boundary() {
        let line = "İ Дата: 12.05.2026";
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
    fn accepts_mixed_case_label() {
        let line = "НОМЕР: 42";
        let end = find_label_end(line, "Номер").expect("label");
        assert_eq!(&line[end..], ": 42");
    }
}
