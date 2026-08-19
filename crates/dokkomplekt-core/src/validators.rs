//! Checksum and structural validators shared by deterministic parsing, model proposals and UI input.
use crate::{canonical_storage_field_id, SemanticCase};
use chrono::NaiveDate;

pub fn validate_field_value(field_id: &str, value: &str) -> Result<(), String> {
    let canonical = canonical_storage_field_id(field_id);
    let field_id = canonical.as_str();
    let v = value.trim();
    if v.is_empty() {
        return Ok(());
    }
    let exact = match field_id {
        "subject.snils" | "doctor.snils" => validate_snils(v),
        "org.inn" | "counterparty.inn" => validate_inn(v),
        "org.ogrn" | "counterparty.ogrn" => validate_ogrn(v),
        "org.kpp" | "counterparty.kpp" => validate_kpp(v),
        "org.bank_bik" => digits(v, 9, "БИК"),
        "org.bank_account" => digits(v, 20, "Расчётный счёт"),
        "org.bank_corr_account" => digits(v, 20, "Корреспондентский счёт"),
        "realty.cadastral_number" => validate_cadastral(v),
        "vehicle.vin" => validate_vin(v),
        "subject.passport_series" => digits(v, 4, "Серия паспорта"),
        "subject.passport_number" => digits(v, 6, "Номер паспорта"),
        _ => Ok(()),
    };
    exact?;
    let id = field_id.to_ascii_lowercase();
    if id.ends_with("_date") || id.ends_with(".date") || id.contains("date_") {
        let birth_year_only = field_id == "subject.birth_date" && is_plausible_birth_year(v);
        if !birth_year_only {
            parse_supported_date(v).map(|_| ()).ok_or_else(|| {
                format!("{field_id}: дата должна быть в формате ДД.ММ.ГГГГ или ГГГГ-ММ-ДД")
            })?;
        }
    }
    if matches!(
        crate::infer_input_kind(field_id),
        crate::PromptInputKind::Money | crate::PromptInputKind::Number
    ) {
        validate_decimal(v, field_id)?;
    }
    if id.ends_with("email") || id.ends_with(".email") {
        validate_email(v)?;
    }
    Ok(())
}

/// Validate relations which cannot be checked from one field in isolation.
/// The returned field id identifies the value that must be rejected or corrected.
pub fn validate_case_relations(case: &SemanticCase) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    if let Some(bik) = case.get("org.bank_bik") {
        if let Some(account) = case.get("org.bank_account") {
            if let Err(error) = validate_bank_account_with_bik(bik, account) {
                errors.push(("org.bank_account".into(), error));
            }
        }
        if let Some(account) = case.get("org.bank_corr_account") {
            if let Err(error) = validate_corr_account_with_bik(bik, account) {
                errors.push(("org.bank_corr_account".into(), error));
            }
        }
    }

    validate_date_order(
        case,
        &[
            "medical.admission_date",
            "admission.date",
            "admission_date",
            "document.start_date",
        ],
        &[
            "medical.discharge_date",
            "discharge.date",
            "discharge_date",
            "document.end_date",
        ],
        "Дата окончания не может быть раньше даты начала.",
        &mut errors,
    );
    validate_date_order(
        case,
        &["document.issue_date", "issue_date", "contract.start_date"],
        &["document.expiry_date", "expiry_date", "contract.end_date"],
        "Дата окончания действия не может быть раньше даты выдачи/начала.",
        &mut errors,
    );

    if let Some(birth_id) = first_present_field(
        case,
        &[
            "subject.birth_date",
            "patient.birth_date",
            "birth_date",
            "date_of_birth",
        ],
    ) {
        if let Some(birth) = case.get(birth_id).and_then(parse_supported_date) {
            if let Some(reference_id) = first_present_field(
                case,
                &[
                    "document.date",
                    "medical.admission_date",
                    "admission.date",
                    "admission_date",
                ],
            ) {
                if let Some(reference) = case.get(reference_id).and_then(parse_supported_date) {
                    if birth > reference {
                        errors.push((
                            birth_id.to_string(),
                            "Дата рождения не может быть позже даты документа/события.".into(),
                        ));
                    }
                }
            }
        }
    }
    for field_id in ["medical.workplace", "subject.organization"] {
        let Some(value) = case.get(field_id) else {
            continue;
        };
        if is_no_employment_marker(value)
            && !errors.iter().any(|(existing, _)| existing == field_id)
        {
            errors.push((
                field_id.to_string(),
                "Значение означает отсутствие места работы и не может быть названием организации."
                    .into(),
            ));
        }
    }
    errors
}

