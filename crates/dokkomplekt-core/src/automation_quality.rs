use crate::{
    validate_case_relations, validate_field_value, SemanticCase, SemanticValue, ValueSource,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl FieldRisk {
    pub fn minimum_confidence(self) -> f32 {
        match self {
            Self::Low => 0.80,
            Self::Medium => 0.90,
            Self::High => 0.98,
            Self::Critical => 0.995,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationBlocker {
    pub field_id: String,
    pub value: String,
    pub risk: FieldRisk,
    pub confidence: f32,
    pub required_confidence: f32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationQualityReport {
    pub ready: bool,
    pub checked_fields: usize,
    pub blockers: Vec<AutomationBlocker>,
}

/// Реестр классов риска. Единственный источник истины, который читает
/// и Rust-рантайм, и Python-калибровка (`scripts/measure_domain.py`).
///
/// До 18.4.0 это были две независимые реализации, и они расходились:
/// `subject.address` считался High в рантайме, но полностью выпадал
/// из измерения калибровкой. Подписанный артефакт калибровки поэтому
/// авторизовал автопечать для полей, ошибки на которых он не измерял.
const FIELD_RISK_REGISTRY: &str = include_str!("../../../resources/field_risk_registry.json");

/// Разобранный реестр: множества вместо линейного поиска по Vec.
struct RiskRule {
    tokens: BTreeSet<String>,
    prefixes: BTreeSet<String>,
    exact: BTreeSet<String>,
}

struct RiskRegistry {
    /// Порядок проверки, от строгого к мягкому.
    order: Vec<(FieldRisk, RiskRule)>,
    /// Реестр разобран успешно. Если нет — классификация обязана быть
    /// максимально строгой, а не максимально мягкой.
    parsed: bool,
}

fn string_set(value: Option<&serde_json::Value>) -> BTreeSet<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default()
}

/// Реестр разбирается ОДИН раз за процесс.
///
/// Предыдущая реализация вызывала `serde_json` и собирала `Vec<String>`
/// на каждое обращение — девять аллокаций на одно поле, а `field_risk`
/// вызывается для каждого поля каждого документа. Исходная реализация
/// 18.3.2 была вовсе без аллокаций, и терять это ради читаемости нельзя.
fn registry() -> &'static RiskRegistry {
    use std::sync::OnceLock;
    static PARSED: OnceLock<RiskRegistry> = OnceLock::new();
    PARSED.get_or_init(|| {
        let Ok(root) = serde_json::from_str::<serde_json::Value>(FIELD_RISK_REGISTRY) else {
            return RiskRegistry {
                order: Vec::new(),
                parsed: false,
            };
        };
        let named = |name: &str| -> RiskRule {
            let entry = root.get("rules").and_then(|rules| rules.get(name));
            RiskRule {
                tokens: string_set(entry.and_then(|rule| rule.get("tokens"))),
                prefixes: string_set(entry.and_then(|rule| rule.get("prefixes"))),
                exact: string_set(entry.and_then(|rule| rule.get("exact"))),
            }
        };
        let order = vec![
            (FieldRisk::Critical, named("critical")),
            (FieldRisk::High, named("high")),
            (FieldRisk::Medium, named("medium")),
        ];
        let parsed = order
            .iter()
            .any(|(_, rule)| !rule.tokens.is_empty() || !rule.prefixes.is_empty());
        RiskRegistry { order, parsed }
    })
}

pub fn field_risk(field_id: &str) -> FieldRisk {
    let lowered = field_id.trim().to_ascii_lowercase();
    let registry = registry();
    if !registry.parsed {
        // Встроенный реестр не разобрался. Деградация обязана идти
        // в сторону строгости: молча объявить все поля Low означало бы
        // открыть автоматизацию для ИНН и диагнозов. Тест
        // `registry_is_parseable_and_non_empty` не даёт этой ветке
        // случиться в поставке.
        return FieldRisk::Critical;
    }
    let segments = lowered.split('.').filter(|part| !part.is_empty());
    let mut words = Vec::new();
    let mut segment_list = Vec::new();
    for segment in segments {
        segment_list.push(segment);
        words.extend(segment.split('_').filter(|word| !word.is_empty()));
    }
    let first_segment = segment_list.first().copied().unwrap_or_default();

    for (risk, rule) in &registry.order {
        if rule.exact.contains(lowered.as_str()) {
            return *risk;
        }
        if words.iter().any(|word| rule.tokens.contains(*word))
            || segment_list
                .iter()
                .any(|segment| rule.tokens.contains(*segment))
        {
            return *risk;
        }
        if rule.prefixes.contains(first_segment) {
            return *risk;
        }
    }
    FieldRisk::Low
}

pub fn evaluate_automation_quality<'a>(
    case: &SemanticCase,
    field_ids: impl IntoIterator<Item = &'a str>,
) -> AutomationQualityReport {
    evaluate_automation_quality_with_floor(case, field_ids, CalibratedFloor::default())
}

pub fn evaluate_automation_quality_with_floor<'a>(
    case: &SemanticCase,
    field_ids: impl IntoIterator<Item = &'a str>,
    floor: CalibratedFloor,
) -> AutomationQualityReport {
    let unique = field_ids
        .into_iter()
        .map(str::trim)
        .filter(|field_id| !field_id.is_empty())
        .map(crate::canonical_storage_field_id)
        .collect::<BTreeSet<_>>();
    let mut blockers = Vec::new();
    for field_id in &unique {
        let Some(value) = case.value(field_id) else {
            continue;
        };
        if let Err(reason) = validate_field_value(field_id, &value.value) {
            blockers.push(AutomationBlocker {
                field_id: field_id.to_string(),
                value: value.value.clone(),
                risk: field_risk(field_id),
                confidence: value.confidence.clamp(0.0, 1.0),
                required_confidence: field_risk(field_id).minimum_confidence(),
                reason,
            });
            continue;
        }
        if let Some(blocker) = value_blocker_with_floor(value, floor) {
            blockers.push(blocker);
        }
    }
    for (field_id, reason) in validate_case_relations(case) {
        if unique.contains(field_id.as_str())
            && !blockers.iter().any(|blocker| blocker.field_id == field_id)
        {
            let value = case.value(&field_id);
            blockers.push(AutomationBlocker {
                field_id: field_id.clone(),
                value: value.map(|item| item.value.clone()).unwrap_or_default(),
                risk: field_risk(&field_id),
                confidence: value
                    .map(|item| item.confidence.clamp(0.0, 1.0))
                    .unwrap_or(0.0),
                required_confidence: field_risk(&field_id).minimum_confidence(),
                reason,
            });
        }
    }
    AutomationQualityReport {
        ready: blockers.is_empty(),
        checked_fields: unique.len(),
        blockers,
    }
}

/// Реквизиты с БЕЗУСЛОВНОЙ математической контрольной суммой.
///
/// Список намеренно короткий, и каждое исключение из него — не забывчивость.
///
/// * `validate_kpp` — проверка ФОРМАТА: принимает любые 9 символов нужного
///   вида. Случайный мусор проходит её с вероятностью около единицы.
/// * `validate_cadastral` — проверка ФОРМАТА: любые четыре группы цифр
///   через двоеточие.
/// * `validate_vin` — контрольная цифра считается ТОЛЬКО когда VIN начинается
///   с 1..=5 (Северная Америка). Для европейских и японских VIN функция
///   возвращает `Ok(())` после проверки формата. Условное доказательство
///   доказательством не является.
/// * `validate_bank_account_with_bik` — настоящая к.с., но требует двух полей
///   сразу, а этот шлюз видит одно значение. Подключать нужно уровнем выше.
///
/// Вероятность прохождения случайно неверного значения:
///   ИНН юрлица (1 к.ц.)  — 1/10
///   ИНН ИП (2 к.ц.)      — 1/100
///   СНИЛС (mod 101)      — ~1/101
///   ОГРН/ОГРНИП (mod 11) — ~1/11
fn checksum_verified(field_id: &str, value: &str) -> bool {
    let outcome = match checksum_class(field_id) {
        Some("inn") => crate::validate_inn(value),
        Some("snils") => crate::validate_snils(value),
        Some("ogrn") => crate::validate_ogrn(value),
        _ => return false,
    };
    outcome.is_ok()
}

/// Класс поля, если у него ЕСТЬ математическая контрольная сумма.
///
/// Единый источник истины для двух вопросов: «прошло ли значение к.с.»
/// (`checksum_verified`) и «есть ли у поля к.с. вообще»
/// (`field_has_checksum`). Держать их одним `match` обязательно —
/// иначе список к.с.-полей разъедется между двумя местами.
fn checksum_class(field_id: &str) -> Option<&'static str> {
    let class = field_id.trim().to_ascii_lowercase();
    let class = class.rsplit('.').next().unwrap_or_default();
    match class {
        "inn" => Some("inn"),
        "snils" => Some("snils"),
        "ogrn" | "ogrnip" => Some("ogrn"),
        _ => None,
    }
}

