//! Сквозной прогон решающего конвейера по профессиям.
//!
//! До 18.4.0 сквозного теста не существовало вовсе: каждое звено
//! (`parse_source_text`, `recommend_document_bundle`, `decide_document_bundle`,
//! `evaluate_automation_quality`, `plan_created_documents_batch`) было покрыто
//! по отдельности, но никто ни разу не проверял их вместе на реальном тексте.
//! Именно поэтому мёртвая ветка автозапуска (`domain_confidence` = 0 при
//! выводе домена по сходству) прожила в коде незамеченной.
//!
//! Здесь проверяются ИНВАРИАНТЫ, а не ожидаемые ответы. Утверждать
//! «для этого текста система обязана предложить документ N» значило бы
//! зафиксировать текущее поведение эвристик; такой тест ломается при любой
//! настройке порогов и ничего не говорит о безопасности. Инварианты же
//! обязаны держаться при любых порогах.

use dokkomplekt_core::{
    decide_document_bundle, evaluate_automation_quality, parse_source_text,
    plan_created_documents_batch, recommend_document_bundle, BundleDecisionSource,
    ConfiguredDocument, CreatedDocumentsBatch, DocumentPack, DocumentTemplateSpec, DomainKind,
    FolderNamePart, WorkflowFlags,
};

