//! Semantic extraction engine — the real "parser-parser".
//!
//! Instead of a single substring lookup per label, this engine understands a
//! document through several cooperating strategies and picks the best answer per
//! field by confidence:
//!
//!   1. **Typed scanners** find self-identifying tokens anywhere in the text —
//!      dates, money, ИНН/КПП/ОГРН/СНИЛС (checksum-validated), e-mail, phone,
//!      percent, ICD-10 codes, ФИО, organisations. A token that validates against
//!      its type is trustworthy even without a label.
//!   2. **A label engine** maps a rich synonym dictionary (medical, accounting,
//!      HR, legal, generic) to canonical fields, tolerating `:`/`—`/tab/next-line
//!      /next-cell layouts, and *type-checks* the captured value (a field typed as
//!      Date only accepts a parseable date, an ИНН must pass its checksum, …).
//!   3. **Conflict resolution** keeps the highest-confidence candidate per field;
//!      typed validation boosts confidence, so a checksum-valid ИНН beats a bare
//!      label guess.
//!
//! Everything is deterministic and unit-tested. It never fabricates: a value that
//! fails its type validation is dropped, which is what keeps the zero-touch
//! pipeline honest (unknown → «требует внимания», not a wrong document).

use serde::{Deserialize, Serialize};

use crate::label_search::find_label_end;
use crate::{
    canonical_storage_field_id, parse_flexible_date, SemanticCase, SemanticValue, ValueSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Text,
    PersonName,
    Organization,
    Date,
    Money,
    Integer,
    Percent,
    Inn,
    Kpp,
    Ogrn,
    Snils,
    Phone,
    Email,
    Icd10,
    CaseNumber,
    Address,
}

struct FieldDef {
    id: &'static str,
    ftype: FieldType,
    labels: &'static [&'static str],
}

