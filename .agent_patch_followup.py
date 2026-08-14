from pathlib import Path


def read(path):
    return Path(path).read_text(encoding='utf-8')


def write(path, text):
    Path(path).write_text(text, encoding='utf-8')


def replace(path, old, new, count=1):
    text = read(path)
    actual = text.count(old)
    assert actual == count, f'{path}: expected {count} matches, got {actual}: {old[:100]!r}'
    write(path, text.replace(old, new, count))


def replace_in_test(path, test_name, old, new):
    text = read(path)
    marker = f'fn {test_name}()'
    start = text.index(marker)
    next_test = text.find('\n    #[test]', start + len(marker))
    end = len(text) if next_test < 0 else next_test
    chunk = text[start:end]
    assert old in chunk, f'{path}:{test_name}: fragment not found'
    chunk = chunk.replace(old, new, 1)
    write(path, text[:start] + chunk + text[end:])


# Donor folder contract uses compact initials: "Иванов И.И.", not "Иванов И. И.".
replace(
    'crates/dokkomplekt-core/src/output_naming.rs',
    '''fn short_initials(name: &str) -> String {
    let parts = name.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return name.to_string();
    }
    let mut out = parts[0].to_string();
    for part in parts.iter().skip(1).take(2) {
        if let Some(ch) = part.chars().next() {
            out.push(' ');
            out.push(ch);
            out.push('.');
        }
    }
    out
}
''',
    '''fn short_initials(name: &str) -> String {
    let parts = name.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return name.to_string();
    }
    let mut initials = String::new();
    for part in parts.iter().skip(1).take(2) {
        if let Some(ch) = part.chars().next() {
            initials.push(ch);
            initials.push('.');
        }
    }
    if initials.is_empty() {
        parts[0].to_string()
    } else {
        format!("{} {initials}", parts[0])
    }
}
''',
)

# This pre-existing regression explicitly tests the soft "skip now, replace later" path.
# Keep it, but classify that prompt as skippable. Hard template requirements are covered
# separately and remain non-skippable.
replace_in_test(
    'crates/dokkomplekt-core/src/popup_engine.rs',
    'explicit_skip_hides_existing_value_until_user_supplies_a_replacement',
    '            skippable: false,\n',
    '            skippable: true,\n',
)

# The integration test remains exact: it now locks the restored donor-visible format.
replace_in_test(
    'crates/dokkomplekt-core/tests/behavior_regressions.rs',
    'output_folder_name_uses_spaces_not_underscores',
    '"Иванов И. И. 01.06.2026 - 03.06.2026"',
    '"Иванов И.И. 01.06.2026 - 03.06.2026"',
)

print('focused donor parity follow-up applied')