fn is_plausible_birth_year(value: &str) -> bool {
    value.len() == 4
        && value.chars().all(|ch| ch.is_ascii_digit())
        && value
            .parse::<i32>()
            .is_ok_and(|year| (1900..=2200).contains(&year))
}

fn is_no_employment_marker(value: &str) -> bool {
    let normalized = value
        .trim()
        .to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        "не работает",
        "не работаю",
        "не трудоустроен",
        "не трудоустроена",
        "безработный",
        "безработная",
        "безработен",
    ]
    .iter()
    .any(|marker| normalized == *marker || normalized.starts_with(&format!("{marker} ")))
}

fn validate_date_order(
    case: &SemanticCase,
    start_ids: &[&str],
    end_ids: &[&str],
    message: &str,
    errors: &mut Vec<(String, String)>,
) {
    let Some(start_id) = first_present_field(case, start_ids) else {
        return;
    };
    let Some(end_id) = first_present_field(case, end_ids) else {
        return;
    };
    let Some(start) = case.get(start_id).and_then(parse_supported_date) else {
        return;
    };
    let Some(end) = case.get(end_id).and_then(parse_supported_date) else {
        return;
    };
    if end < start {
        errors.push((end_id.to_string(), message.to_string()));
    }
}

fn first_present_field<'a>(case: &SemanticCase, ids: &'a [&str]) -> Option<&'a str> {
    ids.iter()
        .copied()
        .find(|field_id| case.get(field_id).is_some())
}

fn parse_supported_date(value: &str) -> Option<NaiveDate> {
    let value = value.trim();
    ["%d.%m.%Y", "%Y-%m-%d", "%d/%m/%Y"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(value, format).ok())
}

fn validate_decimal(value: &str, field_id: &str) -> Result<(), String> {
    let normalized = value.replace(['\u{00a0}', ' '], "").replace(',', ".");
    let number = normalized
        .parse::<f64>()
        .map_err(|_| format!("{field_id}: ожидается корректное числовое значение"))?;
    if !number.is_finite() || number.abs() > 1.0e15 {
        return Err(format!("{field_id}: число выходит за допустимый диапазон"));
    }
    Ok(())
}

fn validate_email(value: &str) -> Result<(), String> {
    if value.len() > 254
        || value.chars().any(char::is_whitespace)
        || value.matches('@').count() != 1
    {
        return Err("Email: некорректный адрес".into());
    }
    let (local, domain) = value
        .split_once('@')
        .ok_or_else(|| "Email: отсутствует символ @".to_string())?;
    if local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '.' | '!'
                        | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '*'
                        | '+'
                        | '-'
                        | '/'
                        | '='
                        | '?'
                        | '^'
                        | '_'
                        | '`'
                        | '{'
                        | '|'
                        | '}'
                        | '~'
                )
        })
    {
        return Err("Email: некорректная локальная часть".into());
    }
    let labels = domain.split('.').collect::<Vec<_>>();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
        || labels
            .last()
            .is_none_or(|label| label.len() < 2 || !label.chars().any(|c| c.is_ascii_alphabetic()))
    {
        return Err("Email: некорректный домен".into());
    }
    Ok(())
}

fn only_digits(v: &str) -> String {
    v.chars().filter(char::is_ascii_digit).collect()
}

fn normalize_digit_text(v: &str, title: &str) -> Result<String, String> {
    let value = v.trim();
    if value.chars().any(|character| {
        !character.is_ascii_digit() && !matches!(character, ' ' | '-' | '\u{00a0}')
    }) {
        return Err(format!("{title}: допустимы только цифры, пробелы и дефисы"));
    }
    Ok(only_digits(value))
}

fn digits(v: &str, len: usize, title: &str) -> Result<(), String> {
    normalized_digits(v, len, title).map(|_| ())
}