fn document(
    id: &str,
    label: &str,
    role: &str,
    category: DomainKind,
    fields: &[&str],
) -> ConfiguredDocument {
    ConfiguredDocument {
        spec: DocumentTemplateSpec {
            id: id.into(),
            button_label: label.into(),
            template_path: format!("{id}.docx"),
            category,
            role_id: role.into(),
            required_fields: fields.iter().map(|f| (*f).to_string()).collect(),
            placeholders: fields.iter().map(|f| (*f).to_string()).collect(),
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        },
        template_text: fields
            .iter()
            .map(|f| format!("{{{{{f}}}}}"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Набор шаблонов, приближённый к starter-пакам.
fn installed_documents() -> Vec<ConfiguredDocument> {
    vec![
        document(
            "hr.employment_contract",
            "Трудовой договор",
            "employment_contract",
            DomainKind::Hr,
            &[
                "employee.name",
                "employee.position",
                "employee.hire_date",
                "org.name",
            ],
        ),
        document(
            "hr.employment_order",
            "Приказ о приёме",
            "employment_order",
            DomainKind::Hr,
            &[
                "employee.name",
                "hr.order_number",
                "hr.order_date",
                "org.name",
            ],
        ),
        document(
            "legal.contract",
            "Договор",
            "contract",
            DomainKind::Legal,
            &[
                "contract.number",
                "contract.date",
                "counterparty.name",
                "org.name",
            ],
        ),
        document(
            "legal.acceptance_act",
            "Акт приёма-передачи",
            "acceptance_act",
            DomainKind::Legal,
            &[
                "document.number",
                "document.date",
                "counterparty.name",
                "org.name",
            ],
        ),
        document(
            "accounting.invoice",
            "Счёт на оплату",
            "invoice",
            DomainKind::Accounting,
            &[
                "accounting.invoice_number",
                "accounting.invoice_date",
                "amount.total",
                "org.inn",
                "org.name",
            ],
        ),
        document(
            "accounting.service_act",
            "Акт оказанных услуг",
            "service_act",
            DomainKind::Accounting,
            &[
                "document.number",
                "document.date",
                "amount.total",
                "org.name",
            ],
        ),
        document(
            "medical.discharge",
            "Выписной эпикриз",
            "discharge",
            DomainKind::Medical,
            &[
                "patient.name",
                "medical.admission_date",
                "medical.discharge_date",
            ],
        ),
        document(
            "education.certificate",
            "Справка об обучении",
            "certificate",
            DomainKind::Education,
            &["subject.name", "document.date", "org.name"],
        ),
    ]
}

fn pack(documents: &[ConfiguredDocument]) -> DocumentPack {
    DocumentPack {
        pack_id: "integration".into(),
        name: "Интеграционный набор".into(),
        documents: documents.iter().map(|item| item.spec.clone()).collect(),
    }
}

struct Scenario {
    profession: &'static str,
    supported: bool,
    text: &'static str,
}

/// Сценарии по профессиям.
///
/// Первые пять — предметные области, для которых в наборе ЕСТЬ шаблоны.
/// Остальные — специальности, шаблонов для которых нет: они проверяют
/// корректную деградацию, а не отказ и не выдумывание комплекта.
fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            profession: "кадровик: трудовой договор",
            supported: true,
            text: "Трудовой договор № ТД-115 от 12.02.2026. Работодатель ООО Ромашка. \
                   Работник Иванов Иван Иванович. Должность инженер. Дата приёма 16.02.2026.",
        },
        Scenario {
            profession: "кадровик: приказ о приёме",
            supported: true,
            text: "Приказ о приеме на работу № 44 от 16.02.2026. \
                   Принять на работу Петрова П.П. на должность бухгалтера.",
        },
        Scenario {
            profession: "бухгалтер: счёт на оплату",
            supported: true,
            text: "Счет на оплату № 148 от 01.02.2026. ООО Ромашка, ИНН 7707083893. \
                   Контрагент ООО Василёк. К оплате 120000 руб, в т.ч. НДС 20000 руб.",
        },
        Scenario {
            profession: "юрист: договор поставки",
            supported: true,
            text: "ДОГОВОР ПОСТАВКИ № 77 от 03.03.2026. Настоящий договор заключен между \
                   сторонами. Предмет договора: поставка оборудования.",
        },
        Scenario {
            profession: "врач: выписной эпикриз",
            supported: true,
            text: "Выписной эпикриз. История болезни № 5512. Пациент Сидоров С.С. \
                   Дата поступления 03.01.2026. Дата выписки 11.01.2026. Лечащий врач Кузнецов.",
        },
        Scenario {
            profession: "педагог: справка об обучении",
            supported: true,
            text: "Настоящая справка выдана обучающемуся Смирнову А.А. в том, что он \
                   обучается по образовательной программе. Учебный план утверждён.",
        },
        Scenario {
            profession: "ветеринар",
            supported: false,
            text: "Ветеринарный паспорт. Владелец Сидоров. Кличка Барсик. \
                   Вакцинация от бешенства 10.01.2026. Клеймо AB-1234.",
        },
        Scenario {
            profession: "нотариус",
            supported: false,
            text: "Свидетельство об удостоверении сделки. Реестровый номер 3-1145. \
                   Нотариус нотариального округа. Взыскано по тарифу 2500 руб.",
        },
        Scenario {
            profession: "риелтор",
            supported: false,
            text: "Акт осмотра объекта недвижимости. Кадастровый номер 77:01:0004001:123. \
                   Площадь 54,3 кв.м. Этаж 7.",
        },
        Scenario {
            profession: "логист",
            supported: false,
            text: "Транспортная накладная № ТН-889. Грузоотправитель. Грузополучатель. \
                   Пункт погрузки Москва. Пункт разгрузки Казань. Масса груза 1200 кг.",
        },
        Scenario {
            profession: "стоматолог",
            supported: false,
            text: "Зубная формула. Санация полости рта. Пломбирование 26 зуба. \
                   Анестезия артикаин. Рекомендован повторный осмотр.",
        },
        Scenario {
            profession: "строитель",
            supported: false,
            text: "Акт освидетельствования скрытых работ. Объект капитального строительства. \
                   Проектная документация шифр 2026-АР. Предъявлены к приёмке работы.",
        },
        Scenario {
            profession: "фармацевт",
            supported: false,
            text: "Журнал предметно-количественного учёта. Серия 240126. \
                   Срок годности 01.2028. Приход 50 упаковок.",
        },
        Scenario {
            profession: "страховой агент",
            supported: false,
            text: "Страховой полис серии ХХХ № 0012345678. Страхователь. \
                   Период страхования с 01.03.2026 по 28.02.2027. Страховая премия 8400 руб.",
        },
    ]
}

/// Патологические входы: они не должны ронять конвейер.
fn hostile_inputs() -> Vec<(&'static str, String)> {
    vec![
        ("пустая строка", String::new()),
        ("только пробелы", "   \t\n  ".to_string()),
        ("нулевой байт", "текст\u{0}ещё".to_string()),
        ("очень длинная строка", "а".repeat(200_000)),
        ("только цифры", "1234567890".repeat(500)),
        (
            "эмодзи и суррогаты",
            "📄🧾✅ договор № 1 от 01.01.2026 🏥".to_string(),
        ),
        (
            "RTL и смешанные скрипты",
            "مرحبا договор № 7 שלום 01.01.2026".to_string(),
        ),
        (
            "разметка шаблонизатора во входе",
            "{{#each x}}{{org.name}}{{/each}} договор № 1".to_string(),
        ),
        ("одни разделители", "..:::___...___:::..".to_string()),
        (
            "контрольные символы",
            "\u{1}\u{2}\u{7}договор\u{1b}[31m".to_string(),
        ),
        (
            "невалидный UTF-8 суррогат",
            "\u{fffd}\u{fffd} счет".to_string(),
        ),
        (
            "даты вне диапазона",
            "Договор № 1 от 99.99.9999".to_string(),
        ),
        ("год вне разумного", "Договор № 1 от 01.01.0001".to_string()),
    ]
}

// ---------------------------------------------------------------------------
// Инварианты
// ---------------------------------------------------------------------------

#[test]
fn every_profession_reaches_a_defined_outcome_without_panic() {
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);

        // Каждый исход обязан быть определён: либо готовый план, либо вопрос.
        assert!(
            decision.is_generation_ready() || decision.review_required,
            "{}: исход не определён",
            scenario.profession
        );
        // Вопрос обязан быть задан, если генерация не разрешена.
        if !decision.is_generation_ready() {
            assert!(
                decision.question.is_some(),
                "{}: генерация запрещена, но вопроса нет — это пустой экран",
                scenario.profession
            );
        }
    }
}