/// Canonical field dictionary spanning several domains. Synonyms are matched
/// case-insensitively; longer/more specific labels are tried first.
fn dictionary() -> &'static [FieldDef] {
    &[
        // --- subject / person ---
        FieldDef {
            id: "subject.name",
            ftype: FieldType::PersonName,
            labels: &[
                "фамилия имя отчество",
                "ф.и.о",
                "фио",
                "пациент",
                "заявитель",
                "гражданин",
                "клиент",
            ],
        },
        FieldDef {
            id: "subject.birth_date",
            ftype: FieldType::Date,
            labels: &[
                "дата рождения",
                "родился",
                "родилась",
                "г.р",
                "год рождения",
            ],
        },
        FieldDef {
            id: "subject.address",
            ftype: FieldType::Address,
            labels: &[
                "адрес регистрации",
                "адрес проживания",
                "место жительства",
                "проживает",
                "зарегистрирован",
                "адрес",
            ],
        },
        FieldDef {
            id: "subject.snils",
            ftype: FieldType::Snils,
            labels: &["снилс", "страховой номер"],
        },
        FieldDef {
            id: "subject.phone",
            ftype: FieldType::Phone,
            labels: &["телефон", "тел", "контактный телефон", "моб"],
        },
        FieldDef {
            id: "subject.email",
            ftype: FieldType::Email,
            labels: &["e-mail", "email", "эл. почта", "электронная почта", "почта"],
        },
        // --- medical ---
        FieldDef {
            id: "medical.case_number",
            ftype: FieldType::CaseNumber,
            labels: &[
                "история болезни №",
                "номер истории болезни",
                "история болезни",
                "иб №",
                "и/б №",
                "амбулаторная карта",
            ],
        },
        FieldDef {
            id: "medical.diagnosis",
            ftype: FieldType::Text,
            labels: &["основной диагноз", "клинический диагноз", "диагноз"],
        },
        FieldDef {
            id: "medical.icd10",
            ftype: FieldType::Icd10,
            labels: &["код мкб", "мкб-10", "мкб"],
        },
        FieldDef {
            id: "medical.treatment",
            ftype: FieldType::Text,
            labels: &["назначенное лечение", "проведённое лечение", "лечение"],
        },
        FieldDef {
            id: "medical.admission_date",
            ftype: FieldType::Date,
            labels: &[
                "дата поступления",
                "поступил",
                "госпитализирован",
                "начало лечения",
            ],
        },
        FieldDef {
            id: "medical.discharge_date",
            ftype: FieldType::Date,
            labels: &["дата выписки", "выписан", "окончание лечения"],
        },
        FieldDef {
            id: "medical.workplace",
            ftype: FieldType::Organization,
            labels: &["место работы", "работает в", "работодатель"],
        },
        // «должность» без уточнения принадлежит employee.position; медицинский вариант
        // распознаётся по «в должности» (и по алиасам source_parser в мед. документах),
        // чтобы одна метка не заполняла два разных канонических поля.
        FieldDef {
            id: "medical.position",
            ftype: FieldType::Text,
            labels: &["в должности"],
        },
        // --- accounting ---
        FieldDef {
            id: "org.name",
            ftype: FieldType::Organization,
            labels: &[
                "наименование организации",
                "организация",
                "поставщик",
                "исполнитель",
                "продавец",
                "работодатель",
                "оператор",
                "отправитель",
                "сторона 1",
            ],
        },
        FieldDef {
            id: "counterparty.name",
            ftype: FieldType::Organization,
            labels: &[
                "контрагент",
                "покупатель",
                "заказчик",
                "получатель",
                "адресат",
                "плательщик",
                "сторона 2",
            ],
        },
        FieldDef {
            id: "counterparty.inn",
            ftype: FieldType::Inn,
            labels: &["инн контрагента", "инн покупателя", "инн заказчика"],
        },
        FieldDef {
            id: "counterparty.kpp",
            ftype: FieldType::Kpp,
            labels: &["кпп контрагента", "кпп покупателя", "кпп заказчика"],
        },
        FieldDef {
            id: "org.inn",
            ftype: FieldType::Inn,
            labels: &[
                "инн организации",
                "инн поставщика",
                "инн исполнителя",
                "инн продавца",
            ],
        },
        FieldDef {
            id: "org.kpp",
            ftype: FieldType::Kpp,
            labels: &[
                "кпп организации",
                "кпп поставщика",
                "кпп исполнителя",
                "кпп продавца",
            ],
        },
        FieldDef {
            id: "org.ogrn",
            ftype: FieldType::Ogrn,
            labels: &["огрнип", "огрн"],
        },
        FieldDef {
            id: "document.number",
            ftype: FieldType::CaseNumber,
            labels: &[
                "счёт на оплату №",
                "счет на оплату №",
                "счёт №",
                "счет №",
                "акт №",
                "договор №",
                "приказ №",
                "номер документа",
                "№ документа",
            ],
        },
        FieldDef {
            id: "document.date",
            ftype: FieldType::Date,
            labels: &["дата документа", "дата составления", "от", "дата"],
        },
        FieldDef {
            id: "amount.total",
            ftype: FieldType::Money,
            labels: &[
                "сумма к оплате",
                "итого к оплате",
                "всего к оплате",
                "сумма",
                "итого",
                "всего",
            ],
        },
        FieldDef {
            id: "amount.vat",
            ftype: FieldType::Money,
            labels: &["в том числе ндс", "ндс"],
        },
        // --- hr / generic ---
        FieldDef {
            id: "employee.name",
            ftype: FieldType::PersonName,
            labels: &["фио сотрудника", "сотрудник", "работник"],
        },
        FieldDef {
            id: "employee.position",
            ftype: FieldType::Text,
            labels: &["должность сотрудника", "должность", "профессия"],
        },
        FieldDef {
            id: "employee.salary",
            ftype: FieldType::Money,
            labels: &["оклад", "заработная плата", "зарплата", "ставка"],
        },
        FieldDef {
            id: "employee.department",
            ftype: FieldType::Text,
            labels: &["подразделение", "отдел", "департамент"],
        },
        FieldDef {
            id: "employee.hire_date",
            ftype: FieldType::Date,
            labels: &["дата приёма", "дата приема", "приступить к работе"],
        },
        FieldDef {
            id: "employee.contract_number",
            ftype: FieldType::CaseNumber,
            labels: &["трудовой договор №", "номер трудового договора"],
        },
        FieldDef {
            id: "employee.tab_number",
            ftype: FieldType::CaseNumber,
            labels: &["табельный номер"],
        },
        // --- legal ---
        FieldDef {
            id: "contract.number",
            ftype: FieldType::CaseNumber,
            labels: &["номер договора", "договор №", "контракт №"],
        },
        FieldDef {
            id: "contract.date",
            ftype: FieldType::Date,
            labels: &["дата договора", "дата контракта"],
        },
        FieldDef {
            id: "contract.party_a",
            ftype: FieldType::Organization,
            labels: &["сторона 1", "заказчик", "продавец", "арендодатель"],
        },
        FieldDef {
            id: "contract.party_b",
            ftype: FieldType::Organization,
            labels: &["сторона 2", "исполнитель", "покупатель", "арендатор"],
        },
        FieldDef {
            id: "contract.subject",
            ftype: FieldType::Text,
            labels: &["предмет договора", "предмет контракта"],
        },
        FieldDef {
            id: "contract.amount",
            ftype: FieldType::Money,
            labels: &["сумма договора", "цена договора", "стоимость договора"],
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedField {
    pub field_id: String,
    pub value: String,
    pub confidence: f32,
    pub method: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExtractionReport {
    pub fields: Vec<ExtractedField>,
    pub warnings: Vec<String>,
}

struct Candidate {
    field_id: String,
    value: String,
    confidence: f32,
    method: &'static str,
}

/// Run the whole engine over an extracted document text.
pub fn extract_semantic(text: &str, default_year: i32) -> (SemanticCase, ExtractionReport) {
    let mut candidates: Vec<Candidate> = Vec::new();
    candidates.extend(scan_labeled(text, default_year));
    candidates.extend(scan_typed_tokens(text));
    candidates.extend(scan_party_requisites(text));
    candidates.extend(scan_person_and_org(text));

    // Conflict resolution: best confidence per field, typed validation already
    // folded into confidence.
    use std::collections::BTreeMap;
    let mut best: BTreeMap<String, Candidate> = BTreeMap::new();
    for mut cand in candidates {
        cand.field_id = canonical_storage_field_id(&cand.field_id);
        match best.get(&cand.field_id) {
            Some(existing) if existing.confidence >= cand.confidence => {}
            _ => {
                best.insert(cand.field_id.clone(), cand);
            }
        }
    }

    let mut case = SemanticCase::default();
    let mut report = ExtractionReport::default();
    for (_id, cand) in best {
        let value = cand.value.trim().to_string();
        if value.is_empty() {
            continue;
        }
        case.values.insert(
            cand.field_id.clone(),
            SemanticValue::new(
                &cand.field_id,
                &value,
                ValueSource::Scanner,
                cand.confidence,
            )
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                &value,
                cand.method,
                cand.confidence,
            )),
        );
        report.fields.push(ExtractedField {
            field_id: cand.field_id,
            value,
            confidence: cand.confidence,
            method: cand.method.to_string(),
        });
    }
    report.fields.sort_by(|a, b| a.field_id.cmp(&b.field_id));

    // Safety carried over from the legacy parser: a case number that reads like a
    // full name is not trusted.
    if case
        .get("medical.case_number")
        .is_some_and(looks_like_person_name)
    {
        case.values.remove("medical.case_number");
        report
            .warnings
            .push("Номер истории болезни был похож на ФИО и не принят автоматически".into());
    }

    (case, report)
}

// ---------------------------------------------------------------------------
// Strategy 1 — label engine
// ---------------------------------------------------------------------------

fn scan_labeled(text: &str, default_year: i32) -> Vec<Candidate> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (idx, raw_line) in lines.iter().enumerate() {
        for def in dictionary() {
            for label in def.labels {
                let Some(value_start) = find_label_end(raw_line, label) else {
                    continue;
                };
                // Value after the label on the same line…
                let after = raw_line[value_start..]
                    .trim_start_matches([' ', ':', '-', '—', '№', '\t', '\u{00A0}'])
                    .trim();
                let raw_value = if !after.is_empty() {
                    Some(after.to_string())
                } else if def.ftype == FieldType::Money && looks_like_tabular_header(raw_line) {
                    // A column header such as "Наименование Количество Цена Сумма"
                    // must not consume the first quantity/price row as a scalar total.
                    None
                } else {
                    // …or the next non-empty line / next table cell.
                    lines
                        .get(idx + 1)
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                };
                let Some(raw_value) = raw_value else { continue };
                let cell = first_cell(&raw_value);
                if let Some((value, boost)) = normalize_typed(def.ftype, &cell, default_year) {
                    let base = 0.72 + label_specificity(label);
                    out.push(Candidate {
                        field_id: def.id.to_string(),
                        value,
                        confidence: (base + boost).min(0.99),
                        method: "label",
                    });
                }
            }
        }
    }
    out
}