pub fn validate_snils(v: &str) -> Result<(), String> {
    let d = normalized_digits(v, 11, "СНИЛС")?;
    let nums = d
        .chars()
        .map(|c| {
            c.to_digit(10)
                .ok_or_else(|| "СНИЛС: неверный формат".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sum: u32 = nums[..9]
        .iter()
        .enumerate()
        .map(|(i, n)| n * (9 - i) as u32)
        .sum();
    let check = if sum < 100 {
        sum
    } else if sum == 100 || sum == 101 {
        0
    } else {
        let remainder = sum % 101;
        if remainder == 100 {
            0
        } else {
            remainder
        }
    };
    let actual = nums[9] * 10 + nums[10];
    if check == actual {
        Ok(())
    } else {
        Err("СНИЛС: неверное контрольное число".into())
    }
}

pub fn validate_inn(v: &str) -> Result<(), String> {
    let digits = normalize_digit_text(v, "ИНН")?;
    let numbers = digits
        .bytes()
        .map(|byte| u32::from(byte - b'0'))
        .collect::<Vec<_>>();
    if matches!(numbers.len(), 10 | 12)
        && numbers
            .first()
            .is_some_and(|first| numbers.iter().all(|value| value == first))
    {
        return Err("ИНН: фиктивная последовательность одинаковых цифр недопустима".into());
    }
    let checksum = |weights: &[u32], values: &[u32]| -> u32 {
        weights
            .iter()
            .zip(values)
            .map(|(weight, value)| weight * value)
            .sum::<u32>()
            % 11
            % 10
    };
    match numbers.len() {
        10 => {
            let expected = checksum(&[2, 4, 10, 3, 5, 9, 4, 6, 8], &numbers[..9]);
            if expected == numbers[9] {
                Ok(())
            } else {
                Err("ИНН: неверное контрольное число".into())
            }
        }
        12 => {
            let first = checksum(&[7, 2, 4, 10, 3, 5, 9, 4, 6, 8], &numbers[..10]);
            let second = checksum(&[3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8], &numbers[..11]);
            if first == numbers[10] && second == numbers[11] {
                Ok(())
            } else {
                Err("ИНН: неверное контрольное число".into())
            }
        }
        _ => Err("ИНН: ожидается 10 или 12 цифр".into()),
    }
}

pub fn validate_ogrn(v: &str) -> Result<(), String> {
    let d = normalize_digit_text(v, "ОГРН/ОГРНИП")?;
    match d.len() {
        13 => {
            let base: u128 = d[..12]
                .parse()
                .map_err(|_| "ОГРН: неверный формат".to_string())?;
            let expected = (base % 11 % 10) as u8;
            let actual = d[12..]
                .parse::<u8>()
                .map_err(|_| "ОГРН: неверный формат".to_string())?;
            if expected == actual {
                Ok(())
            } else {
                Err("ОГРН: неверное контрольное число".into())
            }
        }
        15 => {
            let base: u128 = d[..14]
                .parse()
                .map_err(|_| "ОГРНИП: неверный формат".to_string())?;
            let expected = (base % 13 % 10) as u8;
            let actual = d[14..]
                .parse::<u8>()
                .map_err(|_| "ОГРНИП: неверный формат".to_string())?;
            if expected == actual {
                Ok(())
            } else {
                Err("ОГРНИП: неверное контрольное число".into())
            }
        }
        _ => Err("ОГРН/ОГРНИП: ожидается 13 или 15 цифр".into()),
    }
}

pub fn validate_kpp(v: &str) -> Result<(), String> {
    let s = v.trim();
    if s.len() == 9
        && s.chars().enumerate().all(|(i, c)| {
            if !(4..=5).contains(&i) {
                c.is_ascii_digit()
            } else {
                c.is_ascii_alphanumeric()
            }
        })
    {
        Ok(())
    } else {
        Err("КПП: неверный формат".into())
    }
}

pub fn validate_cadastral(v: &str) -> Result<(), String> {
    let parts: Vec<_> = v.trim().split(':').collect();
    if parts.len() == 4
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    {
        Ok(())
    } else {
        Err("Кадастровый номер: ожидается XX:XX:XXXXXXX:XXX".into())
    }
}

/// ISO 3779 VIN validation.
///
/// The 17-character structure and forbidden letters are universal. The weighted
/// check digit in position 9 is mandatory for North-American WMIs (first symbol
/// `1`..`5`), but many valid European VINs intentionally use another symbol in
/// that position. Enforcing the checksum globally would discard real contracts.
pub fn validate_vin(v: &str) -> Result<(), String> {
    let vin = v.trim().to_ascii_uppercase();
    if vin.len() != 17
        || !vin
            .chars()
            .all(|c| c.is_ascii_alphanumeric() && !matches!(c, 'I' | 'O' | 'Q'))
    {
        return Err("VIN: ожидается 17 символов без I, O, Q".into());
    }

    let checksum_required = vin
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '1' | '2' | '3' | '4' | '5'));
    if !checksum_required {
        return Ok(());
    }

    let weights = [8_u32, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
    let mut sum = 0_u32;
    for (index, ch) in vin.chars().enumerate() {
        let value = vin_value(ch).ok_or_else(|| "VIN: недопустимый символ".to_string())?;
        sum += value * weights[index];
    }
    let remainder = sum % 11;
    let expected = if remainder == 10 {
        'X'
    } else {
        char::from_digit(remainder, 10)
            .ok_or_else(|| "VIN: ошибка контрольной цифры".to_string())?
    };
    let actual = vin
        .chars()
        .nth(8)
        .ok_or_else(|| "VIN: отсутствует контрольная цифра".to_string())?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "VIN: неверная контрольная цифра для WMI 1–5, ожидается {expected}"
        ))
    }
}