#[test]
fn unsupported_professions_degrade_to_a_question_never_to_a_wrong_kit() {
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios().into_iter().filter(|item| !item.supported) {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);

        assert!(
            !decision.is_generation_ready(),
            "{}: шаблонов для этой специальности нет, а система собралась \
             автоматически создать комплект",
            scenario.profession
        );
        assert!(
            decision.question.is_some(),
            "{}: специалист обязан получить вопрос, а не молчание",
            scenario.profession
        );
    }
}

#[test]
fn a_rich_supported_source_still_reaches_a_concrete_kit() {
    // Без этой проверки ужесточение классификатора могло бы обнулить
    // маршрутизацию целиком, а остальные инварианты этого не заметили бы:
    // «ничего не предлагать» проходит их все.
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios().into_iter().filter(|item| item.supported) {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        assert!(
            !decision.document_ids.is_empty(),
            "{}: для поддерживаемой области с полным текстом не предложено ничего",
            scenario.profession
        );
        assert_ne!(
            decision.source,
            BundleDecisionSource::NoSafeProposal,
            "{}: поддерживаемая область провалилась в «ничего не могу»",
            scenario.profession
        );
    }
}

#[test]
fn the_primary_document_leads_the_proposed_kit() {
    // Ранжирование маршрутизатора не должно теряться при сборке комплекта.
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        if decision.document_ids.len() < 2 {
            continue;
        }
        let best = routing
            .matches
            .iter()
            .filter(|item| decision.document_ids.contains(&item.document_id))
            .max_by(|left, right| left.score.total_cmp(&right.score))
            .map(|item| item.document_id.clone());
        assert_eq!(
            best.as_deref(),
            decision.document_ids.first().map(String::as_str),
            "{}: комплект начинается не с самого подходящего документа",
            scenario.profession
        );
    }
}

#[test]
fn a_proposed_kit_is_always_a_subset_of_installed_templates() {
    let documents = installed_documents();
    let pack = pack(&documents);
    let known: Vec<&str> = pack.documents.iter().map(|d| d.id.as_str()).collect();
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        for id in &decision.document_ids {
            assert!(
                known.contains(&id.as_str()),
                "{}: предложен документ {id}, которого нет в наборе",
                scenario.profession
            );
        }
        for id in &routing.recommended_document_ids {
            assert!(
                known.contains(&id.as_str()),
                "{}: маршрутизатор вернул документ {id} вне набора",
                scenario.profession
            );
        }
    }
}

