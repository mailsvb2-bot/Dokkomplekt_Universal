//! Donor-compatible grammatical adaptation for medical diary text.
//!
//! This module is intentionally nested under `diary_professional_records`: it
//! must never become a universal renderer rule. Specialist-owned text stays the
//! source of truth; only known male/female word forms are adapted.

use crate::{SemanticAtom, SemanticCase, SemanticRecord};

const GENDER_WORD_PAIRS: &[(&str, &str)] = &[
    ("pacjent", "pacjentka"),
    ("pacjenta", "pacjentki"),
    ("chory", "chora"),
    ("przyjęty", "przyjęta"),
    ("przyjety", "przyjeta"),
    ("wypisany", "wypisana"),
    ("hospitalizowany", "hospitalizowana"),
    ("leczony", "leczona"),
    ("badany", "badana"),
    ("zgłaszał", "zgłaszała"),
    ("zglaszal", "zglaszala"),
    ("пациент", "пациентка"),
    ("пациента", "пациентки"),
    ("больной", "больная"),
    ("он", "она"),
    ("его", "ее"),
    ("ему", "ей"),
    ("ним", "ней"),
    ("него", "нее"),
    ("сам", "сама"),
    ("был", "была"),
    ("поступил", "поступила"),
    ("госпитализирован", "госпитализирована"),
    ("осмотрен", "осмотрена"),
    ("обследован", "обследована"),
    ("выписан", "выписана"),
    ("переведен", "переведена"),
    ("переведён", "переведена"),
    ("направлен", "направлена"),
    ("доставлен", "доставлена"),
    ("обратился", "обратилась"),
    ("находился", "находилась"),
    ("выезжал", "выезжала"),
    ("выехал", "выехала"),
    ("проживал", "проживала"),
    ("прожил", "прожила"),
    ("работал", "работала"),
    ("учился", "училась"),
    ("обучался", "обучалась"),
    ("родился", "родилась"),
    ("рожден", "рождена"),
    ("рождён", "рождена"),
    ("воспитывался", "воспитывалась"),
    ("рос", "росла"),
    ("развивался", "развивалась"),
    ("окончил", "окончила"),
    ("закончил", "закончила"),
    ("служил", "служила"),
    ("состоял", "состояла"),
    ("занимался", "занималась"),
    ("употреблял", "употребляла"),
    ("перенес", "перенесла"),
    ("перенёс", "перенесла"),
    ("оставался", "оставалась"),
    ("жаловался", "жаловалась"),
    ("отказывался", "отказывалась"),
    ("согласился", "согласилась"),
    ("успокоился", "успокоилась"),
    ("вернулся", "вернулась"),
    ("ориентировался", "ориентировалась"),
    ("смеялся", "смеялась"),
    ("раздражался", "раздражалась"),
    ("нуждался", "нуждалась"),
    ("держался", "держалась"),
    ("отмечал", "отмечала"),
    ("отметил", "отметила"),
    ("сообщал", "сообщала"),
    ("сообщил", "сообщила"),
    ("рассказал", "рассказала"),
    ("указал", "указала"),
    ("сказал", "сказала"),
    ("заявил", "заявила"),
    ("назвал", "назвала"),
    ("подтвердил", "подтвердила"),
    ("отрицал", "отрицала"),
    ("высказывал", "высказывала"),
    ("предъявлял", "предъявляла"),
    ("имел", "имела"),
    ("являлся", "являлась"),
    ("получал", "получала"),
    ("получил", "получила"),
    ("принимал", "принимала"),
    ("просил", "просила"),
    ("требовал", "требовала"),
    ("отвечал", "отвечала"),
    ("вступал", "вступала"),
    ("контактировал", "контактировала"),
    ("спал", "спала"),
    ("просыпался", "просыпалась"),
    ("засыпал", "засыпала"),
    ("ел", "ела"),
    ("пил", "пила"),
    ("съел", "съела"),
    ("выпил", "выпила"),
    ("посещал", "посещала"),
    ("участвовал", "участвовала"),
    ("выполнял", "выполняла"),
    ("соблюдал", "соблюдала"),
    ("нарушал", "нарушала"),
    ("покидал", "покидала"),
    ("лежал", "лежала"),
    ("сидел", "сидела"),
    ("стоял", "стояла"),
    ("плакал", "плакала"),
    ("сохранял", "сохраняла"),
    ("проявлял", "проявляла"),
    ("демонстрировал", "демонстрировала"),
    ("страдал", "страдала"),
    ("переносил", "переносила"),
    ("реагировал", "реагировала"),
    ("понимал", "понимала"),
    ("осознавал", "осознавала"),
    ("считал", "считала"),
    ("планировал", "планировала"),
    ("выражал", "выражала"),
    ("подписал", "подписала"),
    ("завершил", "завершила"),
    ("прошел", "прошла"),
    ("прошёл", "прошла"),
    ("шел", "шла"),
    ("шёл", "шла"),
    ("вел", "вела"),
    ("вёл", "вела"),
    ("дал", "дала"),
    ("лег", "легла"),
    ("лёг", "легла"),
    ("сел", "села"),
    ("стал", "стала"),
    ("ориентирован", "ориентирована"),
    ("дезориентирован", "дезориентирована"),
    ("контактен", "контактна"),
    ("доступен", "доступна"),
    ("адекватен", "адекватна"),
    ("спокоен", "спокойна"),
    ("тревожен", "тревожна"),
    ("напряжен", "напряжена"),
    ("напряжён", "напряжена"),
    ("возбужден", "возбуждена"),
    ("возбуждён", "возбуждена"),
    ("заторможен", "заторможена"),
    ("стабилен", "стабильна"),
    ("нестабилен", "нестабильна"),
    ("активен", "активна"),
    ("пассивен", "пассивна"),
    ("опрятен", "опрятна"),
    ("аккуратен", "аккуратна"),
    ("критичен", "критична"),
    ("самокритичен", "самокритична"),
    ("эмоционален", "эмоциональна"),
    ("раздражителен", "раздражительна"),
    ("подозрителен", "подозрительна"),
    ("насторожен", "насторожена"),
    ("насторожён", "насторожена"),
    ("замкнут", "замкнута"),
    ("открыт", "открыта"),
    ("общителен", "общительна"),
    ("вежлив", "вежлива"),
    ("агрессивен", "агрессивна"),
    ("конфликтен", "конфликтна"),
    ("упорядочен", "упорядочена"),
    ("собран", "собрана"),
    ("растерян", "растеряна"),
    ("испуган", "испугана"),
    ("обеспокоен", "обеспокоена"),
    ("заинтересован", "заинтересована"),
    ("мотивирован", "мотивирована"),
    ("настроен", "настроена"),
    ("склонен", "склонна"),
    ("способен", "способна"),
    ("готов", "готова"),
    ("удовлетворен", "удовлетворена"),
    ("удовлетворён", "удовлетворена"),
    ("согласен", "согласна"),
    ("сонлив", "сонлива"),
    ("вял", "вяла"),
    ("уверен", "уверена"),
    ("расторможен", "расторможена"),
    ("расторможён", "расторможена"),
    ("ухожен", "ухожена"),
    ("неухожен", "неухожена"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatientGender {
    Male,
    Female,
}

pub(super) fn adapt_diary_rows(rows: &mut [SemanticRecord], case: &SemanticCase) {
    let Some(gender) = patient_gender(case) else {
        return;
    };
    for row in rows {
        let Some(text) = row.get("text").map(SemanticAtom::as_text) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        row.insert(
            "text".into(),
            SemanticAtom::Text(adapt_gender_text(&text, gender)),
        );
    }
}

fn patient_gender(case: &SemanticCase) -> Option<PatientGender> {
    if let Some(value) = case.get("subject.gender") {
        let normalized = value.trim().to_lowercase().replace('ё', "е");
        if matches!(
            normalized.as_str(),
            "female" | "f" | "ж" | "жен" | "женский" | "женщина"
        ) {
            return Some(PatientGender::Female);
        }
        if matches!(
            normalized.as_str(),
            "male" | "m" | "м" | "муж" | "мужской" | "мужчина"
        ) {
            return Some(PatientGender::Male);
        }
    }
    detect_gender_from_name(case.get("subject.name")?)
}

fn detect_gender_from_name(name: &str) -> Option<PatientGender> {
    let tokens = name
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| !ch.is_alphabetic() && ch != '-')
                .to_lowercase()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }

    const FEMALE_NAMES: &[&str] = &[
        "anna",
        "ewa",
        "maria",
        "joanna",
        "agnieszka",
        "katarzyna",
        "magdalena",
        "małgorzata",
        "malgorzata",
        "aleksandra",
        "barbara",
        "monika",
        "natalia",
        "paulina",
        "karolina",
        "zofia",
        "alicja",
        "marta",
    ];
    const MALE_NAMES: &[&str] = &[
        "jan",
        "piotr",
        "paweł",
        "pawel",
        "tomasz",
        "adam",
        "marek",
        "krzysztof",
        "michał",
        "michal",
        "andrzej",
        "marcin",
        "grzegorz",
        "jakub",
        "wojciech",
        "łukasz",
        "lukasz",
        "kamil",
        "maciej",
    ];

    for token in tokens.iter().take(2) {
        if FEMALE_NAMES.contains(&token.as_str())
            || ["ska", "cka", "dzka", "owa", "ina"]
                .iter()
                .any(|suffix| token.ends_with(suffix))
        {
            return Some(PatientGender::Female);
        }
        if MALE_NAMES.contains(&token.as_str())
            || ["ski", "cki", "dzki"]
                .iter()
                .any(|suffix| token.ends_with(suffix))
        {
            return Some(PatientGender::Male);
        }
    }

    let last = tokens[0].chars().last()?;
    if "аеёиоуыэюяaąę".contains(last) {
        Some(PatientGender::Female)
    } else {
        Some(PatientGender::Male)
    }
}