/// Есть ли у КЛАССА поля контрольная сумма (независимо от конкретного значения).
fn field_has_checksum(field_id: &str) -> bool {
    checksum_class(field_id).is_some()
}

/// Откалиброванный порог, заменяющий хардкод, ЕСЛИ он подтверждён подписью.
///
/// Это разрешает главный архитектурный разрыв: до 18.4.1 внутри
/// `evaluate_print_triage_with_thresholds` вызывался
/// `evaluate_automation_quality`, который брал жёсткие пороги реестра
/// (High 0.98 / Critical 0.995). Даже когда подписанная калибровка давала
/// `auto_min_confidence = 0.94`, внутренний хардкод-гейт уже успевал
/// выставить блокер, и калиброванный порог не мог вступить в силу никогда:
/// два гейта, верхний хардкод перекрывал нижний калиброванный.
///
/// Теперь калибровка передаётся внутрь. Важные ограничения безопасности
/// НЕ ослабляются:
///   * поле высокого риска без доказательства происхождения по-прежнему
///     блокируется независимо от порога;
///   * контрольная сумма по-прежнему заменяет порог как факт;
///   * калибровка применяется ТОЛЬКО при наличии `evidence_sha256`
///     (подписанный корпус); без него берётся консервативный хардкод.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalibratedFloor {
    /// Порог из подписанной калибровки. `None` => применять хардкод реестра.
    pub auto_min_confidence: Option<f32>,
    /// Калибровка подтверждена подписанным доказательством на корпусе.
    /// Без этого `auto_min_confidence` игнорируется — нельзя понижать порог
    /// неподписанным значением.
    pub evidence_backed: bool,
}