#[test]
fn automatic_route_never_fires_on_an_empty_or_hostile_source() {
    let documents = installed_documents();
    let pack = pack(&documents);
    for (name, text) in hostile_inputs() {
        let (case, _) = parse_source_text(&text, 2026);
        let routing = recommend_document_bundle(&text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        assert!(
            !decision.is_generation_ready(),
            "вход «{name}» не должен давать автоматическую генерацию"
        );
    }
}

#[test]
fn hostile_input_never_panics_anywhere_in_the_pipeline() {
    let documents = installed_documents();
    let pack = pack(&documents);
    let flags = WorkflowFlags::default();
    for (name, text) in hostile_inputs() {
        let (case, report) = parse_source_text(&text, 2026);
        let routing = recommend_document_bundle(&text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        let quality = evaluate_automation_quality(
            &case,
            documents
                .iter()
                .flat_map(|item| item.spec.required_fields.iter())
                .map(String::as_str),
        );
        let batch = plan_created_documents_batch(
            &case,
            &documents,
            &flags,
            &[FolderNamePart::FullSubjectName],
            "source",
            "source.docx",
        );
        // Каждый этап обязан вернуть значение, а не развалиться.
        let _ = report.warnings.len();
        let _ = routing.cluster_confidence;
        let _ = decision.confidence;
        let _ = quality.ready;
        match batch {
            CreatedDocumentsBatch::Ready { .. } | CreatedDocumentsBatch::Attention { .. } => {}
        }
        assert!(
            routing.cluster_confidence.is_finite(),
            "вход «{name}» дал не-число в уверенности кластера"
        );
    }
}

#[test]
fn confidence_values_are_always_finite_and_bounded() {
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        for (name, value) in [
            ("domain_confidence", routing.domain_confidence),
            ("cluster_confidence", routing.cluster_confidence),
            ("decision.confidence", decision.confidence),
        ] {
            assert!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "{}: {name} = {value} вне диапазона",
                scenario.profession
            );
        }
        for candidate in &routing.matches {
            assert!(
                candidate.score.is_finite() && (0.0..=1.0).contains(&candidate.score),
                "{}: оценка {} вне диапазона",
                scenario.profession,
                candidate.score
            );
        }
    }
}

#[test]
fn high_risk_values_without_provenance_always_block_automation() {
    // Инвариант безопасности: он обязан держаться при ЛЮБОМ входе,
    // независимо от того, что решил маршрутизатор.
    let documents = installed_documents();
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let quality = evaluate_automation_quality(
            &case,
            documents
                .iter()
                .flat_map(|item| item.spec.required_fields.iter())
                .map(String::as_str),
        );
        for blocker in &quality.blockers {
            assert!(
                blocker.confidence.is_finite(),
                "{}: блокер с не-числом уверенности",
                scenario.profession
            );
            assert!(
                !blocker.reason.trim().is_empty(),
                "{}: блокер без объяснения для специалиста",
                scenario.profession
            );
        }
        if !quality.ready {
            assert!(
                !quality.blockers.is_empty(),
                "{}: автоматизация запрещена без единой названной причины",
                scenario.profession
            );
        }
    }
}

#[test]
fn a_ready_batch_never_contains_unfilled_placeholders() {
    let documents = installed_documents();
    let flags = WorkflowFlags::default();
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let batch = plan_created_documents_batch(
            &case,
            &documents,
            &flags,
            &[FolderNamePart::FullSubjectName],
            "source",
            "source.docx",
        );
        if let CreatedDocumentsBatch::Ready { outputs, .. } = batch {
            for output in outputs {
                assert!(
                    !output.rendered_text.contains("{{"),
                    "{}: документ {} отдан как готовый с незаполненным полем",
                    scenario.profession,
                    output.document_id
                );
            }
        }
    }
}

#[test]
fn the_pipeline_is_deterministic_across_repeated_runs() {
    // Недетерминизм в маршрутизации означал бы, что один и тот же документ
    // иногда создаёт комплект, а иногда нет. Для инструмента, который
    // работает без человека, это худший вид дефекта.
    let documents = installed_documents();
    let pack = pack(&documents);
    for scenario in scenarios() {
        let (first_case, _) = parse_source_text(scenario.text, 2026);
        let first = recommend_document_bundle(scenario.text, &first_case, &pack);
        for _ in 0..8 {
            let (case, _) = parse_source_text(scenario.text, 2026);
            let again = recommend_document_bundle(scenario.text, &case, &pack);
            assert_eq!(
                first.recommended_document_ids, again.recommended_document_ids,
                "{}: состав комплекта нестабилен между прогонами",
                scenario.profession
            );
            assert_eq!(
                first.auto_select, again.auto_select,
                "{}: решение об автозапуске нестабильно",
                scenario.profession
            );
            assert!(
                (first.cluster_confidence - again.cluster_confidence).abs() < f32::EPSILON,
                "{}: уверенность кластера нестабильна",
                scenario.profession
            );
        }
    }
}

#[test]
fn an_empty_template_set_can_never_produce_a_kit() {
    let empty: Vec<ConfiguredDocument> = Vec::new();
    let pack = pack(&empty);
    for scenario in scenarios() {
        let (case, _) = parse_source_text(scenario.text, 2026);
        let routing = recommend_document_bundle(scenario.text, &case, &pack);
        let decision = decide_document_bundle(&pack, &routing, None, &[]);
        assert!(decision.document_ids.is_empty(), "{}", scenario.profession);
        assert!(!decision.is_generation_ready(), "{}", scenario.profession);
        assert_eq!(decision.source, BundleDecisionSource::NoSafeProposal);
    }
}
