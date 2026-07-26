//! Small, deterministic, offline Russian morphology/formatting primitives.
//! This is deliberately dependency-light and conservative: uncertain words are left unchanged.
use chrono::{Datelike, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammaticalCase {
    Nominative,
    Genitive,
    Dative,
    Accusative,
    Instrumental,
    Prepositional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersonGender {
    Male,
    Female,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersonNamePart {
    Surname,
    GivenName,
    Patronymic,
    Unknown,
}

pub fn decline_person_name(value: &str, case: GrammaticalCase) -> String {
    let value = value.trim();
    if case == GrammaticalCase::Nominative || value.is_empty() {
        return value.to_string();
    }

    let words = value.split_whitespace().collect::<Vec<_>>();
    let parts = classify_person_name_parts(&words);
    let gender = detect_person_gender(&words, &parts);
    words
        .iter()
        .zip(parts.iter().copied())
        .map(|(word, part)| decline_person_part(word, part, gender, case))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn decline_position(value: &str, case: GrammaticalCase) -> String {
    if case == GrammaticalCase::Nominative {
        return value.trim().to_string();
    }
    value
        .split_whitespace()
        .map(|word| decline_position_word(word, case))
        .collect::<Vec<_>>()
        .join(" ")
}

fn replace_core(original: &str, core: &str, changed: String) -> String {
    let changed = if core.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
        changed.to_uppercase()
    } else if core.chars().next().is_some_and(char::is_uppercase) {
        let mut chars = changed.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or(changed)
    } else {
        changed
    };
    original.replacen(core, &changed, 1)
}

fn person_core(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphabetic() && c != '-')
}

fn classify_person_name_parts(words: &[&str]) -> Vec<PersonNamePart> {
    match words.len() {
        0 => Vec::new(),
        1 => vec![PersonNamePart::Unknown],
        2 => {
            let first = person_core(words[0]).to_lowercase();
            let second = person_core(words[1]).to_lowercase();
            let first_is_name = is_likely_given_name(&first);
            let second_is_name = is_likely_given_name(&second);
            if first_is_name && !second_is_name {
                vec![PersonNamePart::GivenName, PersonNamePart::Surname]
            } else if (second_is_name && !first_is_name)
                || (looks_like_surname(&first) && !looks_like_surname(&second))
            {
                vec![PersonNamePart::Surname, PersonNamePart::GivenName]
            } else {
                // Do not invent an order for an ambiguous two-token name.  Both
                // components remain conservative and uncertain endings are left
                // unchanged by the generic declensor.
                vec![PersonNamePart::Unknown, PersonNamePart::Unknown]
            }
        }
        len => {
            let mut parts = vec![PersonNamePart::Unknown; len];
            let first = person_core(words[0]).to_lowercase();
            let second = person_core(words[1]).to_lowercase();
            if is_likely_given_name(&first)
                && looks_like_surname(&second)
                && is_patronymic(person_core(words[2]))
            {
                parts[0] = PersonNamePart::GivenName;
                parts[1] = PersonNamePart::Surname;
            } else {
                parts[0] = PersonNamePart::Surname;
                parts[1] = PersonNamePart::GivenName;
            }
            parts[2] = PersonNamePart::Patronymic;
            parts
        }
    }
}

fn detect_person_gender(words: &[&str], parts: &[PersonNamePart]) -> PersonGender {
    for (word, part) in words.iter().zip(parts.iter()) {
        if *part != PersonNamePart::Patronymic {
            continue;
        }
        let lower = person_core(word).to_lowercase();
        if lower.ends_with("овна")
            || lower.ends_with("евна")
            || lower.ends_with("ична")
            || lower.ends_with("инична")
        {
            return PersonGender::Female;
        }
        if lower.ends_with("ович") || lower.ends_with("евич") || lower.ends_with("ич") {
            return PersonGender::Male;
        }
    }

    for (word, part) in words.iter().zip(parts.iter()) {
        if *part != PersonNamePart::GivenName {
            continue;
        }
        let given = person_core(word).to_lowercase();
        if is_known_female_name(&given)
            || given.ends_with('а')
            || given.ends_with('я')
            || given == "любовь"
        {
            return PersonGender::Female;
        }
        if is_known_male_name(&given)
            || given
                .chars()
                .last()
                .is_some_and(|ch| ch.is_alphabetic() && !"аеёиоуыэюя".contains(ch))
        {
            return PersonGender::Male;
        }
    }
    PersonGender::Unknown
}

fn is_patronymic(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.ends_with("овна")
        || lower.ends_with("евна")
        || lower.ends_with("ична")
        || lower.ends_with("инична")
        || lower.ends_with("ович")
        || lower.ends_with("евич")
}

fn looks_like_surname(value: &str) -> bool {
    let lower = value.to_lowercase();
    [
        "ов", "ев", "ёв", "ин", "ын", "ова", "ева", "ёва", "ина", "ына", "ский", "цкий", "ская",
        "цкая", "енко", "ко", "ук", "юк", "ич", "вич",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

fn is_likely_given_name(value: &str) -> bool {
    is_known_female_name(value) || is_known_male_name(value)
}

fn is_known_female_name(value: &str) -> bool {
    matches!(
        value,
        "мария"
            | "софия"
            | "лидия"
            | "юлия"
            | "наталья"
            | "татьяна"
            | "ольга"
            | "любовь"
            | "надежда"
            | "вера"
            | "анна"
            | "елена"
            | "ирина"
            | "екатерина"
            | "александра"
            | "анастасия"
            | "валентина"
            | "виктория"
            | "галина"
            | "дарья"
            | "евгения"
            | "людмила"
            | "марина"
            | "нина"
            | "светлана"
    )
}

fn is_known_male_name(value: &str) -> bool {
    matches!(
        value,
        "иван"
            | "александр"
            | "алексей"
            | "анатолий"
            | "андрей"
            | "антон"
            | "борис"
            | "виктор"
            | "владимир"
            | "дмитрий"
            | "евгений"
            | "максим"
            | "михаил"
            | "николай"
            | "олег"
            | "роман"
            | "сергей"
            | "илья"
            | "кузьма"
            | "никита"
            | "фома"
            | "лука"
            | "савва"
            | "лев"
            | "павел"
            | "петр"
            | "пётр"
    )
}

fn decline_person_part(
    word: &str,
    part: PersonNamePart,
    gender: PersonGender,
    case: GrammaticalCase,
) -> String {
    let core = person_core(word);
    if core.len() < 2 {
        return word.to_string();
    }
    if core.contains('-') {
        let declined = core
            .split('-')
            .map(|piece| decline_person_part(piece, part, gender, case))
            .collect::<Vec<_>>()
            .join("-");
        return replace_core(word, core, declined);
    }

    let lower = core.to_lowercase();
    let changed = match part {
        PersonNamePart::Surname => decline_surname_core(&lower, gender, case),
        PersonNamePart::GivenName => decline_given_name_core(&lower, gender, case),
        PersonNamePart::Patronymic => decline_patronymic_core(&lower, gender, case),
        PersonNamePart::Unknown => decline_generic_name_core(&lower, gender, case),
    };
    replace_core(word, core, changed)
}

fn decline_surname_core(value: &str, gender: PersonGender, case: GrammaticalCase) -> String {
    if matches!(value, "толстой") {
        return decline_masculine_adjective(value, case);
    }
    if value.ends_with("ский") || value.ends_with("цкий") {
        return if gender == PersonGender::Female {
            value.to_string()
        } else {
            decline_masculine_adjective(value, case)
        };
    }
    if value.ends_with("ская") || value.ends_with("цкая") {
        return decline_feminine_adjective(value, case);
    }
    if value.ends_with("ая") || value.ends_with("яя") {
        return decline_feminine_adjective(value, case);
    }
    if value.ends_with("ый") || value.ends_with("ой") || value.ends_with("ий") {
        return if gender == PersonGender::Female {
            value.to_string()
        } else {
            decline_masculine_adjective(value, case)
        };
    }
    if value.ends_with("ова")
        || value.ends_with("ева")
        || value.ends_with("ина")
        || value.ends_with("ына")
    {
        return decline_feminine_surname(value, case);
    }
    if value.ends_with("ов")
        || value.ends_with("ев")
        || value.ends_with("ин")
        || value.ends_with("ын")
    {
        return if gender == PersonGender::Female {
            value.to_string()
        } else {
            decline_masculine_consonant(value, case, true)
        };
    }
    if value.ends_with("енко")
        || value.ends_with("ко")
        || value.ends_with("их")
        || value.ends_with("ых")
        || value.ends_with("аго")
        || value.ends_with("ово")
    {
        return value.to_string();
    }
    if gender == PersonGender::Female
        && value
            .chars()
            .last()
            .is_some_and(|ch| ch.is_alphabetic() && !"аеёиоуыэюя".contains(ch))
    {
        return value.to_string();
    }
    decline_generic_name_core(value, gender, case)
}

fn decline_given_name_core(value: &str, gender: PersonGender, case: GrammaticalCase) -> String {
    match value {
        "мария" | "софия" | "лидия" | "юлия" => {
            let stem = value.strip_suffix("ия").unwrap_or(value);
            return match case {
                GrammaticalCase::Genitive
                | GrammaticalCase::Dative
                | GrammaticalCase::Prepositional => format!("{stem}ии"),
                GrammaticalCase::Accusative => format!("{stem}ию"),
                GrammaticalCase::Instrumental => format!("{stem}ией"),
                _ => value.to_string(),
            };
        }
        "наталья" | "дарья" | "марья" => {
            let stem = value.strip_suffix('я').unwrap_or(value);
            return match case {
                GrammaticalCase::Genitive => format!("{stem}и"),
                GrammaticalCase::Dative | GrammaticalCase::Prepositional => {
                    format!("{stem}е")
                }
                GrammaticalCase::Accusative => format!("{stem}ю"),
                GrammaticalCase::Instrumental => format!("{stem}ей"),
                _ => value.to_string(),
            };
        }
        "любовь" => {
            return match case {
                GrammaticalCase::Genitive
                | GrammaticalCase::Dative
                | GrammaticalCase::Prepositional => "любови".into(),
                GrammaticalCase::Accusative => "любовь".into(),
                GrammaticalCase::Instrumental => "любовью".into(),
                _ => value.to_string(),
            };
        }
        "илья" => {
            return match case {
                GrammaticalCase::Genitive => "ильи".into(),
                GrammaticalCase::Dative | GrammaticalCase::Prepositional => "илье".into(),
                GrammaticalCase::Accusative => "илью".into(),
                GrammaticalCase::Instrumental => "ильёй".into(),
                _ => value.to_string(),
            };
        }
        "павел" => {
            return match case {
                GrammaticalCase::Genitive | GrammaticalCase::Accusative => "павла".into(),
                GrammaticalCase::Dative => "павлу".into(),
                GrammaticalCase::Instrumental => "павлом".into(),
                GrammaticalCase::Prepositional => "павле".into(),
                _ => value.to_string(),
            };
        }
        "лев" => {
            return match case {
                GrammaticalCase::Genitive | GrammaticalCase::Accusative => "льва".into(),
                GrammaticalCase::Dative => "льву".into(),
                GrammaticalCase::Instrumental => "львом".into(),
                GrammaticalCase::Prepositional => "льве".into(),
                _ => value.to_string(),
            };
        }
        _ => {}
    }
    decline_generic_name_core(value, gender, case)
}

fn decline_patronymic_core(value: &str, gender: PersonGender, case: GrammaticalCase) -> String {
    if value.ends_with("овна")
        || value.ends_with("евна")
        || value.ends_with("ична")
        || value.ends_with("инична")
    {
        return decline_feminine_a(value, case);
    }
    if value.ends_with("ович") || value.ends_with("евич") || value.ends_with("ич") {
        return match case {
            GrammaticalCase::Genitive | GrammaticalCase::Accusative => format!("{value}а"),
            GrammaticalCase::Dative => format!("{value}у"),
            GrammaticalCase::Instrumental => format!("{value}ем"),
            GrammaticalCase::Prepositional => format!("{value}е"),
            _ => value.to_string(),
        };
    }
    decline_generic_name_core(value, gender, case)
}

fn decline_generic_name_core(value: &str, gender: PersonGender, case: GrammaticalCase) -> String {
    if value.ends_with("ия") {
        let stem = value.strip_suffix("ия").unwrap_or(value);
        return match case {
            GrammaticalCase::Genitive
            | GrammaticalCase::Dative
            | GrammaticalCase::Prepositional => format!("{stem}ии"),
            GrammaticalCase::Accusative => format!("{stem}ию"),
            GrammaticalCase::Instrumental => format!("{stem}ией"),
            _ => value.to_string(),
        };
    }
    if value.ends_with('а') {
        return decline_feminine_a(value, case);
    }
    if value.ends_with('я') {
        let stem = value.strip_suffix('я').unwrap_or(value);
        return match case {
            GrammaticalCase::Genitive => format!("{stem}и"),
            GrammaticalCase::Dative | GrammaticalCase::Prepositional => format!("{stem}е"),
            GrammaticalCase::Accusative => format!("{stem}ю"),
            GrammaticalCase::Instrumental => format!("{stem}ей"),
            _ => value.to_string(),
        };
    }
    if value.ends_with("ий") || value.ends_with('й') || value.ends_with('ь') {
        if gender == PersonGender::Female && value.ends_with('ь') {
            let stem = value.strip_suffix('ь').unwrap_or(value);
            return match case {
                GrammaticalCase::Genitive
                | GrammaticalCase::Dative
                | GrammaticalCase::Prepositional => format!("{stem}и"),
                GrammaticalCase::Accusative => value.to_string(),
                GrammaticalCase::Instrumental => format!("{stem}ью"),
                _ => value.to_string(),
            };
        }
        // Personal names ending in -ий keep the preceding «и»:
        // Дмитрий -> Дмитрия, Анатолий -> Анатолия.
        let stem = value
            .strip_suffix('й')
            .or_else(|| value.strip_suffix('ь'))
            .unwrap_or(value);
        return match case {
            GrammaticalCase::Genitive | GrammaticalCase::Accusative => format!("{stem}я"),
            GrammaticalCase::Dative => format!("{stem}ю"),
            GrammaticalCase::Instrumental => format!("{stem}ем"),
            GrammaticalCase::Prepositional => format!("{stem}е"),
            _ => value.to_string(),
        };
    }
    if value
        .chars()
        .last()
        .is_some_and(|ch| ch.is_alphabetic() && !"аеёиоуыэюя".contains(ch))
        && !value.ends_with("ых")
        && !value.ends_with("их")
    {
        if gender == PersonGender::Female {
            return value.to_string();
        }
        return decline_masculine_consonant(value, case, false);
    }
    value.to_string()
}

fn decline_feminine_surname(value: &str, case: GrammaticalCase) -> String {
    let stem = value.strip_suffix('а').unwrap_or(value);
    match case {
        GrammaticalCase::Genitive
        | GrammaticalCase::Dative
        | GrammaticalCase::Instrumental
        | GrammaticalCase::Prepositional => format!("{stem}ой"),
        GrammaticalCase::Accusative => format!("{stem}у"),
        _ => value.to_string(),
    }
}

fn decline_feminine_a(value: &str, case: GrammaticalCase) -> String {
    let stem = value.strip_suffix('а').unwrap_or(value);
    match case {
        GrammaticalCase::Genitive => format!("{stem}{}", genitive_a_ending(stem)),
        GrammaticalCase::Dative | GrammaticalCase::Prepositional => format!("{stem}е"),
        GrammaticalCase::Accusative => format!("{stem}у"),
        GrammaticalCase::Instrumental => format!("{stem}ой"),
        _ => value.to_string(),
    }
}

fn genitive_a_ending(stem: &str) -> &'static str {
    if stem
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, 'г' | 'к' | 'х' | 'ж' | 'ч' | 'ш' | 'щ'))
    {
        "и"
    } else {
        "ы"
    }
}

fn decline_masculine_consonant(
    value: &str,
    case: GrammaticalCase,
    adjective_instrumental: bool,
) -> String {
    match case {
        GrammaticalCase::Genitive | GrammaticalCase::Accusative => format!("{value}а"),
        GrammaticalCase::Dative => format!("{value}у"),
        GrammaticalCase::Instrumental => {
            format!(
                "{value}{}",
                if adjective_instrumental {
                    "ым"
                } else {
                    "ом"
                }
            )
        }
        GrammaticalCase::Prepositional => format!("{value}е"),
        _ => value.to_string(),
    }
}

fn decline_masculine_adjective(value: &str, case: GrammaticalCase) -> String {
    let (stem, soft) = if let Some(stem) = value.strip_suffix("ий") {
        (stem, true)
    } else if let Some(stem) = value.strip_suffix("ый") {
        (stem, false)
    } else if let Some(stem) = value.strip_suffix("ой") {
        (stem, false)
    } else {
        return value.to_string();
    };
    match case {
        GrammaticalCase::Genitive | GrammaticalCase::Accusative => {
            format!("{stem}{}", if soft { "его" } else { "ого" })
        }
        GrammaticalCase::Dative => format!("{stem}{}", if soft { "ему" } else { "ому" }),
        GrammaticalCase::Instrumental => {
            format!("{stem}{}", if soft { "им" } else { "ым" })
        }
        GrammaticalCase::Prepositional => format!("{stem}{}", if soft { "ем" } else { "ом" }),
        _ => value.to_string(),
    }
}

fn decline_feminine_adjective(value: &str, case: GrammaticalCase) -> String {
    let (stem, soft) = if let Some(stem) = value.strip_suffix("яя") {
        (stem, true)
    } else if let Some(stem) = value.strip_suffix("ая") {
        (stem, false)
    } else {
        return value.to_string();
    };
    match case {
        GrammaticalCase::Genitive
        | GrammaticalCase::Dative
        | GrammaticalCase::Instrumental
        | GrammaticalCase::Prepositional => {
            format!("{stem}{}", if soft { "ей" } else { "ой" })
        }
        GrammaticalCase::Accusative => {
            format!("{stem}{}", if soft { "юю" } else { "ую" })
        }
        _ => value.to_string(),
    }
}

fn decline_name_word(word: &str, case: GrammaticalCase) -> String {
    decline_person_part(word, PersonNamePart::Unknown, PersonGender::Unknown, case)
}
fn decline_position_word(word: &str, case: GrammaticalCase) -> String {
    let core = word.trim_matches(|c: char| !c.is_alphabetic() && c != '-');
    if core.is_empty() {
        return word.to_string();
    }
    if core.contains('-') {
        let declined = core
            .split('-')
            .map(|part| decline_position_word(part, case))
            .collect::<Vec<_>>()
            .join("-");
        return replace_core(word, core, declined);
    }
    let l = core.to_lowercase();
    let changed = if l.ends_with("ый") || l.ends_with("ой") {
        let st = if l.ends_with("ый") {
            l.strip_suffix("ый").unwrap_or(&l)
        } else {
            l.strip_suffix("ой").unwrap_or(&l)
        };
        match case {
            GrammaticalCase::Genitive | GrammaticalCase::Accusative => format!("{st}ого"),
            GrammaticalCase::Dative => format!("{st}ому"),
            GrammaticalCase::Instrumental => format!("{st}ым"),
            GrammaticalCase::Prepositional => format!("{st}ом"),
            _ => l,
        }
    } else if l.ends_with("ий") {
        let st = l.trim_end_matches("ий");
        match case {
            GrammaticalCase::Genitive | GrammaticalCase::Accusative => format!("{st}его"),
            GrammaticalCase::Dative => format!("{st}ему"),
            GrammaticalCase::Instrumental => format!("{st}им"),
            GrammaticalCase::Prepositional => format!("{st}ем"),
            _ => l,
        }
    } else if l.ends_with("ая") {
        let st = l.trim_end_matches("ая");
        match case {
            GrammaticalCase::Genitive
            | GrammaticalCase::Dative
            | GrammaticalCase::Instrumental
            | GrammaticalCase::Prepositional => format!("{st}ой"),
            GrammaticalCase::Accusative => format!("{st}ую"),
            _ => l,
        }
    } else if l.ends_with("яя") {
        let st = l.trim_end_matches("яя");
        match case {
            GrammaticalCase::Genitive
            | GrammaticalCase::Dative
            | GrammaticalCase::Instrumental
            | GrammaticalCase::Prepositional => format!("{st}ей"),
            GrammaticalCase::Accusative => format!("{st}юю"),
            _ => l,
        }
    } else if l.ends_with("ые") || l.ends_with("ие") {
        let soft = l.ends_with("ие");
        let st = if soft {
            l.trim_end_matches("ие")
        } else {
            l.trim_end_matches("ые")
        };
        match case {
            GrammaticalCase::Genitive
            | GrammaticalCase::Accusative
            | GrammaticalCase::Prepositional => format!("{st}{}", if soft { "их" } else { "ых" }),
            GrammaticalCase::Dative => format!("{st}{}", if soft { "им" } else { "ым" }),
            GrammaticalCase::Instrumental => {
                format!("{st}{}", if soft { "ими" } else { "ыми" })
            }
            _ => l,
        }
    } else if l.ends_with("ия") {
        let st = l.trim_end_matches("ия");
        match case {
            GrammaticalCase::Genitive
            | GrammaticalCase::Dative
            | GrammaticalCase::Prepositional => format!("{st}ии"),
            GrammaticalCase::Accusative => format!("{st}ию"),
            GrammaticalCase::Instrumental => format!("{st}ией"),
            _ => l,
        }
    } else {
        return decline_name_word(word, case);
    };
    replace_core(word, core, changed)
}

const ONES_M: [&str; 10] = [
    "",
    "один",
    "два",
    "три",
    "четыре",
    "пять",
    "шесть",
    "семь",
    "восемь",
    "девять",
];
const ONES_F: [&str; 10] = [
    "",
    "одна",
    "две",
    "три",
    "четыре",
    "пять",
    "шесть",
    "семь",
    "восемь",
    "девять",
];
const TEENS: [&str; 10] = [
    "десять",
    "одиннадцать",
    "двенадцать",
    "тринадцать",
    "четырнадцать",
    "пятнадцать",
    "шестнадцать",
    "семнадцать",
    "восемнадцать",
    "девятнадцать",
];
const TENS: [&str; 10] = [
    "",
    "",
    "двадцать",
    "тридцать",
    "сорок",
    "пятьдесят",
    "шестьдесят",
    "семьдесят",
    "восемьдесят",
    "девяносто",
];
const HUNDREDS: [&str; 10] = [
    "",
    "сто",
    "двести",
    "триста",
    "четыреста",
    "пятьсот",
    "шестьсот",
    "семьсот",
    "восемьсот",
    "девятьсот",
];
fn plural(n: u16, forms: [&str; 3]) -> &str {
    let n100 = n % 100;
    let n10 = n % 10;
    if (11..=14).contains(&n100) {
        forms[2]
    } else if n10 == 1 {
        forms[0]
    } else if (2..=4).contains(&n10) {
        forms[1]
    } else {
        forms[2]
    }
}
fn group_words(n: u16, feminine: bool, rank: usize) -> String {
    let mut p = Vec::new();
    let h = n / 100;
    if h > 0 {
        p.push(HUNDREDS[h as usize]);
    }
    let r = n % 100;
    if (10..=19).contains(&r) {
        p.push(TEENS[(r - 10) as usize]);
    } else {
        let t = r / 10;
        let o = r % 10;
        if t > 0 {
            p.push(TENS[t as usize]);
        }
        if o > 0 {
            p.push(if feminine {
                ONES_F[o as usize]
            } else {
                ONES_M[o as usize]
            });
        }
    }
    if rank > 0 {
        let forms = match rank {
            1 => ["тысяча", "тысячи", "тысяч"],
            2 => ["миллион", "миллиона", "миллионов"],
            3 => ["миллиард", "миллиарда", "миллиардов"],
            4 => ["триллион", "триллиона", "триллионов"],
            5 => ["квадриллион", "квадриллиона", "квадриллионов"],
            6 => ["квинтиллион", "квинтиллиона", "квинтиллионов"],
            _ => ["секстиллион", "секстиллиона", "секстиллионов"],
        };
        p.push(plural(n, forms));
    }
    p.join(" ")
}
pub fn number_to_words_ru(value: i64) -> String {
    if value == 0 {
        return "ноль".into();
    }
    let neg = value < 0;
    let mut n = value.unsigned_abs();
    let mut groups = Vec::new();
    let mut rank = 0;
    while n > 0 {
        let g = (n % 1000) as u16;
        if g > 0 {
            groups.push(group_words(g, rank == 1, rank));
        }
        n /= 1000;
        rank += 1;
    }
    groups.reverse();
    let s = groups.join(" ");
    if neg {
        format!("минус {s}")
    } else {
        s
    }
}
pub fn money_to_words_ru(kopecks: i64) -> String {
    let neg = kopecks < 0;
    let abs = kopecks.unsigned_abs();
    let rub = (abs / 100) as i64;
    let kop = (abs % 100) as u16;
    let prefix = if neg { "минус " } else { "" };
    let rub_words = number_to_words_ru(rub);
    let rub_words = if neg {
        rub_words
    } else {
        capitalize(&rub_words)
    };
    format!(
        "{prefix}{rub_words} {} {:02} {}",
        plural(
            (rub.unsigned_abs() % 1000) as u16,
            ["рубль", "рубля", "рублей"]
        ),
        kop,
        plural(kop, ["копейка", "копейки", "копеек"])
    )
}
pub fn format_money_ru(kopecks: i64) -> String {
    let neg = kopecks < 0;
    let abs = kopecks.unsigned_abs();
    let rub = abs / 100;
    let mut digits = rub.to_string();
    let mut out = String::new();
    while digits.len() > 3 {
        let tail = digits.split_off(digits.len() - 3);
        if out.is_empty() {
            out = tail
        } else {
            out = format!("{tail} {out}")
        }
    }
    if out.is_empty() {
        out = digits
    } else {
        out = format!("{digits} {out}")
    }
    format!("{}{out},{:02}", if neg { "-" } else { "" }, abs % 100)
}
pub fn format_phone_ru(raw: &str) -> String {
    let d: Vec<char> = raw.chars().filter(char::is_ascii_digit).collect();
    if d.len() == 11 && (d[0] == '7' || d[0] == '8') {
        format!(
            "+7 ({}) {}-{}-{}",
            d[1..4].iter().collect::<String>(),
            d[4..7].iter().collect::<String>(),
            d[7..9].iter().collect::<String>(),
            d[9..11].iter().collect::<String>()
        )
    } else {
        raw.trim().to_string()
    }
}
pub fn date_to_words_ru(date: NaiveDate) -> String {
    const M: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    format!(
        "{} {} {} года",
        date.day(),
        M[date.month0() as usize],
        date.year()
    )
}
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn money_words() {
        assert!(money_to_words_ru(14_650_000).contains("Сто сорок шесть тысяч пятьсот рублей"));
    }
    #[test]
    fn large_integer_ranks_are_not_repeated_as_trillions() {
        let words = number_to_words_ru(i64::MIN);
        assert!(words.contains("квинтиллионов"), "{words}");
        assert!(words.contains("квадриллиона"), "{words}");
        assert!(words.contains("триллиона"), "{words}");
        assert_eq!(words.matches("квинтиллион").count(), 1, "{words}");
        assert_eq!(words.matches("квадриллион").count(), 1, "{words}");
        assert_eq!(words.matches("триллион").count(), 1, "{words}");
    }

    #[test]
    fn negative_money_keeps_lowercase_after_minus() {
        assert_eq!(
            money_to_words_ru(-12_345),
            "минус сто двадцать три рубля 45 копеек"
        );
    }

    #[test]
    fn surname() {
        assert_eq!(
            decline_person_name("Иванов Иван", GrammaticalCase::Genitive),
            "Иванова Ивана"
        );
    }
    #[test]
    fn female_full_name_declines_all_components() {
        assert_eq!(
            decline_person_name("Иванова Мария Петровна", GrammaticalCase::Genitive),
            "Ивановой Марии Петровны"
        );
        assert_eq!(
            decline_person_name("Иванова Мария Петровна", GrammaticalCase::Dative),
            "Ивановой Марии Петровне"
        );
    }

    #[test]
    fn masculine_given_names_ending_in_iy_keep_the_i_stem() {
        assert_eq!(
            decline_person_name("Иванов Дмитрий Сергеевич", GrammaticalCase::Genitive),
            "Иванова Дмитрия Сергеевича"
        );
        assert_eq!(
            decline_person_name("Иванов Анатолий Сергеевич", GrammaticalCase::Instrumental),
            "Ивановым Анатолием Сергеевичем"
        );
    }

    #[test]
    fn hyphenated_male_surname_declines_each_part() {
        assert_eq!(
            decline_person_name("Петров-Водкин Кузьма Сергеевич", GrammaticalCase::Genitive),
            "Петрова-Водкина Кузьмы Сергеевича"
        );
    }

    #[test]
    fn female_consonant_surname_stays_unchanged_but_name_declines() {
        assert_eq!(
            decline_person_name("Жук Мария Петровна", GrammaticalCase::Genitive),
            "Жук Марии Петровны"
        );
    }

    #[test]
    fn surname_ending_in_ich_does_not_force_male_gender() {
        assert_eq!(
            decline_person_name("Мицкевич Анна", GrammaticalCase::Genitive),
            "Мицкевич Анны"
        );
    }

    #[test]
    fn given_name_first_order_is_detected_for_two_tokens() {
        assert_eq!(
            decline_person_name("Анна Петрова", GrammaticalCase::Genitive),
            "Анны Петровой"
        );
    }

    #[test]
    fn compound_and_feminine_positions_decline_as_positions() {
        assert_eq!(
            decline_position("главный врач-терапевт", GrammaticalCase::Genitive),
            "главного врача-терапевта"
        );
        assert_eq!(
            decline_position("главная медицинская сестра", GrammaticalCase::Genitive),
            "главной медицинской сестры"
        );
    }
    #[test]
    fn phone() {
        assert_eq!(format_phone_ru("89161234567"), "+7 (916) 123-45-67");
    }
}