impl CalibratedFloor {
    /// Насколько калибровка вправе опустить порог Critical-поля БЕЗ
    /// контрольной суммы.
    ///
    /// Прежнее значение 0.90 было выбрано произвольно и оказалось слишком
    /// слабым: сквозной тест показал, что сумма платежа с уверенностью 0.92
    /// уходила в автопечать при щедрой калибровке. Для полей, ошибка в
    /// которых юридически необратима (сумма, диагноз, номер дела) и у которых
    /// НЕТ математической контрольной суммы, единственная защита — сама
    /// уверенность, поэтому калибровка не вправе опускать её ниже уровня,
    /// на котором ошибка становится редким событием.
    ///
    /// 0.98 — не подгонка: это тот же порог, что реестр назначает классу High.
    /// Логика: Critical без контрольной суммы не может быть надёжнее, чем
    /// самое строгое, что система вообще умеет измерять уверенностью (High).
    /// Опуститься ниже 0.995 калибровка вправе (иначе Critical недостижим
    /// никогда), но не ниже 0.98 — планки соседнего класса риска.
    const CRITICAL_UNVERIFIED_FLOOR: f32 = 0.98;

    /// Эффективный порог для класса риска.
    ///
    /// Калибровка может ЗАМЕНИТЬ хардкод, будучи подписанной, но не для всех
    /// полей одинаково:
    ///
    /// * Critical с пройденной контрольной суммой (ИНН/СНИЛС/ОГРН) вообще
    ///   не доходит сюда — `value_blocker` пропускает такое значение как
    ///   математический факт до сравнения с порогом.
    /// * Critical БЕЗ контрольной суммы (сумма, диагноз, номер дела)
    ///   калибровка может понизить лишь до планки High (0.98), не глубже.
    /// * High и ниже — калибровка свободна в пределах [0, 1].
    ///
    /// Аргумент `has_checksum` позволяет вызывающей стороне сообщить, что у
    /// конкретного поля контрольная сумма ЕСТЬ (даже если конкретное значение
    /// её не прошло) — тогда предохранитель не нужен, потому что защита уже
    /// обеспечена самим фактом проверки в `value_blocker`.
    fn effective(self, risk: FieldRisk, has_checksum: bool) -> f32 {
        let hardcoded = risk.minimum_confidence();
        match (self.evidence_backed, self.auto_min_confidence) {
            (true, Some(calibrated)) if calibrated.is_finite() => {
                // Исчерпывающий match намеренно: если в FieldRisk добавят
                // новый вариант, компилятор ПОТРЕБУЕТ решить, какой у него
                // предохранитель, а не присвоит молча ноль. Wildcard `_ => 0.0`
                // здесь был бы дырой ровно того класса, что раньше скрывал
                // незамеченные варианты ValueSource.
                let floor = match risk {
                    FieldRisk::Critical if !has_checksum => Self::CRITICAL_UNVERIFIED_FLOOR,
                    FieldRisk::Critical => 0.0,
                    FieldRisk::High | FieldRisk::Medium | FieldRisk::Low => 0.0,
                };
                calibrated.clamp(floor, 1.0)
            }
            _ => hardcoded,
        }
    }
}