/// Longer, more specific labels are more trustworthy.
fn label_specificity(label: &str) -> f32 {
    let words = label.split_whitespace().count();
    let structural = match words {
        0 | 1 => 0.0,
        2 => 0.04,
        _ => 0.08,
    };
    let lower = label.to_lowercase();
    let total_marker =
        if lower.contains("итого") || lower.contains("всего") || lower.contains("к оплате")
        {
            0.03
        } else {
            0.0
        };
    structural + total_marker
}

fn looks_like_tabular_header(line: &str) -> bool {
    let lower = line.to_lowercase();
    let markers = [
        "наименование",
        "количество",
        "цена",
        "стоимость",
        "сумма",
        "единица",
        "артикул",
    ];
    markers
        .iter()
        .filter(|marker| lower.contains(**marker))
        .count()
        >= 2
}

/// In a tab/multi-space separated "key<->value" line, keep the first value cell.
fn first_cell(value: &str) -> String {
    let by_tab = value.split('\t').map(str::trim).find(|s| !s.is_empty());
    let cell = if value.contains('\t') {
        by_tab.unwrap_or_default()
    } else {
        value.trim()
    };
    truncate_before_next_label(cell).trim().to_string()
}

/// Do not consume a second key/value pair written on the same line, for example
/// `Поставщик: ООО «Ромашка», ИНН: 7736050003`.
fn truncate_before_next_label(value: &str) -> &str {
    for (idx, ch) in value.char_indices() {
        if !matches!(ch, ',' | ';') {
            continue;
        }
        let tail = value[idx + ch.len_utf8()..]
            .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '—' | '-'));
        let tail_lower = tail.to_lowercase();
        let is_label = dictionary()
            .iter()
            .flat_map(|def| def.labels.iter().copied())
            .chain([
                "инн",
                "кпп",
                "огрн",
                "огрнип",
                "бик",
                "расчётный счёт",
                "расчетный счет",
                "корреспондентский счёт",
                "корреспондентский счет",
            ])
            .any(|label| {
                let label_lower = label.to_lowercase();
                tail_lower.starts_with(&label_lower)
                    && tail_lower[label_lower.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_whitespace() || matches!(c, ':' | '№' | '-' | '—'))
            });
        if is_label {
            return value[..idx].trim_end();
        }
    }
    value
}

// ---------------------------------------------------------------------------
// Strategy 2 — typed token scanners (label-free)
// ---------------------------------------------------------------------------

