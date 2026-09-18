#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedMoney {
    minor_units: i128,
    had_fraction: bool,
}

pub(crate) fn parse_money_minor_units(value: &str) -> Result<i128, ()> {
    parse_money(value).map(|parsed| parsed.minor_units)
}

pub(crate) fn normalize_money(value: &str) -> Option<String> {
    let parsed = parse_money(value).ok()?;
    let negative = parsed.minor_units.is_negative();
    let absolute = parsed.minor_units.checked_abs()?;
    let integer = absolute / 100;
    let fraction = absolute % 100;
    let grouped = group_thousands(&integer.to_string());
    let sign = if negative { "-" } else { "" };
    if parsed.had_fraction {
        Some(format!("{sign}{grouped},{fraction:02}"))
    } else {
        Some(format!("{sign}{grouped}"))
    }
}

fn parse_money(value: &str) -> Result<ParsedMoney, ()> {
    let mut normalized = value.trim().to_lowercase();
    for suffix in ["рублей", "рубля", "руб.", "руб", "₽"] {
        if normalized.ends_with(suffix) {
            let new_len = normalized.len().saturating_sub(suffix.len());
            normalized.truncate(new_len);
            normalized = normalized.trim().to_string();
            break;
        }
    }
    if normalized.is_empty() {
        return Err(());
    }

    let (negative, unsigned) = if let Some(rest) = normalized.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = normalized.strip_prefix('+') {
        (false, rest)
    } else {
        (false, normalized.as_str())
    };
    if unsigned.is_empty() || unsigned.contains('+') || unsigned.contains('-') {
        return Err(());
    }

    let has_comma = unsigned.contains(',');
    let has_dot = unsigned.contains('.');
    if has_comma && has_dot {
        return Err(());
    }
    let separator = if has_comma {
        Some(',')
    } else if has_dot {
        Some('.')
    } else {
        None
    };
    if separator.is_some_and(|separator| unsigned.matches(separator).count() != 1) {
        return Err(());
    }

    let (integer_text, fraction_text) = match separator {
        Some(separator) => {
            let (integer, fraction) = unsigned.split_once(separator).ok_or(())?;
            (integer, Some(fraction))
        }
        None => (unsigned, None),
    };
    if integer_text.is_empty() {
        return Err(());
    }

    let has_grouping = integer_text
        .chars()
        .any(|character| matches!(character, ' ' | '\u{00a0}' | '\''));
    let integer_digits = if has_grouping {
        let groups = integer_text
            .split([' ', '\u{00a0}', '\''])
            .collect::<Vec<_>>();
        if groups.is_empty()
            || groups[0].is_empty()
            || groups[0].len() > 3
            || !groups[0]
                .chars()
                .all(|character| character.is_ascii_digit())
            || groups[1..].iter().any(|group| {
                group.len() != 3 || !group.chars().all(|character| character.is_ascii_digit())
            })
        {
            return Err(());
        }
        groups.concat()
    } else {
        if !integer_text
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            return Err(());
        }
        integer_text.to_string()
    };

    let integer = integer_digits.parse::<i128>().map_err(|_| ())?;
    let fraction_minor = match fraction_text {
        None => 0_i128,
        Some(fraction)
            if !fraction.is_empty()
                && fraction.len() <= 2
                && fraction.chars().all(|character| character.is_ascii_digit()) =>
        {
            let parsed = fraction.parse::<i128>().map_err(|_| ())?;
            if fraction.len() == 1 {
                parsed.checked_mul(10).ok_or(())?
            } else {
                parsed
            }
        }
        Some(_) => return Err(()),
    };

    let amount = integer
        .checked_mul(100)
        .and_then(|scaled| scaled.checked_add(fraction_minor))
        .ok_or(())?;
    let minor_units = if negative {
        amount.checked_neg().ok_or(())?
    } else {
        amount
    };
    Ok(ParsedMoney {
        minor_units,
        had_fraction: fraction_text.is_some(),
    })
}

fn group_thousands(digits: &str) -> String {
    let chars = digits.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let len = chars.len();
    for (index, character) in chars.iter().enumerate() {
        if index > 0 && (len - index).is_multiple_of(3) {
            out.push('\u{00a0}');
        }
        out.push(*character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_minor_units_do_not_round_above_f64_integer_precision() {
        assert_eq!(
            parse_money_minor_units("9 007 199 254 740 993,01 руб."),
            Ok(900_719_925_474_099_301)
        );
    }

    #[test]
    fn strict_parser_rejects_ambiguous_or_truncated_notation() {
        for value in ["1.234,56", "12.345", "1.", ".50", "NaN", "12 34,56"] {
            assert_eq!(parse_money_minor_units(value), Err(()), "{value}");
            assert_eq!(normalize_money(value), None, "{value}");
        }
    }

    #[test]
    fn display_normalization_is_derived_from_exact_minor_units() {
        assert_eq!(normalize_money("-12,5 ₽").as_deref(), Some("-12,50"));
        assert_eq!(normalize_money("+0.01").as_deref(), Some("0,01"));
        assert_eq!(
            normalize_money("1000000").as_deref(),
            Some("1\u{00a0}000\u{00a0}000")
        );
    }

    #[test]
    fn minor_unit_overflow_is_rejected() {
        assert_eq!(
            parse_money_minor_units("170141183460469231731687303715884105727"),
            Err(())
        );
    }
}