pub fn value_blocker(value: &SemanticValue) -> Option<AutomationBlocker> {
    value_blocker_with_floor(value, CalibratedFloor::default())
}

pub fn value_blocker_with_floor(
    value: &SemanticValue,
    floor: CalibratedFloor,
) -> Option<AutomationBlocker> {
    let risk = field_risk(&value.field_id);
    let required = floor.effective(risk, field_has_checksum(&value.field_id));
    if matches!(value.source, ValueSource::UserConfirmed) {
        return None;
    }
    // Уверенность вне [0, 1] или не-число — признак поломки восходящего кода,
    // а не «высокая уверенность». Раньше `clamp` молча срезал 2.0 и +inf до
    // 1.0, и значение с невозможной уверенностью проходило порог (а при
    // валидной контрольной сумме — ещё и в обход порога). Такое значение
    // блокируется безусловно, до любого обхода: если экстрактор сообщает
    // невозможную уверенность, доверять нельзя и самому значению.
    if !value.confidence.is_finite() || !(0.0..=1.0).contains(&value.confidence) {
        return Some(AutomationBlocker {
            field_id: value.field_id.clone(),
            value: value.value.clone(),
            risk,
            confidence: value.confidence,
            required_confidence: required,
            reason: format!(
                "Недопустимая уверенность {}: значение отклонено как повреждённое.",
                value.confidence
            ),
        });
    }
    let confidence = value.confidence;
    if matches!(risk, FieldRisk::High | FieldRisk::Critical) && value.evidence.is_empty() {
        return Some(AutomationBlocker {
            field_id: value.field_id.clone(),
            value: value.value.clone(),
            risk,
            confidence,
            required_confidence: required,
            reason: "Высокорисковое поле не содержит проверяемого доказательства происхождения."
                .into(),
        });
    }
    // Сошедшаяся контрольная сумма — математический факт, а не эвристическая
    // оценка, и она заменяет числовой порог уверенности.
    //
    // До этого изменения `org.inn` с уверенностью 0.95 блокировался порогом
    // Critical 0.995, который не мог взять НИ ОДИН автоматический источник:
    // потолок детерминированного парсера 0.97, а semantic_engine.rs:465
    // и semantic_llm.rs:194 жёстко ограничены `.min(0.99)`. Сравнивалась
    // вписанная человеком константа с вероятностным порогом — величины
    // разной природы.
    //
    // Проверка НЕ отменяет требование доказательства происхождения выше:
    // контрольная сумма доказывает «это валидный ИНН», но не «это нужный ИНН».
    // Подмена ИНН поставщика на ИНН покупателя проходит контроль идеально,
    // поэтому происхождение по-прежнему обязано быть подтверждено.
    if checksum_verified(&value.field_id, &value.value) {
        return None;
    }
    if !confidence.is_finite() || confidence < required {
        return Some(AutomationBlocker {
            field_id: value.field_id.clone(),
            value: value.value.clone(),
            risk,
            confidence,
            required_confidence: required,
            reason: format!(
                "Поле риска {:?} нельзя использовать автоматически: уверенность {:.1}% ниже требуемых {:.1}%.",
                risk,
                confidence * 100.0,
                required * 100.0
            ),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemanticValue;
    use std::collections::BTreeMap;

    #[test]
    fn unsigned_calibration_cannot_lower_the_threshold() {
        // Порог 0.60 без подписанного доказательства обязан игнорироваться:
        // иначе кто угодно, подложив JSON, открыл бы автоматизацию.
        let value = SemanticValue::new("document.date", "01.02.2026", ValueSource::Scanner, 0.70)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "01.02.2026",
                "deterministic_source_parser",
                0.70,
            ));
        let floor = CalibratedFloor {
            auto_min_confidence: Some(0.60),
            evidence_backed: false,
        };
        // High требует 0.98; 0.70 < 0.98 -> блок, несмотря на «калибровку».
        assert!(value_blocker_with_floor(&value, floor).is_some());
    }

    #[test]
    fn signed_calibration_replaces_the_hardcoded_minimum() {
        let value = SemanticValue::new("document.date", "01.02.2026", ValueSource::Scanner, 0.95)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "01.02.2026",
                "deterministic_source_parser",
                0.95,
            ));
        // Хардкод 0.98 заблокировал бы 0.95.
        assert!(value_blocker(&value).is_some());
        // Подписанная калибровка 0.93 — пропускает.
        let floor = CalibratedFloor {
            auto_min_confidence: Some(0.93),
            evidence_backed: true,
        };
        assert!(value_blocker_with_floor(&value, floor).is_none());
    }

    #[test]
    fn calibration_cannot_drop_critical_below_the_safety_floor() {
        // Critical-поле БЕЗ контрольной суммы: калибровка не должна опускать
        // порог ниже планки High (0.98), какой бы корпус ни был предъявлен.
        // amount.total — сумма платежа, ошибка необратима, к.с. у неё нет.
        let value = SemanticValue::new("amount.total", "120000", ValueSource::Scanner, 0.92)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "120000",
                "deterministic_source_parser",
                0.92,
            ));
        assert_eq!(field_risk("amount.total"), FieldRisk::Critical);
        assert!(!field_has_checksum("amount.total"));
        let floor = CalibratedFloor {
            auto_min_confidence: Some(0.50),
            evidence_backed: true,
        };
        // Порог поднят страховкой до 0.98; 0.92 < 0.98 -> блок.
        let blocker = value_blocker_with_floor(&value, floor).expect("страховка Critical");
        assert!((blocker.required_confidence - 0.98).abs() < 1e-6);
    }

    #[test]
    fn calibration_may_lower_critical_that_has_a_checksum() {
        // Critical С контрольной суммой (ИНН) не нуждается в предохранителе:
        // защита обеспечена самим фактом проверки. Здесь проверяем, что
        // предохранитель к таким полям НЕ применяется — порог берётся
        // калиброванный. Значение с НЕверной к.с., чтобы дойти до сравнения.
        assert_eq!(field_risk("org.inn"), FieldRisk::Critical);
        assert!(field_has_checksum("org.inn"));
        // Неверный ИНН (к.с. не сходится) с уверенностью 0.95.
        let value = SemanticValue::new("org.inn", "7707083894", ValueSource::Scanner, 0.95)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "7707083894",
                "deterministic_source_parser",
                0.95,
            ));
        let floor = CalibratedFloor {
            auto_min_confidence: Some(0.93),
            evidence_backed: true,
        };
        // Предохранителя нет -> порог 0.93; 0.95 >= 0.93 -> пропуск.
        assert!(value_blocker_with_floor(&value, floor).is_none());
    }

    #[test]
    fn calibration_never_waives_the_provenance_requirement() {
        // Даже с щедрой подписанной калибровкой поле без доказательства
        // происхождения обязано блокироваться.
        let value = SemanticValue::new("document.date", "01.02.2026", ValueSource::Scanner, 0.99);
        let floor = CalibratedFloor {
            auto_min_confidence: Some(0.50),
            evidence_backed: true,
        };
        let blocker = value_blocker_with_floor(&value, floor).expect("нет происхождения — блок");
        assert!(blocker.reason.contains("доказательства происхождения"));
    }

    #[test]
    fn checksum_still_bypasses_regardless_of_calibration() {
        let value = SemanticValue::new("org.inn", "7707083893", ValueSource::Scanner, 0.10)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "ИНН 7707083893",
                "deterministic_source_parser",
                0.10,
            ));
        // Контрольная сумма — факт: проходит при любой калибровке и уверенности.
        assert!(value_blocker_with_floor(&value, CalibratedFloor::default()).is_none());
    }

    #[test]
    fn out_of_range_confidence_is_treated_as_corruption_not_high_confidence() {
        // Уверенность 2.0 и +inf раньше срезались clamp до 1.0 и ПРОХОДИЛИ
        // порог. Значение с невозможной уверенностью — признак поломки
        // восходящего кода и обязано блокироваться безусловно.
        for bad in [2.0_f32, f32::INFINITY, f32::NAN, -0.5, 100.0] {
            let value =
                SemanticValue::new("document.date", "01.01.2026", ValueSource::Scanner, bad)
                    .with_evidence(crate::ValueEvidence::new(
                        "document_text",
                        "01.01.2026",
                        "deterministic_source_parser",
                        0.9,
                    ));
            let blocker =
                value_blocker(&value).unwrap_or_else(|| panic!("conf={bad} должно блокироваться"));
            assert!(blocker.reason.contains("Недопустимая уверенность"));
        }
    }

    #[test]
    fn out_of_range_confidence_blocks_even_with_valid_checksum() {
        // Критично: невозможная уверенность обязана блокировать ДАЖE при
        // сошедшейся контрольной сумме — обход к.с. не должен спасать
        // повреждённое значение.
        let value = SemanticValue::new("org.inn", "7707083893", ValueSource::Scanner, 5.0)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "7707083893",
                "deterministic_source_parser",
                0.9,
            ));
        assert!(
            value_blocker(&value).is_some(),
            "невозможная уверенность 5.0 должна блокировать вопреки валидной к.с."
        );
    }

    #[test]
    fn valid_in_range_boundary_confidence_still_works() {
        // Границы диапазона 0.0 и 1.0 остаются валидными.
        let at_one = SemanticValue::new("org.inn", "7707083893", ValueSource::Scanner, 1.0)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "7707083893",
                "deterministic_source_parser",
                1.0,
            ));
        // Валидный ИНН при уверенности 1.0 — проходит по к.с.
        assert!(value_blocker(&at_one).is_none());
    }

    #[test]
    fn real_checksum_supersedes_the_unreachable_confidence_threshold() {
        // ИНН Сбербанка: контрольная сумма сходится.
        let value = SemanticValue::new("org.inn", "7707083893", ValueSource::Scanner, 0.95)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "ИНН 7707083893",
                "deterministic_source_parser",
                0.95,
            ));
        // Critical требует 0.995 — недостижимо ни для одного источника.
        assert_eq!(field_risk("org.inn"), FieldRisk::Critical);
        assert!(value.confidence < FieldRisk::Critical.minimum_confidence());
        assert!(value_blocker(&value).is_none(), "к.с. должна снимать порог");
    }

    #[test]
    fn broken_checksum_is_still_blocked() {
        let value = SemanticValue::new("org.inn", "7707083894", ValueSource::Scanner, 0.95)
            .with_evidence(crate::ValueEvidence::new(
                "document_text",
                "ИНН 7707083894",
                "deterministic_source_parser",
                0.95,
            ));
        assert!(value_blocker(&value).is_some());
    }

    #[test]
    fn checksum_does_not_waive_the_provenance_requirement() {
        // Контрольная сумма доказывает валидность, но не происхождение.
        let value = SemanticValue::new("org.inn", "7707083893", ValueSource::Scanner, 0.99);
        let blocker = value_blocker(&value).expect("без доказательства — блок");
        assert!(blocker.reason.contains("доказательства происхождения"));
    }

    #[test]
    fn format_checks_are_not_checksums_and_grant_nothing() {
        // validate_kpp и validate_cadastral — проверки формата: случайный
        // мусор нужной формы проходит их с вероятностью ~1. Доказательством
        // они не являются и порог снимать не должны.
        assert!(!checksum_verified("org.kpp", "770101001"));
        assert!(!checksum_verified("object.cadastral", "77:01:0004001:123"));
        // VIN исключён: контрольная цифра считается только для 1..=5.
        // Европейский VIN прошёл бы лишь проверку формата.
        assert!(!checksum_verified("vehicle.vin", "WVWZZZ1JZXW000001"));
        assert!(!checksum_verified("document.date", "01.02.2026"));
        assert!(!checksum_verified("counterparty.name", "ООО Ромашка"));
    }

    #[test]
    fn checksum_classes_are_matched_by_segment_not_substring() {
        assert!(checksum_verified("counterparty.inn", "7707083893"));
        assert!(checksum_verified("org.inn", "7707083893"));
        // "winner" содержит подстроку "inn", но сегментом не является.
        assert!(!checksum_verified("hr.winner", "7707083893"));
    }

    #[test]
    fn substring_false_positives_from_18_3_2_are_closed() {
        // Все шесть до 18.4.0 получали ложный Critical/High по подстроке.
        assert_eq!(field_risk("hr.winner_bonus"), FieldRisk::High); // prefix hr, не "inn"
        assert_eq!(field_risk("org.beginning_date"), FieldRisk::High); // word date, не "inn"
        assert_eq!(field_risk("spinner.state"), FieldRisk::Low);
        assert_eq!(field_risk("education.candidate_id"), FieldRisk::Low);
        assert_eq!(field_risk("task.update_flag"), FieldRisk::Low);
        assert_eq!(field_risk("doc.validated_by"), FieldRisk::Low);
    }

    #[test]
    fn genuine_risk_classes_are_not_weakened() {
        assert_eq!(field_risk("org.inn"), FieldRisk::Critical);
        assert_eq!(field_risk("amount.total"), FieldRisk::Critical);
        assert_eq!(field_risk("document.number"), FieldRisk::Critical);
        assert_eq!(field_risk("subject.birth_date"), FieldRisk::Critical);
        assert_eq!(field_risk("medical.diagnosis_code"), FieldRisk::Critical);
        assert_eq!(field_risk("org.bank_account"), FieldRisk::Critical);
        assert_eq!(field_risk("document.date"), FieldRisk::High);
        assert_eq!(field_risk("employee.hire_date"), FieldRisk::High);
        assert_eq!(field_risk("subject.address"), FieldRisk::High);
        assert_eq!(field_risk("org.name"), FieldRisk::Medium);
        assert_eq!(field_risk("employee.name"), FieldRisk::Low);
    }

    #[test]
    fn registry_is_parseable_and_non_empty() {
        // Битый JSON деградирует в «всё Critical» (fail-closed).
        // Этот тест не даёт такой поставке уйти незамеченной.
        assert!(registry().parsed, "встроенный реестр рисков не разобрался");
        let critical = &registry().order[0].1;
        assert!(!critical.tokens.is_empty());
        assert!(critical.tokens.contains("inn"));
        assert!(registry().order.len() == 3);
    }

    #[test]
    fn critical_medical_value_is_blocked_below_near_certainty() {
        let case = SemanticCase {
            values: BTreeMap::from([(
                "medical.diagnosis_code".into(),
                SemanticValue::new(
                    "medical.diagnosis_code",
                    "F20.0",
                    ValueSource::Scanner,
                    0.98,
                ),
            )]),
            ..Default::default()
        };
        let report = evaluate_automation_quality(&case, ["medical.diagnosis_code"]);
        assert!(!report.ready);
        assert_eq!(report.blockers[0].risk, FieldRisk::Critical);
    }

    #[test]
    fn legacy_required_id_checks_canonical_value_instead_of_skipping_it() {
        let case = SemanticCase {
            values: BTreeMap::from([(
                "medical.icd10".into(),
                SemanticValue::new("medical.icd10", "J45.0", ValueSource::Scanner, 0.70),
            )]),
            ..Default::default()
        };
        let report = evaluate_automation_quality(&case, ["medical.diagnosis_code"]);
        assert!(!report.ready);
        assert_eq!(report.blockers[0].field_id, "medical.icd10");
    }

    #[test]
    fn critical_value_with_evidence_and_near_certainty_is_allowed() {
        let value = SemanticValue::new(
            "medical.diagnosis_code",
            "F20.0",
            ValueSource::Scanner,
            0.999,
        )
        .with_evidence(crate::ValueEvidence::new(
            "document_text",
            "Диагноз F20.0",
            "label_parser",
            0.999,
        ));
        assert!(value_blocker(&value).is_none());
    }

    #[test]
    fn user_confirmed_critical_value_is_allowed() {
        let case = SemanticCase {
            values: BTreeMap::from([(
                "amount.total".into(),
                SemanticValue::new("amount.total", "100000.00", ValueSource::UserConfirmed, 1.0),
            )]),
            ..Default::default()
        };
        assert!(evaluate_automation_quality(&case, ["amount.total"]).ready);
    }

    #[test]
    fn low_risk_field_can_be_automated_at_eighty_percent() {
        let value = SemanticValue::new("custom.note", "Готово", ValueSource::Scanner, 0.80);
        assert!(value_blocker(&value).is_none());
    }
}
