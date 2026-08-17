use crate::label_search::find_label_end;
use crate::{
    merge_value, parse_flexible_date, validate_case_relations, validate_field_value, SemanticAtom,
    SemanticCase, SemanticRecord, SemanticValue, ValueSource,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSourceReport {
    pub recognized_title: Option<String>,
    pub filled_fields: Vec<String>,
    pub warnings: Vec<String>,
}

/// Deterministic, profession-neutral source scanner. Medical aliases are activated
/// only when the text contains medical markers; the resulting values are mirrored
/// into generic period/classification/action fields so the core remains universal.
pub fn parse_source_text(text: &str, default_year: i32) -> (SemanticCase, ParsedSourceReport) {
    let generic_source = crate::core::SourceDocument {
        id: "source_text".into(),
        text: text.into(),
        metadata: Default::default(),
    };
    let generic_parsed =
        crate::core::parse_source_document_with_default_year(&generic_source, default_year);
    let mut case = SemanticCase::default();
    let mut report = ParsedSourceReport {
        recognized_title: detect_source_title(text)
            .or_else(|| Some(generic_parsed.document_type.title.clone())),
        filled_fields: Vec::new(),
        warnings: generic_parsed.warnings,
    };
    put(
        &mut case,
        &mut report,
        "document.type",
        &generic_parsed.document_type.id,
        0.55,
    );
    for field in generic_parsed.fields {
        put(
            &mut case,
            &mut report,
            &field.id,
            &field.value,
            f32::from(field.confidence) / 100.0,
        );
    }
    if let Some(title) = report.recognized_title.clone() {
        put(&mut case, &mut report, "document.title", &title, 0.88);
    }
    if let Some(date) = detect_date_near_title(text, default_year) {
        put(&mut case, &mut report, "document.date", &date, 0.72);
    }

    for rule in generic_rules() {
        if let Some(value) = find_labeled_value(text, rule.labels, rule.multiline) {
            let value = normalize_field_value(rule.field, &value, default_year).unwrap_or(value);
            put(&mut case, &mut report, rule.field, &value, 0.80);
        }
    }
    apply_role_aware_source_facts(text, default_year, &mut case, &mut report);

    let lower = text.to_lowercase();
    let medical = [
        "диагноз",
        "история болезни",
        "пациент",
        "госпитализац",
        "лечащий врач",
        "дата поступления",
        "жалобы",
        "анамнез",
        "соматический статус",
        "профильный статус",
        "лаборатор",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if medical {
        for rule in medical_rules() {
            if let Some(value) = find_labeled_value(text, rule.labels, rule.multiline) {
                let value =
                    normalize_field_value(rule.field, &value, default_year).unwrap_or(value);
                put(&mut case, &mut report, rule.field, &value, 0.82);
            }
        }
        // In primary/profile documents the date printed next to the title is
        // commonly the start/admission date even when no explicit label exists.
        // The generic document date is still retained; the profile alias is only
        // filled when no stronger labelled value was found.
        if case.get("medical.admission_date").is_none() {
            if let Some(date) = case.get("document.date").map(str::to_owned) {
                put(
                    &mut case,
                    &mut report,
                    "medical.admission_date",
                    &date,
                    0.70,
                );
            }
        }
        mirror_medical_to_generic(&mut case, &mut report);
    }

    let (engine_case, engine_report) = crate::extract_semantic(text, default_year);
    for (field_id, semantic_value) in engine_case.values {
        let better = case
            .values
            .get(&field_id)
            .is_none_or(|existing| semantic_value.confidence > existing.confidence + 0.001);
        if better {
            put(
                &mut case,
                &mut report,
                &field_id,
                &semantic_value.value,
                semantic_value.confidence,
            );
        }
    }
    for warning in engine_report.warnings {
        if !report.warnings.contains(&warning) {
            report.warnings.push(warning);
        }
    }

    if let Some((items, warnings)) = extract_items_table(text) {
        case.set_collection("items", items);
        if !report
            .filled_fields
            .iter()
            .any(|field| field == "collection.items")
        {
            report.filled_fields.push("collection.items".into());
        }
        for warning in warnings {
            if !report.warnings.contains(&warning) {
                report.warnings.push(warning);
            }
        }
    }

    if case
        .get("document.number")
        .is_some_and(looks_like_person_name)
    {
        case.values.remove("document.number");
        report
            .filled_fields
            .retain(|field| field != "document.number");
        report
            .warnings
            .push("Номер документа был похож на ФИО/имя и не принят автоматически".into());
    }
    if case
        .get("medical.case_number")
        .is_some_and(looks_like_person_name)
    {
        case.values.remove("medical.case_number");
        report
            .filled_fields
            .retain(|field| field != "medical.case_number");
        report
            .warnings
            .push("Номер профильного дела был похож на ФИО/имя и не принят автоматически".into());
    }

    let prediction = crate::classify_source_domain(text, &case);
    if prediction.domain != crate::DomainKind::Generic && prediction.confidence >= 0.60 {
        case.active_domains = vec![prediction.domain.clone()];
        mirror_profile_fields(&mut case, &mut report, &prediction.domain);
        report.warnings.push(format!(
            "Профиль источника определён автоматически: {:?} ({:.0}%, признаки: {}).",
            prediction.domain,
            prediction.confidence * 100.0,
            prediction.evidence.join(", ")
        ));
    }

    crate::normalize_semantic_case_aliases(&mut case);
    report.filled_fields = case.values.keys().cloned().collect();
    report.filled_fields.extend(
        case.collections
            .keys()
            .map(|collection_id| format!("collection.{collection_id}")),
    );
    report.filled_fields.sort();
    report.filled_fields.dedup();

    for (field_id, error) in validate_case_relations(&case) {
        case.values.remove(&field_id);
        report.filled_fields.retain(|field| field != &field_id);
        report
            .warnings
            .push(format!("Поле «{field_id}» отклонено: {error}"));
    }
    (case, report)
}

struct LabelRule {
    field: &'static str,
    labels: &'static [&'static str],
    multiline: bool,
}

fn generic_rules() -> Vec<LabelRule> {
    vec![
        LabelRule {
            field: "subject.name",
            labels: &[
                "ФИО",
                "Полное имя",
                "Субъект",
                "Клиент",
                "Сотрудник",
                "Ученик",
                "Студент",
            ],
            multiline: false,
        },
        LabelRule {
            field: "subject.birth_date",
            labels: &["Дата рождения", "Родился", "Родилась"],
            multiline: false,
        },
        LabelRule {
            field: "subject.address",
            labels: &["Адрес", "Место жительства", "Проживает"],
            multiline: false,
        },
        LabelRule {
            field: "subject.organization",
            labels: &[
                "Место работы",
                "Организация субъекта",
                "Учреждение",
                "Работает",
            ],
            multiline: false,
        },
        LabelRule {
            field: "subject.position",
            labels: &["Должность", "в должности", "Роль"],
            multiline: false,
        },
        LabelRule {
            field: "org.name",
            labels: &[
                "Наименование организации",
                "Организация",
                "Компания",
                "Учреждение",
                "Поставщик",
                "Исполнитель",
                "Продавец",
                "Работодатель",
                "Оператор",
                "Отправитель",
                "Сторона 1",
            ],
            multiline: false,
        },
        LabelRule {
            field: "counterparty.name",
            labels: &[
                "Контрагент",
                "Покупатель",
                "Заказчик",
                "Получатель",
                "Адресат",
                "Плательщик",
                "Сторона 2",
            ],
            multiline: false,
        },
        LabelRule {
            field: "counterparty.inn",
            labels: &["ИНН контрагента", "ИНН покупателя", "ИНН заказчика"],
            multiline: false,
        },
        LabelRule {
            field: "counterparty.kpp",
            labels: &["КПП контрагента", "КПП покупателя", "КПП заказчика"],
            multiline: false,
        },
        LabelRule {
            field: "org.inn",
            labels: &["ИНН организации", "ИНН поставщика", "ИНН исполнителя"],
            multiline: false,
        },
        LabelRule {
            field: "subject.snils",
            labels: &["СНИЛС"],
            multiline: false,
        },
        LabelRule {
            field: "subject.gender",
            labels: &["Пол"],
            multiline: false,
        },
        LabelRule {
            field: "subject.birth_place",
            labels: &["Место рождения"],
            multiline: false,
        },
        LabelRule {
            field: "subject.passport_series",
            labels: &["Серия паспорта", "Паспорт серия"],
            multiline: false,
        },
        LabelRule {
            field: "subject.passport_number",
            labels: &["Номер паспорта", "Паспорт №"],
            multiline: false,
        },
        LabelRule {
            field: "subject.passport_issued_by",
            labels: &["Кем выдан паспорт", "Паспорт выдан", "Кем выдан"],
            multiline: true,
        },
        LabelRule {
            field: "subject.passport_issued_date",
            labels: &["Дата выдачи паспорта", "Дата выдачи"],
            multiline: false,
        },
        LabelRule {
            field: "subject.passport_code",
            labels: &["Код подразделения"],
            multiline: false,
        },
        LabelRule {
            field: "subject.address_registration",
            labels: &["Адрес регистрации", "Зарегистрирован по адресу"],
            multiline: false,
        },
        LabelRule {
            field: "subject.address_actual",
            labels: &["Фактический адрес", "Адрес проживания"],
            multiline: false,
        },
        LabelRule {
            field: "subject.phone",
            labels: &["Телефон", "Мобильный телефон"],
            multiline: false,
        },
        LabelRule {
            field: "subject.email",
            labels: &["E-mail", "Email", "Электронная почта"],
            multiline: false,
        },
        LabelRule {
            field: "subject.inn_person",
            labels: &["ИНН физического лица", "ИНН физлица"],
            multiline: false,
        },
        LabelRule {
            field: "org.name",
            labels: &[
                "Поставщик",
                "Исполнитель",
                "Продавец",
                "Работодатель",
                "Оператор",
            ],
            multiline: false,
        },
        LabelRule {
            field: "org.inn",
            labels: &[
                "ИНН организации",
                "ИНН поставщика",
                "ИНН исполнителя",
                "ИНН продавца",
            ],
            multiline: false,
        },
        LabelRule {
            field: "org.ogrn",
            labels: &["ОГРН", "ОГРНИП"],
            multiline: false,
        },
        LabelRule {
            field: "org.kpp",
            labels: &[
                "КПП организации",
                "КПП поставщика",
                "КПП исполнителя",
                "КПП продавца",
            ],
            multiline: false,
        },
        LabelRule {
            field: "org.okpo",
            labels: &["ОКПО"],
            multiline: false,
        },
        LabelRule {
            field: "org.okved",
            labels: &["ОКВЭД"],
            multiline: false,
        },
        LabelRule {
            field: "org.legal_address",
            labels: &["Юридический адрес"],
            multiline: false,
        },
        LabelRule {
            field: "org.actual_address",
            labels: &["Фактический адрес организации"],
            multiline: false,
        },
        LabelRule {
            field: "org.bank_name",
            labels: &["Наименование банка", "Банк"],
            multiline: false,
        },
        LabelRule {
            field: "org.bank_bik",
            labels: &["БИК"],
            multiline: false,
        },
        LabelRule {
            field: "org.bank_account",
            labels: &["Расчётный счёт", "Расчетный счет", "р/с"],
            multiline: false,
        },
        LabelRule {
            field: "org.bank_corr_account",
            labels: &["Корреспондентский счёт", "Корреспондентский счет", "к/с"],
            multiline: false,
        },
        LabelRule {
            field: "org.director_name",
            labels: &["Руководитель", "Директор", "в лице"],
            multiline: false,
        },
        LabelRule {
            field: "org.director_position",
            labels: &["Должность руководителя"],
            multiline: false,
        },
        LabelRule {
            field: "org.director_basis",
            labels: &[
                "Действующего на основании",
                "Действующей на основании",
                "Основание полномочий",
            ],
            multiline: false,
        },
        LabelRule {
            field: "employee.name",
            labels: &["ФИО сотрудника", "Сотрудник", "Работник"],
            multiline: false,
        },
        LabelRule {
            field: "employee.position",
            labels: &["Должность сотрудника", "Должность", "в должности"],
            multiline: false,
        },
        LabelRule {
            field: "employee.tab_number",
            labels: &["Табельный номер"],
            multiline: false,
        },
        LabelRule {
            field: "employee.department",
            labels: &["Подразделение", "Отдел"],
            multiline: false,
        },
        LabelRule {
            field: "employee.hire_date",
            labels: &["Дата приёма", "Дата приема"],
            multiline: false,
        },
        LabelRule {
            field: "employee.salary",
            labels: &["Оклад", "Заработная плата"],
            multiline: false,
        },
        LabelRule {
            field: "employee.contract_number",
            labels: &["Трудовой договор №", "Номер трудового договора"],
            multiline: false,
        },
        LabelRule {
            field: "contract.party_a",
            labels: &["Сторона 1", "Заказчик", "Продавец", "Арендодатель"],
            multiline: false,
        },
        LabelRule {
            field: "contract.party_b",
            labels: &["Сторона 2", "Исполнитель", "Покупатель", "Арендатор"],
            multiline: false,
        },
        LabelRule {
            field: "contract.number",
            labels: &["Номер договора", "Договор №"],
            multiline: false,
        },
        LabelRule {
            field: "contract.date",
            labels: &["Дата договора"],
            multiline: false,
        },
        LabelRule {
            field: "contract.subject",
            labels: &["Предмет договора"],
            multiline: true,
        },
        LabelRule {
            field: "contract.start_date",
            labels: &["Дата начала договора", "Договор действует с"],
            multiline: false,
        },
        LabelRule {
            field: "contract.end_date",
            labels: &["Дата окончания договора", "Договор действует по"],
            multiline: false,
        },
        LabelRule {
            field: "contract.amount",
            labels: &["Сумма договора", "Цена договора"],
            multiline: false,
        },
        LabelRule {
            field: "contract.currency",
            labels: &["Валюта договора", "Валюта"],
            multiline: false,
        },
        LabelRule {
            field: "contract.penalty_percent",
            labels: &["Неустойка", "Процент неустойки"],
            multiline: false,
        },
        LabelRule {
            field: "realty.cadastral_number",
            labels: &["Кадастровый номер"],
            multiline: false,
        },
        LabelRule {
            field: "realty.address",
            labels: &["Адрес объекта", "Адрес недвижимости"],
            multiline: false,
        },
        LabelRule {
            field: "realty.area",
            labels: &["Площадь объекта", "Общая площадь"],
            multiline: false,
        },
        LabelRule {
            field: "vehicle.vin",
            labels: &["VIN", "VIN-код"],
            multiline: false,
        },
        LabelRule {
            field: "vehicle.gos_number",
            labels: &["Государственный номер", "Госномер", "Гос. номер"],
            multiline: false,
        },
        LabelRule {
            field: "vehicle.brand_model",
            labels: &["Марка и модель", "Марка/модель"],
            multiline: false,
        },
        LabelRule {
            field: "vehicle.year",
            labels: &["Год выпуска"],
            multiline: false,
        },
        LabelRule {
            field: "vehicle.pts_number",
            labels: &["Номер ПТС", "ПТС №"],
            multiline: false,
        },
        LabelRule {
            field: "accounting.invoice_number",
            labels: &["Счёт на оплату №", "Счет на оплату №", "Счёт №", "Счет №"],
            multiline: false,
        },
        LabelRule {
            field: "accounting.invoice_date",
            labels: &["Дата счёта", "Дата счета"],
            multiline: false,
        },
        LabelRule {
            field: "document.number",
            labels: &[
                "Номер документа",
                "Документ №",
                "№ документа",
                "Номер дела",
                "Дело №",
            ],
            multiline: false,
        },
        LabelRule {
            field: "document.date",
            labels: &["Дата документа", "Дата составления"],
            multiline: false,
        },
        LabelRule {
            field: "period.start_date",
            labels: &["Дата начала", "Начало периода", "Период с", "Срок с"],
            multiline: false,
        },
        LabelRule {
            field: "period.end_date",
            labels: &["Дата окончания", "Конец периода", "Период по", "Срок по"],
            multiline: false,
        },
        LabelRule {
            field: "classification.primary",
            labels: &[
                "Классификация",
                "Категория",
                "Тип случая",
                "Основная тема",
                "Предмет",
            ],
            multiline: true,
        },
        LabelRule {
            field: "action.plan",
            labels: &[
                "План действий",
                "План работы",
                "Назначенные действия",
                "Рекомендации",
                "Мероприятия",
            ],
            multiline: true,
        },
    ]
}

fn medical_rules() -> Vec<LabelRule> {
    vec![
        LabelRule {
            field: "medical.case_number",
            labels: &[
                "История болезни №",
                "История болезни N",
                "Номер истории болезни",
                "ИБ №",
                "и/б №",
                "Nr historii choroby",
                "Numer historii choroby",
                "Historia choroby nr",
            ],
            multiline: false,
        },
        LabelRule {
            field: "subject.name",
            labels: &[
                "Ф.И.О.",
                "Ф.И.О",
                "ФИО пациента",
                "Ф.И.О. пациента",
                "Фамилия Имя Отчество",
                "Пациент",
                "Пациентка",
                "Pacjent",
                "Pacjentka",
                "Imię i nazwisko",
                "Imie i nazwisko",
            ],
            multiline: false,
        },
        LabelRule {
            field: "subject.age",
            labels: &["Возраст", "Wiek"],
            multiline: false,
        },
        LabelRule {
            field: "subject.birth_date",
            labels: &["Дата рождения", "Data urodzenia"],
            multiline: false,
        },
        LabelRule {
            field: "subject.address",
            labels: &[
                "Зарегистрирован по адресу",
                "Адрес регистрации",
                "Адрес проживания",
                "Место жительства",
                "Adres zamieszkania",
                "Miejsce zamieszkania",
            ],
            multiline: false,
        },
        LabelRule {
            field: "medical.admission_date",
            labels: &[
                "Дата поступления",
                "Дата госпитализации",
                "Data przyjęcia",
                "Data przyjecia",
                "Data hospitalizacji",
            ],
            multiline: false,
        },
        LabelRule {
            field: "medical.discharge_date",
            labels: &["Дата выписки", "Data wypisu"],
            multiline: false,
        },
        LabelRule {
            field: "medical.complaints",
            labels: &[
                "Жалобы на момент осмотра",
                "Жалобы при поступлении",
                "Жалобы",
                "Skargi przy przyjęciu",
                "Skargi przy przyjeciu",
                "Dolegliwości",
                "Dolegliwosci",
                "Skargi",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.anamnesis_life",
            labels: &[
                "Анамнез жизни",
                "Wywiad życiowy",
                "Wywiad zyciowy",
                "Wywiad osobniczy",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.anamnesis_disease",
            labels: &[
                "Анамнез заболевания",
                "Wywiad chorobowy",
                "Wywiad obecnej choroby",
                "Historia choroby",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.profile_status",
            labels: &[
                "Профильный статус при поступлении",
                "Профильный статус",
                "Психический статус при поступлении",
                "Психический статус",
                "Stan psychiczny",
                "Badanie psychiatryczne",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.somatic_status",
            labels: &[
                "Сомато-неврологический статус",
                "Соматический статус",
                "Объективный статус",
                "Объективно",
                "Status praesens",
                "Stan przedmiotowy",
                "Badanie przedmiotowe",
                "Stan somatyczny",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.examination_plan",
            labels: &["План обследования", "Plan badań", "Plan badan"],
            multiline: true,
        },
        LabelRule {
            field: "medical.diagnosis",
            labels: &[
                "Клинический диагноз",
                "Предварительный диагноз",
                "Основной диагноз",
                "Заключительный диагноз",
                "Диагноз",
                "Rozpoznanie kliniczne",
                "Rozpoznanie główne",
                "Rozpoznanie glowne",
                "Rozpoznanie",
                "Diagnoza",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.icd10",
            labels: &["Код МКБ-10", "Код МКБ", "МКБ-10", "ICD-10"],
            multiline: false,
        },
        LabelRule {
            field: "medical.treatment",
            labels: &[
                "План лечения",
                "Назначенное лечение",
                "Лечение",
                "Plan leczenia",
                "Zalecone leczenie",
                "Zastosowane leczenie",
                "Leczenie",
                "Terapia",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.treatment_result",
            labels: &["Результат лечения", "Исход лечения", "Эффект лечения"],
            multiline: true,
        },
        LabelRule {
            field: "medical.discharge_condition",
            labels: &["Состояние при выписке", "Состояние на момент выписки"],
            multiline: true,
        },
        LabelRule {
            field: "medical.recommendations",
            labels: &["Рекомендации", "Рекомендовано", "Zalecenia"],
            multiline: true,
        },
        LabelRule {
            field: "medical.labs",
            labels: &[
                "Лабораторные исследования",
                "Лабораторные данные",
                "Результаты анализов",
                "Результаты обследований",
                "Результаты исследований",
                "Анализы",
                "Wyniki badań",
                "Wyniki badan",
            ],
            multiline: true,
        },
        LabelRule {
            field: "medical.labs_date",
            labels: &["Дата анализов", "Дата лабораторных исследований"],
            multiline: false,
        },
        LabelRule {
            field: "medical.workplace",
            labels: &[
                "Работает в организации",
                "Место работы",
                "Работа",
                "Miejsce pracy",
                "Zakład pracy",
                "Zaklad pracy",
            ],
            multiline: false,
        },
        LabelRule {
            field: "medical.position",
            labels: &["Должность", "Stanowisko", "Zawód", "Zawod"],
            multiline: false,
        },
        LabelRule {
            field: "medical.sick_leave_number",
            labels: &[
                "Номер больничного",
                "Больничный лист №",
                "Лист нетрудоспособности №",
            ],
            multiline: false,
        },
        LabelRule {
            field: "medical.attending_doctor",
            labels: &["Лечащий врач", "Lekarz prowadzący", "Lekarz prowadzacy"],
            multiline: false,
        },
        LabelRule {
            field: "medical.department_head",
            labels: &[
                "Заведующий отделением",
                "Зав. отделением",
                "Зав. отд.",
                "Ordynator",
                "Kierownik oddziału",
                "Kierownik oddzialu",
            ],
            multiline: false,
        },
    ]
}

fn mirror_medical_to_generic(case: &mut SemanticCase, report: &mut ParsedSourceReport) {
    let mappings = [
        ("medical.case_number", "document.number"),
        ("medical.admission_date", "period.start_date"),
        ("medical.discharge_date", "period.end_date"),
        ("medical.diagnosis", "classification.primary"),
        ("medical.treatment", "action.plan"),
        ("medical.workplace", "subject.organization"),
        ("medical.position", "subject.position"),
    ];
    for (source, target) in mappings {
        if let Some(value) = case.get(source).map(str::to_owned) {
            put(case, report, target, &value, 0.75);
        }
    }
}

fn extract_items_table(text: &str) -> Option<(Vec<SemanticRecord>, Vec<String>)> {
    let lines = text.lines().collect::<Vec<_>>();
    for (header_index, line) in lines.iter().enumerate() {
        let Some((delimiter, header_cells)) = split_table_line(line) else {
            continue;
        };
        let mapped = header_cells
            .iter()
            .map(|header| item_column_id(header))
            .collect::<Vec<_>>();
        let recognized = mapped.iter().filter(|value| value.is_some()).count();
        let has_name = mapped.iter().any(|value| value.as_deref() == Some("name"));
        let has_value_column = mapped.iter().any(|value| {
            matches!(
                value.as_deref(),
                Some("quantity" | "price" | "amount" | "unit")
            )
        });
        if recognized < 2 || !has_name || !has_value_column {
            continue;
        }

        let mut rows = Vec::new();
        let mut warnings = Vec::new();
        for (offset, raw_line) in lines.iter().skip(header_index + 1).enumerate() {
            if raw_line.trim().is_empty() {
                if !rows.is_empty() {
                    break;
                }
                continue;
            }
            let Some(cells) = split_table_line_with(raw_line, delimiter) else {
                if !rows.is_empty() {
                    break;
                }
                continue;
            };
            let first = cells
                .first()
                .map(|value| value.trim().to_lowercase())
                .unwrap_or_default();
            if matches!(first.as_str(), "итого" | "всего" | "total" | "grand total") {
                break;
            }
            if cells.len() < 2 {
                if !rows.is_empty() {
                    break;
                }
                continue;
            }
            let mut record = SemanticRecord::new();
            for (index, field_id) in mapped.iter().enumerate() {
                let Some(field_id) = field_id else { continue };
                let Some(raw_value) = cells.get(index) else {
                    continue;
                };
                let value = raw_value.trim();
                if value.is_empty() {
                    continue;
                }
                record.insert(field_id.clone(), item_atom(field_id, value));
            }
            let has_row_name = record
                .get("name")
                .map(SemanticAtom::as_text)
                .is_some_and(|value| !value.trim().is_empty());
            if has_row_name && record.len() >= 2 {
                rows.push(record);
            } else if !record.is_empty() {
                warnings.push(format!(
                    "Строка таблицы {} пропущена: нет наименования или значений",
                    header_index + offset + 2
                ));
            } else if !rows.is_empty() {
                break;
            }
            if rows.len() >= 10_000 {
                warnings.push("Таблица ограничена первыми 10 000 позициями".into());
                break;
            }
        }
        if !rows.is_empty() {
            return Some((rows, warnings));
        }
    }
    None
}

#[derive(Debug, Clone, Copy)]
enum TableDelimiter {
    Char(char),
    MultiSpace,
}

fn split_table_line(line: &str) -> Option<(TableDelimiter, Vec<String>)> {
    for delimiter in ['\t', '|', ';'] {
        let cells = split_char_delimited(line, delimiter);
        if cells.len() >= 2 {
            return Some((TableDelimiter::Char(delimiter), cells));
        }
    }
    let cells = split_multi_space(line);
    (cells.len() >= 2).then_some((TableDelimiter::MultiSpace, cells))
}

fn split_table_line_with(line: &str, delimiter: TableDelimiter) -> Option<Vec<String>> {
    let cells = match delimiter {
        TableDelimiter::Char(value) => split_char_delimited(line, value),
        TableDelimiter::MultiSpace => split_multi_space(line),
    };
    (cells.len() >= 2).then_some(cells)
}

fn split_char_delimited(line: &str, delimiter: char) -> Vec<String> {
    let mut cells = line
        .split(delimiter)
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    while cells.last().is_some_and(|value| value.is_empty()) {
        cells.pop();
    }
    cells
}

fn split_multi_space(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut spaces = 0usize;
    for ch in line.chars() {
        if ch.is_whitespace() {
            spaces += 1;
            continue;
        }
        if spaces >= 2 && !current.trim().is_empty() {
            cells.push(current.trim().to_string());
            current.clear();
        } else if spaces == 1 && !current.is_empty() {
            current.push(' ');
        }
        spaces = 0;
        current.push(ch);
    }
    if !current.trim().is_empty() {
        cells.push(current.trim().to_string());
    }
    cells
}

fn item_column_id(header: &str) -> Option<String> {
    let normalized = header
        .trim()
        .to_lowercase()
        .replace(['.', ':', '№', '(', ')'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let id = match normalized.as_str() {
        "наименование"
        | "наименование товара"
        | "товар"
        | "услуга"
        | "работа"
        | "позиция"
        | "описание"
        | "item"
        | "name"
        | "description" => "name",
        "кол-во" | "количество" | "qty" | "quantity" => "quantity",
        "ед" | "ед изм" | "единица" | "единица измерения" | "unit" => {
            "unit"
        }
        "цена" | "цена за ед" | "цена за единицу" | "стоимость за единицу" | "price" => {
            "price"
        }
        "сумма" | "стоимость" | "amount" | "total" => "amount",
        "ндс" | "ставка ндс" | "vat" => "vat",
        _ => return None,
    };
    Some(id.into())
}

fn item_atom(field_id: &str, value: &str) -> SemanticAtom {
    if matches!(field_id, "quantity" | "price" | "amount" | "vat") {
        let normalized = value
            .replace('\u{00a0}', " ")
            .replace(' ', "")
            .replace(',', ".")
            .trim_end_matches('%')
            .trim_end_matches("руб.")
            .trim_end_matches("руб")
            .trim()
            .to_string();
        if is_decimal_literal(&normalized) {
            return SemanticAtom::Decimal(normalized);
        }
    }
    SemanticAtom::Text(value.trim().to_string())
}

fn is_decimal_literal(value: &str) -> bool {
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    if unsigned.is_empty() {
        return false;
    }
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    parts.next().is_none()
        && !integer.is_empty()
        && integer.chars().all(|ch| ch.is_ascii_digit())
        && fraction
            .is_none_or(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

pub fn detect_source_title(text: &str) -> Option<String> {
    text.lines()
        .take(25)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(strip_leading_date_loose)
        .find(|line| line.chars().filter(|c| c.is_alphabetic()).count() >= 4)
        .map(str::to_string)
}

pub fn detect_date_near_title(text: &str, default_year: i32) -> Option<String> {
    for line in text.lines().take(10) {
        let lower = line.to_lowercase();
        if lower.contains("дата рождения")
            || lower.contains("родился")
            || lower.contains("родилась")
        {
            continue;
        }
        if let Some(candidate) = first_date_candidate(line) {
            if let Some(parsed) = parse_flexible_date(&candidate, default_year) {
                return Some(parsed);
            }
        }
        if let Some(parsed) = parse_flexible_date(line, default_year) {
            return Some(parsed);
        }
    }
    None
}

fn find_labeled_value(text: &str, labels: &[&str], multiline: bool) -> Option<String> {
    let lines = text.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        for label in labels {
            if let Some(value_start) = find_label_end(line, label) {
                let after = line[value_start..]
                    .trim_start_matches([' ', ':', '-', '—', '№'])
                    .trim();
                let mut values = Vec::new();
                if !after.is_empty() {
                    values.push(clean_inline_value(after));
                }
                if values.is_empty() || multiline {
                    for next in lines.iter().skip(index + 1) {
                        let cleaned = clean_value(next);
                        if cleaned.is_empty() {
                            if !values.is_empty() {
                                break;
                            } else {
                                continue;
                            }
                        }
                        if looks_like_known_label(&cleaned) {
                            break;
                        }
                        values.push(cleaned);
                        if !multiline {
                            break;
                        }
                    }
                }
                let joined = values.join("\n").trim().to_string();
                if !joined.is_empty() {
                    return Some(joined);
                }
            }
        }
    }
    None
}

fn looks_like_known_label(line: &str) -> bool {
    let lower = line.to_lowercase();
    if [
        "инн",
        "кпп",
        "огрн",
        "огрнип",
        "бик",
        "расчётный счёт",
        "расчетный счет",
        "корреспондентский счёт",
        "корреспондентский счет",
        "телефон",
        "e-mail",
        "email",
    ]
    .iter()
    .any(|label| {
        lower.starts_with(label)
            && lower[label.len()..]
                .chars()
                .next()
                .is_none_or(|ch| ch.is_whitespace() || matches!(ch, ':' | '№' | '-' | '—'))
    }) {
        return true;
    }
    generic_rules()
        .into_iter()
        .chain(medical_rules())
        .any(|rule| {
            rule.labels.iter().any(|label| {
                lower.starts_with(&label.to_lowercase())
                    && (lower.chars().count() == label.to_lowercase().chars().count()
                        || lower.contains(':'))
            })
        })
}

fn normalize_field_value(field: &str, value: &str, default_year: i32) -> Option<String> {
    if field.ends_with(".date") || field.ends_with("_date") {
        return parse_flexible_date(value, default_year);
    }
    if field == "document.number" || field == "medical.case_number" {
        return Some(
            value
                .chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim()
                .to_string(),
        );
    }
    if matches!(
        field,
        "contract.number"
            | "employee.contract_number"
            | "hr.order_number"
            | "accounting.invoice_number"
    ) {
        let cleaned = value
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>();
        return cleaned
            .split_whitespace()
            .next()
            .map(|number| number.trim_matches(['№', '.', ',', ';']).to_string())
            .filter(|number| !number.is_empty());
    }
    None
}

fn apply_role_aware_source_facts(
    text: &str,
    default_year: i32,
    case: &mut SemanticCase,
    report: &mut ParsedSourceReport,
) {
    let header = text
        .lines()
        .take(15)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find_map(|line| {
            let lower = line.to_lowercase();
            let role = if lower.contains("счёт") || lower.contains("счет") {
                Some("invoice")
            } else if lower.contains("приказ") {
                Some("order")
            } else if lower.contains("акт") {
                Some("act")
            } else if lower.contains("договор") || lower.contains("контракт") {
                Some("contract")
            } else {
                None
            }?;
            extract_number_and_date(line, default_year).map(|facts| (role, facts))
        });

    if let Some((role, (number, date))) = header {
        put(case, report, "document.number", &number, 0.97);
        match role {
            "invoice" => put(case, report, "accounting.invoice_number", &number, 0.97),
            "order" => {
                put(case, report, "hr.order_number", &number, 0.97);
                put(case, report, "order.number", &number, 0.95);
            }
            "contract" => put(case, report, "contract.number", &number, 0.97),
            _ => {}
        }
        if let Some(date) = date {
            put(case, report, "document.date", &date, 0.95);
            match role {
                "invoice" => put(case, report, "accounting.invoice_date", &date, 0.95),
                "order" => put(case, report, "hr.order_date", &date, 0.95),
                "contract" => put(case, report, "contract.date", &date, 0.95),
                _ => {}
            }
        }
    }

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let lower = line.to_lowercase();
        if lower.contains("договор") || lower.contains("контракт") {
            if let Some((number, date)) = extract_number_and_date(line, default_year) {
                put(case, report, "contract.number", &number, 0.94);
                if let Some(date) = date {
                    put(case, report, "contract.date", &date, 0.90);
                }
                if lower.contains("трудов") {
                    put(case, report, "employee.contract_number", &number, 0.96);
                }
            }
        }

        if let Some(start) = lower.find("принять ") {
            let original_tail = &line[start + "принять ".len()..];
            if let Some(name) = first_person_name_triplet(original_tail) {
                put(case, report, "employee.name", &name, 0.94);
            }
            if let Some(position_start) = lower.find("на должность") {
                let value_start = position_start + "на должность".len();
                let position = trim_narrative_value(&line[value_start..], &[" с ", ", в ", ";"]);
                if !position.is_empty() {
                    put(case, report, "employee.position", &position, 0.90);
                }
            }
            if let Some(date_start) = lower.rfind(" с ") {
                if let Some(candidate) = first_date_candidate(&line[date_start + 3..]) {
                    if let Some(date) = parse_flexible_date(&candidate, default_year) {
                        put(case, report, "employee.hire_date", &date, 0.94);
                    }
                }
            }
        }
    }
}

fn extract_number_and_date(line: &str, default_year: i32) -> Option<(String, Option<String>)> {
    let number_start = line.find('№')? + '№'.len_utf8();
    let tail = line[number_start..].trim_start_matches([' ', ':', '-', '—']);
    let number = tail
        .split_whitespace()
        .next()?
        .trim_matches(['№', '.', ',', ';'])
        .to_string();
    if number.is_empty() || !number.chars().any(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let date = first_date_candidate(tail)
        .and_then(|candidate| parse_flexible_date(&candidate, default_year));
    Some((number, date))
}

fn first_person_name_triplet(value: &str) -> Option<String> {
    let tokens = value
        .split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphabetic() && ch != '-'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for window in tokens.windows(3) {
        if window.iter().all(|token| {
            token.chars().next().is_some_and(char::is_uppercase)
                && token
                    .chars()
                    .skip(1)
                    .all(|ch| ch.is_lowercase() || ch == '-')
        }) {
            return Some(window.join(" "));
        }
    }
    None
}

fn trim_narrative_value(value: &str, stops: &[&str]) -> String {
    let lower = value.to_lowercase();
    let end = stops
        .iter()
        .filter_map(|stop| lower.find(stop))
        .min()
        .unwrap_or(value.len());
    clean_value(&value[..end])
}

fn put(
    case: &mut SemanticCase,
    report: &mut ParsedSourceReport,
    field_id: &str,
    value: &str,
    confidence: f32,
) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let canonical = crate::canonical_storage_field_id(field_id);
    if let Err(error) = validate_field_value(&canonical, value) {
        report
            .warnings
            .push(format!("Поле «{canonical}» отклонено: {error}"));
        return;
    }
    if merge_value(
        case,
        SemanticValue::new(&canonical, value, ValueSource::Scanner, confidence).with_evidence(
            crate::ValueEvidence::new(
                "document_text",
                value,
                "deterministic_source_parser",
                confidence,
            ),
        ),
    ) && !report.filled_fields.contains(&canonical)
    {
        report.filled_fields.push(canonical);
    }
}

fn mirror_profile_fields(
    case: &mut SemanticCase,
    report: &mut ParsedSourceReport,
    domain: &crate::DomainKind,
) {
    match domain {
        crate::DomainKind::Hr => {
            mirror_first(case, report, "employee.name", &["subject.name"], 0.76);
            mirror_first(
                case,
                report,
                "employee.position",
                &["subject.position", "hr.position"],
                0.76,
            );
            mirror_first(
                case,
                report,
                "employee.department",
                &["hr.department"],
                0.76,
            );
            mirror_first(case, report, "employee.salary", &["hr.salary"], 0.76);
        }
        crate::DomainKind::Accounting => {
            mirror_first(
                case,
                report,
                "accounting.invoice_number",
                &["document.number"],
                0.72,
            );
            mirror_first(
                case,
                report,
                "accounting.invoice_date",
                &["document.date"],
                0.72,
            );
        }
        _ => {}
    }
}

fn mirror_first(
    case: &mut SemanticCase,
    report: &mut ParsedSourceReport,
    target: &str,
    sources: &[&str],
    confidence: f32,
) {
    if case.get(target).is_some() {
        return;
    }
    if let Some(value) = sources
        .iter()
        .find_map(|source| case.get(source))
        .map(str::to_owned)
    {
        put(case, report, target, &value, confidence);
    }
}

fn clean_inline_value(value: &str) -> String {
    let mut end = value.len();
    for (index, ch) in value.char_indices() {
        if !matches!(ch, ',' | ';' | '.') {
            continue;
        }
        let tail = value[index + ch.len_utf8()..].trim_start();
        if !tail.is_empty() && looks_like_known_label(tail) {
            end = index;
            break;
        }
    }
    if let Some(next_label) = next_explicit_inline_label_start(value) {
        end = end.min(next_label);
    }
    clean_value(&value[..end])
}

fn next_explicit_inline_label_start(value: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    for rule in generic_rules().into_iter().chain(medical_rules()) {
        for label in rule.labels {
            let Some(label_end) = find_label_end(value, label) else {
                continue;
            };
            let Some(label_start) = label_start_from_end(value, label, label_end) else {
                continue;
            };
            if label_start == 0 {
                continue;
            }
            let tail = value[label_end..].trim_start();
            let explicit_separator = tail
                .chars()
                .next()
                .is_some_and(|ch| matches!(ch, ':' | '№' | '#' | '-' | '—' | '–'));
            if !explicit_separator {
                continue;
            }
            best = Some(best.map_or(label_start, |current| current.min(label_start)));
        }
    }
    best
}

fn label_start_from_end(value: &str, label: &str, mut end: usize) -> Option<usize> {
    if !value.is_char_boundary(end) {
        return None;
    }
    for _ in label.chars() {
        end = value[..end].char_indices().next_back()?.0;
    }
    Some(end)
}

fn first_date_candidate(line: &str) -> Option<String> {
    let mut current = String::new();
    for ch in line.chars() {
        if ch.is_ascii_digit() || matches!(ch, '.' | '/' | '-') {
            current.push(ch);
        } else if current.chars().filter(|c| c.is_ascii_digit()).count() >= 4 {
            return Some(
                current
                    .trim_matches(|c: char| matches!(c, '.' | '/' | '-'))
                    .to_string(),
            );
        } else {
            current.clear();
        }
    }
    (current.chars().filter(|c| c.is_ascii_digit()).count() >= 4).then(|| {
        current
            .trim_matches(|c: char| matches!(c, '.' | '/' | '-'))
            .to_string()
    })
}

fn strip_leading_date_loose(line: &str) -> &str {
    if let Some(candidate) = first_date_candidate(line) {
        if line.starts_with(&candidate) {
            return line[candidate.len()..]
                .trim_start_matches([' ', '-', '—'])
                .trim();
        }
    }
    line
}

fn clean_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| matches!(c, ':' | ';' | '.' | ' '))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_person_name(value: &str) -> bool {
    let words = value.split_whitespace().collect::<Vec<_>>();
    words.len() >= 2
        && words.len() <= 4
        && words
            .iter()
            .all(|word| word.chars().next().is_some_and(|c| c.is_uppercase()))
        && !value.chars().any(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_default_year_reaches_core_only_date_alias() {
        let text = "Редакция 2026\ndocument.date: 14.07";
        let (case, _) = parse_source_text(text, 2025);
        assert_eq!(case.get("document.date"), Some("14.07.2025"));
    }

    #[test]
    fn unicode_lowercase_expansion_before_label_never_panics_or_mis_slices() {
        let text = "İ служебный префикс — Дата документа: 2026-05-12\nНомер документа: 42";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.date"), Some("12.05.2026"));
        assert_eq!(case.get("document.number"), Some("42"));
    }

    #[test]
    fn kelvin_sign_before_label_never_panics() {
        let value = find_labeled_value("K — Номер документа: A-17", &["Номер документа"], false);
        assert_eq!(value.as_deref(), Some("A-17"));
    }

    #[test]
    fn generic_fields_are_extracted_without_medical_assumptions() {
        let text = "ОТЧЁТ ПО ПРОЕКТУ\nНомер документа: PR-42\nСубъект: Иванов Иван Иванович\nНачало периода: 01.06.2026\nКонец периода: 30.06.2026\nПлан действий:\nПровести аудит\nПодготовить отчёт\nОрганизация: ООО Ромашка";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.number"), Some("PR-42"));
        assert_eq!(case.get("period.start_date"), Some("01.06.2026"));
        assert!(case.get("action.plan").unwrap().contains("Провести аудит"));
        assert!(case.get("medical.diagnosis").is_none());
    }
    #[test]
    fn birth_date_is_not_stolen_as_document_date_near_title() {
        let text = "КАРТОЧКА СОТРУДНИКА\nДата рождения: 10.05.1980\nДата документа: 14.07.2026";
        assert_ne!(
            detect_date_near_title(text, 2026).as_deref(),
            Some("10.05.1980")
        );
    }
    #[test]
    fn extracts_items_collection_from_tabular_text() {
        let text = "СПЕЦИФИКАЦИЯ
Наименование\tКоличество\tЦена\tСумма
Услуга аудита\t2\t1500,00\t3000,00
Подготовка отчёта\t1\t500,00\t500,00
Итого\t\t\t3500,00";
        let (case, report) = parse_source_text(text, 2026);
        let items = case.collection("items").expect("items collection");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].get("name").map(SemanticAtom::as_text).as_deref(),
            Some("Услуга аудита")
        );
        assert_eq!(
            items[0].get("price").map(SemanticAtom::as_text).as_deref(),
            Some("1500.00")
        );
        assert!(report
            .filled_fields
            .contains(&"collection.items".to_string()));
    }

    #[test]
    fn invalid_bank_account_is_rejected_after_bik_pair_check() {
        let text = "РЕКВИЗИТЫ
БИК: 044525225
Расчётный счёт: 40702810900000002851";
        let (case, report) = parse_source_text(text, 2026);
        assert_eq!(case.get("org.bank_bik"), Some("044525225"));
        assert!(case.get("org.bank_account").is_none());
        assert!(report
            .warnings
            .iter()
            .any(|warning| warning.contains("контрольный ключ")));
    }

    #[test]
    fn medical_values_are_mirrored_into_generic_core_fields() {
        let text = "ПЕРВИЧНЫЙ ОСМОТР\nИстория болезни № 123\nДата поступления: 01.06.2026\nДата выписки: 10.06.2026\nДиагноз: F32.1 Депрессивный эпизод\nЛечение: наблюдение";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.number"), Some("123"));
        assert_eq!(case.get("period.start_date"), Some("01.06.2026"));
        assert_eq!(
            case.get("classification.primary"),
            case.get("medical.diagnosis")
        );
        assert_eq!(case.get("action.plan"), case.get("medical.treatment"));
    }

    #[test]
    fn buyer_requisite_does_not_become_owner_requisite() {
        let text = "Заказчик: ООО Север, ИНН заказчика: 7707083893";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("counterparty.inn"), Some("7707083893"));
        assert_eq!(case.get("org.inn"), None);
        assert_eq!(case.get("org.name"), None);
    }

    #[test]
    fn role_aware_parties_do_not_overwrite_each_other() {
        let text = "АКТ ОКАЗАННЫХ УСЛУГ № 7\nИсполнитель: ООО Ромашка. Заказчик: ООО Север.";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("org.name"), Some("ООО Ромашка"));
        assert_eq!(case.get("counterparty.name"), Some("ООО Север"));
    }

    #[test]
    fn acceptance_act_keeps_act_and_contract_numbers_separate() {
        let text = "АКТ ОКАЗАННЫХ УСЛУГ № 7 от 21.07.2026\nк договору № D-15 от 01.07.2026\nИсполнитель: ООО «Ромашка», ИНН: 7736050003\nСумма: 125 000,00 руб.";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.number"), Some("7"));
        assert_eq!(case.get("contract.number"), Some("D-15"));
        assert_eq!(case.get("org.name"), Some("ООО «Ромашка»"));
        assert_eq!(case.get("amount.total"), Some("125\u{00A0}000,00"));
        assert_eq!(case.get("subject.phone"), None);
    }

    #[test]
    fn invoice_header_and_requisites_remain_role_separated() {
        let text = "СЧЁТ НА ОПЛАТУ № 88 от 21.07.2026
Поставщик: ООО «Вектор», ИНН: 7701234567, КПП: 770101001
Покупатель: ООО «Север»
Наименование Количество Цена Сумма
Услуга сопровождения 1 123 456,78 123 456,78
Итого: 123 456,78 руб.";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.number"), Some("88"));
        assert_eq!(case.get("accounting.invoice_number"), Some("88"));
        assert_eq!(case.get("accounting.invoice_date"), Some("21.07.2026"));
        assert_eq!(case.get("org.name"), Some("ООО «Вектор»"));
        assert_eq!(case.get("counterparty.name"), Some("ООО «Север»"));
        assert_eq!(case.get("amount.total"), Some("123\u{00A0}456,78"));
        assert_eq!(case.get("subject.phone"), None);
        assert_ne!(
            case.get("subject.name"),
            Some("Наименование Количество Цена Сумма")
        );
    }

    #[test]
    fn employment_order_extracts_narrative_roles_without_contract_collision() {
        let text = "ПРИКАЗ О ПРИЁМЕ НА РАБОТУ № 15-к от 21.07.2026\nПринять Иванова Ивана Ивановича на должность инженера с 01.08.2026\nТрудовой договор № ТД-77 от 20.07.2026";
        let (case, _) = parse_source_text(text, 2026);
        assert_eq!(case.get("document.number"), Some("15-к"));
        assert_eq!(case.get("hr.order_number"), Some("15-к"));
        assert_eq!(case.get("employee.contract_number"), Some("ТД-77"));
        assert_eq!(case.get("employee.name"), Some("Иванова Ивана Ивановича"));
        assert_eq!(case.get("employee.position"), Some("инженера"));
        assert_eq!(case.get("employee.hire_date"), Some("01.08.2026"));
    }
}