fn vin_value(ch: char) -> Option<u32> {
    match ch {
        '0'..='9' => ch.to_digit(10),
        'A' | 'J' => Some(1),
        'B' | 'K' | 'S' => Some(2),
        'C' | 'L' | 'T' => Some(3),
        'D' | 'M' | 'U' => Some(4),
        'E' | 'N' | 'V' => Some(5),
        'F' | 'W' => Some(6),
        'G' | 'P' | 'X' => Some(7),
        'H' | 'Y' => Some(8),
        'R' | 'Z' => Some(9),
        _ => None,
    }
}

pub fn validate_bank_account_with_bik(bik: &str, account: &str) -> Result<(), String> {
    let bik = normalized_digits(bik, 9, "БИК")?;
    let account = normalized_digits(account, 20, "Расчётный счёт")?;
    let control = format!("{}{}", &bik[6..9], account);
    if weighted_bank_checksum(&control) == 0 {
        Ok(())
    } else {
        Err("Расчётный счёт: контрольный ключ не соответствует БИК".into())
    }
}

pub fn validate_corr_account_with_bik(bik: &str, account: &str) -> Result<(), String> {
    let bik = normalized_digits(bik, 9, "БИК")?;
    let account = normalized_digits(account, 20, "Корреспондентский счёт")?;
    let control = format!("0{}{}", &bik[4..6], account);
    if weighted_bank_checksum(&control) == 0 {
        Ok(())
    } else {
        Err("Корреспондентский счёт: контрольный ключ не соответствует БИК".into())
    }
}

fn normalized_digits(value: &str, expected_len: usize, title: &str) -> Result<String, String> {
    let digits = normalize_digit_text(value, title)?;
    if digits.len() == expected_len {
        Ok(digits)
    } else {
        Err(format!("{title}: ожидается {expected_len} цифр"))
    }
}