fn adapt_gender_text(text: &str, gender: PatientGender) -> String {
    let mut output = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |output: &mut String, word: &mut String| {
        if word.is_empty() {
            return;
        }
        let lower = word.to_lowercase();
        let replacement = GENDER_WORD_PAIRS
            .iter()
            .find_map(|(male, female)| match gender {
                PatientGender::Male if lower == *female => Some(*male),
                PatientGender::Female if lower == *male => Some(*female),
                _ => None,
            });
        if let Some(replacement) = replacement {
            output.push_str(&preserve_case(word, replacement));
        } else {
            output.push_str(word);
        }
        word.clear();
    };

    for ch in text.chars() {
        if ch.is_alphabetic() {
            word.push(ch);
        } else {
            flush(&mut output, &mut word);
            output.push(ch);
        }
    }
    flush(&mut output, &mut word);
    output
}

fn preserve_case(source: &str, target: &str) -> String {
    let mut letters = source.chars().filter(|ch| ch.is_alphabetic()).peekable();
    if letters.peek().is_some() && letters.all(|ch| ch.is_uppercase()) {
        return target.to_uppercase();
    }
    if source
        .chars()
        .find(|ch| ch.is_alphabetic())
        .is_some_and(|ch| ch.is_uppercase())
    {
        let mut chars = target.chars();
        if let Some(first) = chars.next() {
            return first.to_uppercase().collect::<String>() + chars.as_str();
        }
    }
    target.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticValue, ValueSource};

    fn insert(case: &mut SemanticCase, field_id: &str, value: &str) {
        case.values.insert(
            field_id.into(),
            SemanticValue::new(field_id, value, ValueSource::UserConfirmed, 1.0),
        );
    }

    #[test]
    fn female_patient_gets_donor_grammar_adaptation() {
        let mut case = SemanticCase::default();
        insert(&mut case, "subject.name", "Иванова Анна");
        let mut rows = vec![SemanticRecord::from([(
            "text".into(),
            SemanticAtom::Text("Пациент был осмотрен, спокоен, доступен контакту.".into()),
        )])];
        adapt_diary_rows(&mut rows, &case);
        assert_eq!(
            rows[0].get("text").unwrap().as_text(),
            "Пациентка была осмотрена, спокойна, доступна контакту."
        );
    }

    #[test]
    fn explicit_gender_overrides_name_heuristic() {
        let mut case = SemanticCase::default();
        insert(&mut case, "subject.name", "Иванов Иван");
        insert(&mut case, "subject.gender", "женский");
        assert_eq!(patient_gender(&case), Some(PatientGender::Female));
    }

    #[test]
    fn male_text_and_unknown_gender_are_not_rewritten() {
        assert_eq!(
            adapt_gender_text("Пациент был спокоен.", PatientGender::Male),
            "Пациент был спокоен."
        );
        let case = SemanticCase::default();
        let mut rows = vec![SemanticRecord::from([(
            "text".into(),
            SemanticAtom::Text("Пациент был спокоен.".into()),
        )])];
        adapt_diary_rows(&mut rows, &case);
        assert_eq!(
            rows[0].get("text").unwrap().as_text(),
            "Пациент был спокоен."
        );
    }

    #[test]
    fn polish_donor_name_detection_is_preserved() {
        assert_eq!(
            detect_gender_from_name("Kowalska Anna"),
            Some(PatientGender::Female)
        );
        assert_eq!(
            detect_gender_from_name("Kowalski Jan"),
            Some(PatientGender::Male)
        );
    }
}