fn scan_typed_tokens(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();

    for email in find_emails(text) {
        out.push(Candidate {
            field_id: "subject.email".into(),
            value: email,
            confidence: 0.9,
            method: "typed:email",
        });
    }
    for phone in find_phones(text) {
        out.push(Candidate {
            field_id: "subject.phone".into(),
            value: phone,
            confidence: 0.8,
            method: "typed:phone",
        });
    }
    for snils in find_number_tokens(text, 11)
        .into_iter()
        .filter(|d| valid_snils(d))
    {
        out.push(Candidate {
            field_id: "subject.snils".into(),
            value: format_snils(&snils),
            confidence: 0.92,
            method: "typed:snils",
        });
    }
    for icd in find_icd10(text) {
        out.push(Candidate {
            field_id: "medical.icd10".into(),
            value: icd,
            confidence: 0.85,
            method: "typed:icd10",
        });
    }
    out
}

/// Requisites are role-sensitive. A checksum-valid ИНН is not enough to decide
/// whether it belongs to the organization or its counterparty.
fn scan_party_requisites(text: &str) -> Vec<Candidate> {
    const PROVIDER: &[&str] = &[
        "поставщик",
        "исполнитель",
        "продавец",
        "работодатель",
        "оператор",
        "сторона 1",
    ];
    const CUSTOMER: &[&str] = &[
        "контрагент",
        "покупатель",
        "заказчик",
        "получатель",
        "плательщик",
        "сторона 2",
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        for segment in line.split(';') {
            let lower = segment.trim().to_lowercase();
            let role = if PROVIDER.iter().any(|label| lower.contains(label)) {
                Some(("org.inn", "org.kpp", 0.94))
            } else if CUSTOMER.iter().any(|label| lower.contains(label)) {
                Some(("counterparty.inn", "counterparty.kpp", 0.94))
            } else if lower.starts_with("инн") || lower.starts_with("кпп") {
                Some(("org.inn", "org.kpp", 0.86))
            } else {
                None
            };
            let Some((inn_field, kpp_field, confidence)) = role else {
                continue;
            };
            if lower.contains("инн") {
                let inn = find_number_tokens(segment, 10)
                    .into_iter()
                    .chain(find_number_tokens(segment, 12))
                    .find(|digits| valid_inn(digits));
                if let Some(value) = inn {
                    out.push(Candidate {
                        field_id: inn_field.into(),
                        value,
                        confidence,
                        method: "role:inn",
                    });
                }
            }
            if lower.contains("кпп") {
                if let Some(value) = find_number_tokens(segment, 9).into_iter().next() {
                    out.push(Candidate {
                        field_id: kpp_field.into(),
                        value,
                        confidence: confidence - 0.02,
                        method: "role:kpp",
                    });
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Strategy 3 — person & organisation heuristics
// ---------------------------------------------------------------------------

fn scan_person_and_org(text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Some(org) = find_organization(text) {
        out.push(Candidate {
            field_id: "org.name".into(),
            value: org,
            confidence: 0.66,
            method: "heuristic:org",
        });
    }
    if let Some(name) = find_person_name(text) {
        out.push(Candidate {
            field_id: "subject.name".into(),
            value: name,
            confidence: 0.6,
            method: "heuristic:name",
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Typed normalisation / validation
// ---------------------------------------------------------------------------

/// Validate + normalise a raw captured value against a field type.
/// Returns `(normalized_value, confidence_boost)` or `None` if it does not fit.
pub(crate) fn normalize_typed(
    ftype: FieldType,
    raw: &str,
    default_year: i32,
) -> Option<(String, f32)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    match ftype {
        FieldType::Date | FieldType::Money if raw.is_empty() => None,
        FieldType::Date => parse_flexible_date(raw, default_year)
            .or_else(|| normalize_date_ru(raw, default_year))
            .map(|d| (d, 0.12)),
        FieldType::Money => normalize_money(raw).map(|m| (m, 0.12)),
        FieldType::Percent => normalize_percent(raw).map(|p| (p, 0.1)),
        FieldType::Inn => {
            let digits = digits_only(raw);
            if (digits.len() == 10 || digits.len() == 12) && valid_inn(&digits) {
                Some((digits, 0.2))
            } else {
                None
            }
        }
        FieldType::Kpp => {
            let digits = digits_only(raw);
            (digits.len() == 9).then_some((digits, 0.15))
        }
        FieldType::Ogrn => {
            let digits = digits_only(raw);
            (digits.len() == 13 || digits.len() == 15).then_some((digits, 0.15))
        }
        FieldType::Snils => {
            let digits = digits_only(raw);
            (digits.len() == 11 && valid_snils(&digits)).then(|| (format_snils(&digits), 0.2))
        }
        FieldType::Phone => normalize_phone(raw).map(|p| (p, 0.1)),
        FieldType::Email => is_email(raw).then(|| (raw.to_string(), 0.15)),
        FieldType::Icd10 => normalize_icd10(raw).map(|c| (c, 0.15)),
        FieldType::PersonName => {
            let value = take_person_name(raw)?;
            Some((value, 0.1))
        }
        FieldType::Organization => Some((clean_value(raw), 0.04)),
        FieldType::Integer => {
            let digits = digits_only(raw);
            (!digits.is_empty()).then_some((digits, 0.05))
        }
        FieldType::CaseNumber => {
            if looks_like_person_name(raw) {
                return None;
            }
            let cleaned = clean_case_number(raw);
            cleaned
                .chars()
                .any(|c| c.is_ascii_digit())
                .then_some((cleaned, 0.05))
        }
        FieldType::Address | FieldType::Text => Some((clean_value(raw), 0.0)),
    }
}

fn clean_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(['.', ',', ';'])
        .trim()
        .to_string()
}

fn clean_case_number(value: &str) -> String {
    value
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(['№', '.', ',', ';'])
        .to_string()
}

fn digits_only(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

fn normalize_money(raw: &str) -> Option<String> {
    // Accept a run of digits / spaces / nbsp with an optional decimal part.
    let mut int_part = String::new();
    let mut frac_part = String::new();
    let mut seen_sep = false;
    for ch in raw.chars() {
        match ch {
            '0'..='9' => {
                if seen_sep {
                    frac_part.push(ch);
                } else {
                    int_part.push(ch);
                }
            }
            ' ' | '\u{00A0}' | '\'' => {}
            ',' | '.' if !seen_sep && !int_part.is_empty() => seen_sep = true,
            _ => {
                if !int_part.is_empty() {
                    break;
                }
            }
        }
    }
    if int_part.is_empty() {
        return None;
    }
    // group thousands with spaces
    let grouped = group_thousands(&int_part);
    if seen_sep {
        let frac = format!("{:0<2}", frac_part.chars().take(2).collect::<String>());
        Some(format!("{grouped},{frac}"))
    } else {
        Some(grouped)
    }
}

fn group_thousands(digits: &str) -> String {
    let bytes: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    let len = bytes.len();
    for (i, ch) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push('\u{00A0}');
        }
        out.push(*ch);
    }
    out
}

/// Parse dates written with a Russian month word, e.g. «21 февраля 2026 г.».
fn normalize_date_ru(raw: &str, default_year: i32) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
    ];
    let lower = raw.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let mut day: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut year: Option<i32> = None;
    for tok in &tokens {
        if let Ok(n) = tok.parse::<u32>() {
            if (1..=31).contains(&n) && day.is_none() && tok.len() <= 2 {
                day = Some(n);
            } else if tok.len() == 4 {
                year = Some(n as i32);
            }
            continue;
        }
        for (idx, stem) in MONTHS.iter().enumerate() {
            if tok.starts_with(stem) {
                month = Some(idx as u32 + 1);
                break;
            }
        }
    }
    let (d, m) = (day?, month?);
    let y = year.unwrap_or(default_year);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| format!("{d:02}.{m:02}.{y:04}"))
}

fn normalize_percent(raw: &str) -> Option<String> {
    let has_percent = raw.contains('%') || raw.to_lowercase().contains("процент");
    let num: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    if num.is_empty() || !has_percent {
        return None;
    }
    Some(format!("{}%", num.replace('.', ",")))
}

fn normalize_phone(raw: &str) -> Option<String> {
    let digits = digits_only(raw);
    let d = digits.trim_start_matches('8').to_string();
    let core = if digits.len() == 11 && (digits.starts_with('7') || digits.starts_with('8')) {
        digits[1..].to_string()
    } else if digits.len() == 10 {
        digits
    } else if d.len() == 10 {
        d
    } else {
        return None;
    };
    Some(format!(
        "+7 {} {} {} {}",
        &core[0..3],
        &core[3..6],
        &core[6..8],
        &core[8..10]
    ))
}

fn is_email(raw: &str) -> bool {
    let raw = raw.trim();
    let mut parts = raw.splitn(2, '@');
    let (Some(local), Some(domain)) = (parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-' | '+'))
}

fn normalize_icd10(raw: &str) -> Option<String> {
    let token = raw
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches([',', ';', '(', ')']);
    if is_icd10(token) {
        Some(token.to_uppercase())
    } else {
        None
    }
}

fn is_icd10(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 3 {
        return false;
    }
    if !chars[0].is_alphabetic() {
        return false;
    }
    if !(chars[1].is_ascii_digit() && chars[2].is_ascii_digit()) {
        return false;
    }
    // optional ".<digit>[digit]"
    match chars.len() {
        3 => true,
        n if n >= 5 => chars[3] == '.' && chars[4..].iter().all(char::is_ascii_digit),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Token finders
// ---------------------------------------------------------------------------

/// Every maximal run of ascii digits (ignoring spaces/dashes inside groups) of the exact length.
fn find_number_tokens(text: &str, want_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if matches!(ch, ' ' | '-' | '\u{00A0}') && !current.is_empty() {
            // allow separators inside a grouped number only if more digits follow
            if chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                continue;
            } else if current.len() == want_len {
                out.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        } else {
            if current.len() == want_len {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() == want_len {
        out.push(current);
    }
    out
}

fn find_emails(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '<' | '>' | '(' | ')'))
        .map(|t| t.trim_matches(['.', ',', ';', ':']))
        .filter(|t| is_email(t))
        .map(str::to_string)
        .collect()
}

fn find_phones(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let lower = line.to_lowercase();
        let explicitly_phone = ["телефон", "тел.", "мобильный", "моб.", "phone"]
            .iter()
            .any(|label| lower.contains(label));
        let requisites_line = ["инн", "кпп", "огрн", "огрнип", "снилс", "бик"]
            .iter()
            .any(|label| lower.contains(label));
        for token in line.split(|c: char| c.is_alphabetic() && c != '\u{00A0}') {
            let digits = digits_only(token);
            let visibly_phone_formatted =
                token.contains('+') || token.contains('(') || token.matches('-').count() >= 2;
            let plausible_length = matches!(digits.len(), 10 | 11);
            let has_phone_evidence = explicitly_phone || visibly_phone_formatted;
            let permitted_by_context = explicitly_phone || !requisites_line;
            if plausible_length && has_phone_evidence && permitted_by_context {
                if let Some(phone) = normalize_phone(token) {
                    out.push(phone);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn find_icd10(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')'))
        .filter_map(normalize_icd10_token)
        .collect()
}

fn normalize_icd10_token(token: &str) -> Option<String> {
    let t = token.trim_matches([',', ';', '(', ')', '.']);
    if is_icd10(t) && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        Some(t.to_uppercase())
    } else {
        None
    }
}

fn find_organization(text: &str) -> Option<String> {
    const COUNTERPARTY_LABELS: &[&str] = &[
        "контрагент",
        "покупатель",
        "заказчик",
        "получатель",
        "плательщик",
        "сторона 2",
    ];
    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if COUNTERPARTY_LABELS
            .iter()
            .any(|label| lower.contains(label))
        {
            continue;
        }
        for marker in ["ООО", "АО", "ПАО", "ЗАО", "ИП", "ОАО", "НКО"] {
            if let Some(pos) = trimmed.find(marker) {
                let tail = &trimmed[pos..];
                // stop at a following label separator
                let value = tail.split(['\t']).next().unwrap_or(tail).trim();
                let value = strip_after_label(value);
                if value.len() > marker.len() {
                    return Some(value.trim_end_matches([',', ';', '.']).trim().to_string());
                }
            }
        }
    }
    None
}

fn strip_after_label(value: &str) -> &str {
    for cut in [", ИНН", ", инн", " ИНН", " инн", ", КПП"] {
        if let Some(pos) = value.find(cut) {
            return value[..pos].trim();
        }
    }
    value
}

fn find_person_name(text: &str) -> Option<String> {
    for line in text.lines().take(60) {
        if let Some(name) = take_person_name(line.trim()) {
            return Some(name);
        }
    }
    None
}

/// A full name: three capitalised Cyrillic words, or «Фамилия И.О.».
fn take_person_name(raw: &str) -> Option<String> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    // Фамилия Имя Отчество
    for window in tokens.windows(3) {
        if window.iter().all(|w| is_capitalized_cyrillic(w)) {
            return Some(window.join(" "));
        }
    }
    // Фамилия И.О. / Фамилия И. О.
    for i in 0..tokens.len() {
        if is_capitalized_cyrillic(tokens[i]) {
            let initials: String = tokens[i + 1..]
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if is_initials(&initials) {
                return Some(format!("{} {}", tokens[i], initials));
            }
        }
    }
    None
}

fn is_capitalized_cyrillic(word: &str) -> bool {
    let cleaned = word.trim_matches([',', '.', ';', ':', '«', '»']);
    let mut chars = cleaned.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_cyrillic(first) || !first.is_uppercase() {
        return false;
    }
    cleaned.chars().count() >= 2 && chars.all(|c| is_cyrillic(c) && c.is_lowercase())
}

fn is_initials(value: &str) -> bool {
    let v = value.replace(' ', "");
    let chars: Vec<char> = v.chars().collect();
    // И.О. -> letter dot letter dot
    (chars.len() == 4
        && is_cyrillic(chars[0])
        && chars[1] == '.'
        && is_cyrillic(chars[2])
        && chars[3] == '.')
        || (chars.len() == 2 && is_cyrillic(chars[0]) && chars[1] == '.')
}

fn is_cyrillic(c: char) -> bool {
    ('\u{0400}'..='\u{04FF}').contains(&c)
}

fn looks_like_person_name(value: &str) -> bool {
    take_person_name(value).is_some()
}

// ---------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------

/// Russian ИНН checksum for 10- and 12-digit numbers.
fn valid_inn(digits: &str) -> bool {
    let d: Vec<i64> = digits
        .chars()
        .filter_map(|c| c.to_digit(10).map(|x| x as i64))
        .collect();
    match d.len() {
        10 => {
            let w = [2, 4, 10, 3, 5, 9, 4, 6, 8];
            let control = checksum(&d[..9], &w);
            control == d[9]
        }
        12 => {
            let w1 = [7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            let w2 = [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            checksum(&d[..10], &w1) == d[10] && checksum(&d[..11], &w2) == d[11]
        }
        _ => false,
    }
}

fn checksum(digits: &[i64], weights: &[i64]) -> i64 {
    let sum: i64 = digits.iter().zip(weights).map(|(a, b)| a * b).sum();
    (sum % 11) % 10
}

/// СНИЛС checksum (11 digits, last two are the control number).
fn valid_snils(digits: &str) -> bool {
    let d: Vec<i64> = digits
        .chars()
        .filter_map(|c| c.to_digit(10).map(|x| x as i64))
        .collect();
    if d.len() != 11 {
        return false;
    }
    let sum: i64 = (0..9).map(|i| d[i] * (9 - i as i64)).sum();
    let control = match sum % 101 {
        100 => 0,
        other => other,
    };
    let stated = d[9] * 10 + d[10];
    control == stated
}

fn format_snils(digits: &str) -> String {
    if digits.len() == 11 {
        format!(
            "{}-{}-{} {}",
            &digits[0..3],
            &digits[3..6],
            &digits[6..9],
            &digits[9..11]
        )
    } else {
        digits.to_string()
    }
}

/// Canonical field type for a known field id (from the dictionary).
pub(crate) fn field_type_for(field_id: &str) -> Option<FieldType> {
    dictionary()
        .iter()
        .find(|d| d.id == field_id)
        .map(|d| d.ftype)
}

/// Human description of a field type, used to instruct a semantic model.
pub(crate) fn field_type_hint(ftype: FieldType) -> &'static str {
    match ftype {
        FieldType::Text => "свободный текст",
        FieldType::PersonName => "ФИО полностью",
        FieldType::Organization => "название организации",
        FieldType::Date => "дата ДД.ММ.ГГГГ",
        FieldType::Money => "денежная сумма",
        FieldType::Integer => "целое число",
        FieldType::Percent => "процент",
        FieldType::Inn => "ИНН (10 или 12 цифр)",
        FieldType::Kpp => "КПП (9 цифр)",
        FieldType::Ogrn => "ОГРН/ОГРНИП",
        FieldType::Snils => "СНИЛС",
        FieldType::Phone => "телефон",
        FieldType::Email => "адрес электронной почты",
        FieldType::Icd10 => "код МКБ-10",
        FieldType::CaseNumber => "номер документа/дела",
        FieldType::Address => "адрес",
    }
}

/// The canonical field schema a model can be asked to fill for one active domain.
/// Universal identity/document slots stay available everywhere, while profession-
/// specific slots are withheld from unrelated prompts. This prevents, for example,
/// an HR source from being asked to populate medical diagnosis fields.
pub(crate) fn schema_entries_for(
    domain: &crate::DomainKind,
) -> Vec<(&'static str, FieldType, &'static str)> {
    dictionary()
        .iter()
        .filter(|definition| field_visible_in_domain(definition.id, domain))
        .map(|definition| {
            (
                definition.id,
                definition.ftype,
                field_type_hint(definition.ftype),
            )
        })
        .collect()
}

fn field_visible_in_domain(field_id: &str, domain: &crate::DomainKind) -> bool {
    let universal = field_id.starts_with("subject.")
        || field_id.starts_with("org.")
        || field_id.starts_with("counterparty.")
        || field_id.starts_with("document.");
    if universal {
        return true;
    }
    match domain {
        crate::DomainKind::Medical => field_id.starts_with("medical."),
        crate::DomainKind::Hr => {
            field_id.starts_with("employee.") || field_id.starts_with("employment.")
        }
        crate::DomainKind::Legal => {
            field_id.starts_with("contract.") || field_id.starts_with("legal.")
        }
        crate::DomainKind::Accounting => {
            field_id.starts_with("accounting.")
                || field_id.starts_with("invoice.")
                || field_id.starts_with("payment.")
                || field_id.starts_with("amount.")
                || field_id.starts_with("contract.")
        }
        crate::DomainKind::Education => field_id.starts_with("education."),
        crate::DomainKind::Custom(_) => true,
        crate::DomainKind::Generic => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(case: &'a SemanticCase, id: &str) -> Option<&'a str> {
        case.get(id)
    }

    #[test]
    fn label_search_respects_word_boundaries() {
        // «от» внутри «Отчество» и «тел» внутри «работодатель» не должны срабатывать.
        assert_eq!(find_label_end("Фамилия Имя Отчество Иванов", "от"), None);
        assert_eq!(find_label_end("работодатель ООО Ромашка", "тел"), None);
        // Настоящие метки на границах слов — срабатывают.
        assert!(find_label_end("Счёт № 148 от 21.02.2026", "от").is_some());
        assert!(find_label_end("Тел: +7 900 000-00-00", "тел").is_some());
        assert!(find_label_end("05.03.1980 г.р.", "г.р").is_some());
    }

    #[test]
    fn label_search_never_panics_on_case_expanding_unicode() {
        // 'İ' (U+0130) folds to "i\u{307}" — на 1 байт длиннее. Раньше байтовый
        // сдвиг из lowered-строки резал оригинал не по границе символа.
        let tricky = "İİİ дата: 01.02.2026 İ";
        assert!(find_label_end(tricky, "дата").is_some());
        let _ = extract_semantic(tricky, 2026); // не должен паниковать
                                                // 'ẞ' (U+1E9E, 3 байта) folds to 'ß' (2 байта) — сдвиг в другую сторону.
        let _ = extract_semantic("ẞẞ ИНН 7736050003", 2026);
    }

    #[test]
    fn plain_position_label_belongs_to_hr_only() {
        let (case, _r) = extract_semantic("Должность: инженер", 2026);
        assert_eq!(get(&case, "employee.position"), Some("инженер"));
        assert_eq!(get(&case, "medical.position"), None);
        let (case2, _r2) = extract_semantic("работает в должности врача-терапевта", 2026);
        assert_eq!(get(&case2, "medical.position"), Some("врача-терапевта"));
    }

    #[test]
    fn extracts_accounting_document_end_to_end() {
        let text = "Счёт на оплату № 148 от 21.02.2026\n\
                    Поставщик: ООО «Ромашка», ИНН 7736050003\n\
                    КПП: 773601001\n\
                    Сумма к оплате: 146 500,00 руб.\n\
                    E-mail: buh@romashka.ru";
        let (case, report) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "document.number"), Some("148"));
        assert_eq!(get(&case, "document.date"), Some("21.02.2026"));
        assert_eq!(get(&case, "org.inn"), Some("7736050003"));
        assert_eq!(get(&case, "org.kpp"), Some("773601001"));
        assert_eq!(get(&case, "amount.total"), Some("146\u{00A0}500,00"));
        assert_eq!(get(&case, "subject.email"), Some("buh@romashka.ru"));
        assert!(get(&case, "org.name").unwrap().contains("Ромашка"));
        assert!(report.fields.iter().any(|f| f.method.starts_with("typed:")));
    }

    #[test]
    fn extracts_medical_fields() {
        let text = "ВЫПИСНОЙ ЭПИКРИЗ 05.02.2026\n\
                    Пациент: Иванов Иван Иванович\n\
                    Дата рождения: 03.04.1980\n\
                    История болезни № 4021\n\
                    Диагноз: J45.0 Астма\n\
                    СНИЛС: 112-233-445 95";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "subject.name"), Some("Иванов Иван Иванович"));
        assert_eq!(get(&case, "subject.birth_date"), Some("03.04.1980"));
        assert_eq!(get(&case, "medical.case_number"), Some("4021"));
        assert_eq!(get(&case, "medical.icd10"), Some("J45.0"));
        assert_eq!(get(&case, "subject.snils"), Some("112-233-445 95"));
    }

    #[test]
    fn rejects_invalid_inn_checksum() {
        let text = "ИНН 1234567890";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "org.inn"), None);
    }

    #[test]
    fn accepts_valid_inn_checksum() {
        assert!(valid_inn("7736050003"));
        assert!(valid_inn("500100732259"));
        assert!(!valid_inn("7736050004"));
    }

    #[test]
    fn snils_checksum_validates() {
        assert!(valid_snils("11223344595"));
        assert!(!valid_snils("11223344500"));
    }

    #[test]
    fn phone_normalised() {
        assert_eq!(
            normalize_phone("8 (912) 345-67-89").as_deref(),
            Some("+7 912 345 67 89")
        );
        assert_eq!(
            normalize_phone("+7 912 345 67 89").as_deref(),
            Some("+7 912 345 67 89")
        );
    }

    #[test]
    fn money_normalised() {
        assert_eq!(
            normalize_money("146 500,00 руб.").as_deref(),
            Some("146\u{00A0}500,00")
        );
        assert_eq!(
            normalize_money("1000000").as_deref(),
            Some("1\u{00A0}000\u{00A0}000")
        );
    }

    #[test]
    fn tabular_key_value_layout() {
        let text = "Должность\tГлавный врач\nОклад\t85000";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "employee.position"), Some("Главный врач"));
        assert_eq!(get(&case, "employee.salary"), Some("85\u{00A0}000"));
    }

    #[test]
    fn value_on_next_line() {
        let text = "Диагноз:\nОстрый бронхит";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "medical.diagnosis"), Some("Острый бронхит"));
    }

    #[test]
    fn separates_provider_and_customer_roles_on_one_source() {
        let text = "Исполнитель: ООО «Ромашка», ИНН: 7736050003\n\
                    Заказчик: ООО «Вектор», ИНН заказчика: 7707083893";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "org.name"), Some("ООО «Ромашка»"));
        assert_eq!(get(&case, "counterparty.name"), Some("ООО «Вектор»"));
        assert_eq!(get(&case, "org.inn"), Some("7736050003"));
        assert_eq!(get(&case, "counterparty.inn"), Some("7707083893"));
    }

    #[test]
    fn counterparty_only_inn_is_not_misclassified_as_organization_inn() {
        let text = "Заказчик: ООО «Вектор», ИНН заказчика: 7707083893";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "counterparty.inn"), Some("7707083893"));
        assert_eq!(get(&case, "org.inn"), None);
        assert_eq!(get(&case, "org.name"), None);
    }

    #[test]
    fn hr_labels_fill_canonical_employee_fields() {
        let text = "Сотрудник: Иванов Иван Иванович\nДолжность: инженер\nОтдел: производство";
        let (case, _r) = extract_semantic(text, 2026);
        assert_eq!(get(&case, "employee.name"), Some("Иванов Иван Иванович"));
        assert_eq!(get(&case, "employee.position"), Some("инженер"));
        assert_eq!(get(&case, "employee.department"), Some("производство"));
    }

    #[test]
    fn does_not_fabricate_on_empty_source() {
        let (case, report) = extract_semantic("   \n  \n", 2026);
        assert!(case.values.is_empty());
        assert!(report.fields.is_empty());
    }

    #[test]
    fn requisites_are_not_guessed_as_phone_numbers() {
        let (case, _) = extract_semantic(
            "Исполнитель: ООО «Ромашка», ИНН: 7736050003, КПП: 773601001",
            2026,
        );
        assert_eq!(get(&case, "subject.phone"), None);
        let (phone_case, _) = extract_semantic("Телефон: 89123456789", 2026);
        assert_eq!(get(&phone_case, "subject.phone"), Some("+7 912 345 67 89"));
    }
}