fn weighted_bank_checksum(value: &str) -> u32 {
    const WEIGHTS: [u32; 3] = [7, 1, 3];
    value
        .bytes()
        .enumerate()
        .map(|(index, byte)| u32::from(byte - b'0') * WEIGHTS[index % 3])
        .sum::<u32>()
        % 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    #[test]
    fn inn_checksum_is_enforced_for_manual_input() {
        assert!(validate_field_value("org.inn", "7707083893").is_ok());
        assert!(validate_field_value("org.inn", "500100732259").is_ok());
        assert!(validate_field_value("org.inn", "7707083894").is_err());
        assert!(validate_field_value("org.inn", "500100732258").is_err());
        assert!(validate_field_value("org.inn", "7707abc083893").is_err());
        assert!(validate_field_value("org.inn", "0000000000").is_err());
        assert!(validate_field_value("org.inn", "111111111111").is_err());
    }

    #[test]
    fn lowercase_kpp_letters_are_accepted_without_weakening_structure() {
        assert!(validate_kpp("7704ab001").is_ok());
        assert!(validate_kpp("77-4ab001").is_err());
    }

    #[test]
    fn valid_snils() {
        assert!(validate_snils("112-233-445 95").is_ok());
    }

    #[test]
    fn cadastral() {
        assert!(validate_cadastral("52:18:0030248:156").is_ok());
    }

    #[test]
    fn vin_checksum_is_enforced_only_where_it_is_mandatory() {
        assert!(validate_vin("1M8GDM9AXKP042788").is_ok());
        assert!(validate_vin("1M8GDM9A1KP042788").is_err());
        assert!(validate_vin("WVWZZZ1JZ3W386752").is_ok());
    }

    #[test]
    fn bank_accounts_are_checked_against_bik() {
        let bik = "044525225";
        assert!(validate_bank_account_with_bik(bik, "40702810900000002859").is_ok());
        assert!(validate_bank_account_with_bik(bik, "40702810900000002851").is_err());
        assert!(validate_corr_account_with_bik(bik, "30101810400000000225").is_ok());
    }

    #[test]
    fn generic_dates_decimals_and_email_are_validated() {
        assert!(validate_field_value("document.issue_date", "16.07.2026").is_ok());
        assert!(validate_field_value("document.issue_date", "2026-07-16").is_ok());
        assert!(validate_field_value("document.issue_date", "31.02.2026").is_err());
        assert!(validate_field_value("invoice.total_amount", "12 345,67").is_ok());
        assert!(validate_field_value("invoice.total_amount", "NaN").is_err());
        assert!(validate_field_value("employee.salary", "150 000,50").is_ok());
        assert!(validate_field_value("employee.salary", "сто тысяч").is_err());
        assert!(validate_field_value("amount.currency", "RUB").is_ok());
        assert!(validate_field_value("accounting.currency", "USD").is_ok());
        assert!(validate_field_value("custom.amount_description", "По договору").is_ok());
        assert!(validate_field_value("contact.email", "doctor@example.org").is_ok());
        assert!(validate_field_value("contact.email", "not-an-email").is_err());
        assert!(validate_field_value("contact.email", "a@b@c.example").is_err());
        assert!(validate_field_value("contact.email", ".doctor@example.org").is_err());
        assert!(validate_field_value("contact.email", "doctor@example.c").is_err());
    }

    #[test]
    fn digit_identifiers_reject_hidden_letters_and_symbols() {
        for (field, value) in [
            ("org.bank_bik", "04452x5225"),
            ("org.bank_account", "4070281090000000285x9"),
            ("org.bank_corr_account", "3010181040000000022/5"),
            ("subject.passport_series", "12a34"),
            ("subject.passport_number", "12345x6"),
        ] {
            assert!(
                validate_field_value(field, value).is_err(),
                "{field}: {value}"
            );
        }
        assert!(validate_snils("112-233-445x95").is_err());
        assert!(validate_ogrn("1027700132195x").is_err());
    }

    #[test]
    fn relation_validation_blocks_reversed_dates_and_future_birth() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("medical.admission_date", "16.07.2026"),
            ("medical.discharge_date", "15.07.2026"),
            ("subject.birth_date", "17.07.2026"),
            ("document.date", "16.07.2026"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::Scanner, 1.0),
            );
        }
        let errors = validate_case_relations(&case);
        assert!(errors.iter().any(|(id, _)| id == "medical.discharge_date"));
        assert!(errors.iter().any(|(id, _)| id == "subject.birth_date"));
    }

    #[test]
    fn relation_validation_points_to_the_invalid_account() {
        let mut case = SemanticCase::default();
        for (id, value) in [
            ("org.bank_bik", "044525225"),
            ("org.bank_account", "40702810900000002851"),
        ] {
            case.values.insert(
                id.into(),
                SemanticValue::new(id, value, ValueSource::Scanner, 1.0),
            );
        }
        let errors = validate_case_relations(&case);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "org.bank_account");
    }
}
