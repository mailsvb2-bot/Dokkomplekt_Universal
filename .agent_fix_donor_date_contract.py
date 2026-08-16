from pathlib import Path

parser = Path('crates/dokkomplekt-core/src/date_parser.rs')
text = parser.read_text(encoding='utf-8')
old_loop = 'for (day_len, month_len) in patterns {'
new_loop = 'for &(day_len, month_len) in &patterns {'
count = text.count(old_loop)
if count != 2:
    raise SystemExit(f'expected exactly 2 date loops, found {count}')
parser.write_text(text.replace(old_loop, new_loop), encoding='utf-8')

regression = Path('crates/dokkomplekt-core/tests/behavior_regressions.rs')
text = regression.read_text(encoding='utf-8')
old = '''    // Ambiguous historical shorthand is intentionally rejected instead of
    // silently turning 1126 into 01.01.2026.
    assert_eq!(parse_flexible_date("1126", 2026), None);'''
new = '''    // Donor compatibility is restored only after modern DDMM interpretation
    // fails: 1126 cannot be DDMM (month 26), so it is read as D/M/YY.
    assert_eq!(
        parse_flexible_date("1126", 2026).as_deref(),
        Some("01.01.2026")
    );'''
if text.count(old) != 1:
    raise SystemExit('stale 1126 regression contract not found exactly once')
regression.write_text(text.replace(old, new, 1), encoding='utf-8')
