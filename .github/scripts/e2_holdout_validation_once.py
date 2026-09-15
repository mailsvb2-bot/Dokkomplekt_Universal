from pathlib import Path

# Core: reserve the last matched pair as a true hold-out and make validation
# independent from confidence.
core = Path("crates/dokkomplekt-core/src/template_wizard.rs")
text = core.read_text(encoding="utf-8")

report_marker = "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct TemplateLearningReport {"
validation_types = '''#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateLearningValidationVerdict {
    pub passed: bool,
    pub holdout_pair_index: Option<usize>,
    pub evaluated_fields: usize,
    pub matched_fields: usize,
    pub intervention_fields: usize,
    pub intervention_matches: usize,
    pub mismatched_field_ids: Vec<String>,
    pub reasons: Vec<String>,
}

'''
if validation_types.strip() not in text:
    if report_marker not in text:
        raise SystemExit("TemplateLearningReport marker not found")
    text = text.replace(report_marker, validation_types + report_marker, 1)

old_report_tail = "    pub confidence: f32,\n    pub requires_confirmation: bool,\n    pub warnings: Vec<String>,"
new_report_tail = "    pub confidence: f32,\n    pub requires_confirmation: bool,\n    pub validation: TemplateLearningValidationVerdict,\n    pub warnings: Vec<String>,"
if old_report_tail not in text:
    raise SystemExit("TemplateLearningReport tail not found")
text = text.replace(old_report_tail, new_report_tail, 1)

old_fn = "pub fn learn_template_from_examples(input: &TemplateLearningInput) -> TemplateLearningReport {"
training_fn = "fn learn_template_from_training_examples(input: &TemplateLearningInput) -> TemplateLearningReport {"
if old_fn not in text:
    raise SystemExit("public learning function not found")
text = text.replace(old_fn, training_fn, 1)

wrapper = r'''pub fn learn_template_from_examples(input: &TemplateLearningInput) -> TemplateLearningReport {
    let pair_count = input.completed_examples.len();
    let counts_match = !input.source_examples.is_empty()
        && input.source_examples.len() == input.completed_examples.len();
    let pairs_are_usable = counts_match
        && input
            .completed_examples
            .iter()
            .zip(&input.source_examples)
            .all(|(completed, source)| !completed.trim().is_empty() && !source.trim().is_empty());

    if pairs_are_usable && (4..=10).contains(&pair_count) {
        let holdout_pair_index = pair_count - 1;
        let mut training = input.clone();
        let holdout_completed = training
            .completed_examples
            .pop()
            .expect("validated pair count must contain a hold-out output");
        let holdout_source = training
            .source_examples
            .pop()
            .expect("validated pair count must contain a hold-out source");
        let mut report = learn_template_from_training_examples(&training);
        report.validation = validate_template_learning_holdout(
            &report.fields,
            &holdout_source,
            &holdout_completed,
            input.default_year,
            holdout_pair_index,
        );
        if !report.validation.passed {
            report.warnings.push(format!(
                "Контрольная пара {} не доказала перенос карты; автоматическое применение должно быть заблокировано.",
                holdout_pair_index + 1
            ));
        }
        return report;
    }

    let mut safe_input = input.clone();
    let validation_reason = if input.source_examples.is_empty() {
        "Для доказательной проверки нужны пары Source → Correct Output; обучение только по готовым результатам не может быть опубликовано автоматически."
    } else if input.source_examples.len() != input.completed_examples.len() {
        safe_input.source_examples.clear();
        "Количество источников и правильных результатов не совпадает; доказательная проверка заблокирована."
    } else if !pairs_are_usable {
        safe_input.source_examples.clear();
        "Одна или несколько пар Source → Correct Output пусты; доказательная проверка заблокирована."
    } else if pair_count < 4 {
        "Для независимой проверки нужны минимум 4 пары: не менее 3 обучающих и 1 контрольная."
    } else {
        "Для одной серии поддерживается не более 10 пар; разделите примеры на отдельные серии."
    };
    let mut report = learn_template_from_training_examples(&safe_input);
    report.validation = unavailable_learning_validation(validation_reason);
    report.warnings.push(validation_reason.into());
    report
}

'''
if wrapper.strip() not in text:
    if training_fn not in text:
        raise SystemExit("training function marker not found")
    text = text.replace(training_fn, wrapper + training_fn, 1)

# Every internal report construction gets a conservative validation value; the
# public wrapper replaces it only after an independent hold-out check.
needle = "        requires_confirmation: true,\n        warnings,"
replacement = "        requires_confirmation: true,\n        validation: unavailable_learning_validation(\"Контрольная пара ещё не проверена.\"),\n        warnings,"
count = text.count(needle)
if count != 2:
    raise SystemExit(f"expected 2 report constructors, found {count}")
text = text.replace(needle, replacement)

helper_marker = "fn normalized_lines(text: &str) -> Vec<String> {"
helpers = r'''fn unavailable_learning_validation(reason: &str) -> TemplateLearningValidationVerdict {
    TemplateLearningValidationVerdict {
        passed: false,
        holdout_pair_index: None,
        evaluated_fields: 0,
        matched_fields: 0,
        intervention_fields: 0,
        intervention_matches: 0,
        mismatched_field_ids: Vec::new(),
        reasons: vec![reason.into()],
    }
}

fn validate_template_learning_holdout(
    fields: &[LearnedTemplateField],
    source_text: &str,
    completed_text: &str,
    default_year: i32,
    holdout_pair_index: usize,
) -> TemplateLearningValidationVerdict {
    let source = semantic_value_map(source_text, default_year);
    let completed_lines = normalized_lines(completed_text);
    let mut evaluated_fields = 0usize;
    let mut matched_fields = 0usize;
    let mut intervention_fields = 0usize;
    let mut intervention_matches = 0usize;
    let mut mismatched_field_ids = Vec::new();

    for field in fields.iter().filter(|field| !field.source_matches.is_empty()) {
        let Some(expected) = source.get(&field.field_id) else {
            continue;
        };
        evaluated_fields += 1;
        let intervention = !field.source_matches.iter().any(|training_value| {
            values_equivalent(&normalize_value(expected), training_value)
                || values_equivalent(&normalize_value(training_value), expected)
        });
        if intervention {
            intervention_fields += 1;
        }

        let observed = holdout_field_value(field, &completed_lines);
        let matched = observed.as_deref().is_some_and(|value| {
            values_equivalent(&normalize_value(value), expected)
                || values_equivalent(&normalize_value(expected), value)
        });
        if matched {
            matched_fields += 1;
            if intervention {
                intervention_matches += 1;
            }
        } else {
            mismatched_field_ids.push(field.field_id.clone());
        }
    }

    let mut reasons = Vec::new();
    if evaluated_fields == 0 {
        reasons.push("Контрольный источник не содержит ни одного поля, которому научилась карта.".into());
    }
    if !mismatched_field_ids.is_empty() {
        reasons.push(format!(
            "Контрольный результат не совпал с источником для {} полей.",
            mismatched_field_ids.len()
        ));
    }
    if intervention_fields == 0 {
        reasons.push(
            "Контрольная пара не содержит нового значения относительно обучающих пар; перенос, а не запоминание, не доказан."
                .into(),
        );
    }
    if intervention_fields > intervention_matches {
        reasons.push(
            "Хотя бы одно новое значение из контрольного источника не перенеслось в правильный результат."
                .into(),
        );
    }
    let passed = evaluated_fields > 0
        && matched_fields == evaluated_fields
        && intervention_fields > 0
        && intervention_matches == intervention_fields;
    if passed {
        reasons.push(
            "Контрольная пара доказала перенос нового значения Source → Correct Output без участия confidence."
                .into(),
        );
    }

    TemplateLearningValidationVerdict {
        passed,
        holdout_pair_index: Some(holdout_pair_index),
        evaluated_fields,
        matched_fields,
        intervention_fields,
        intervention_matches,
        mismatched_field_ids,
        reasons,
    }
}

fn holdout_field_value(field: &LearnedTemplateField, lines: &[String]) -> Option<String> {
    let line = lines.get(field.line_index)?;
    if !field.common_prefix.is_empty() && !line.starts_with(&field.common_prefix) {
        return None;
    }
    if !field.common_suffix.is_empty() && !line.ends_with(&field.common_suffix) {
        return None;
    }
    let value = variable_between(line, &field.common_prefix, &field.common_suffix);
    (!value.trim().is_empty()).then_some(value)
}

'''
if helpers.strip() not in text:
    if helper_marker not in text:
        raise SystemExit("normalized_lines marker not found")
    text = text.replace(helper_marker, helpers + helper_marker, 1)

# Add focused core regressions inside the existing tests module.
core_tests = r'''

    #[test]
    fn holdout_validation_passes_only_on_unseen_source_value_transfer() {
        let report = learn_template_from_examples(&TemplateLearningInput {
            blank_template_text: "Карточка\nИНН: __________".into(),
            completed_examples: vec![
                "Карточка\nИНН: 7736050003".into(),
                "Карточка\nИНН: 7707083893".into(),
                "Карточка\nИНН: 7812014560".into(),
                "Карточка\nИНН: 7708004767".into(),
            ],
            source_examples: vec![
                "Организация\nИНН: 7736050003".into(),
                "Организация\nИНН: 7707083893".into(),
                "Организация\nИНН: 7812014560".into(),
                "Организация\nИНН: 7708004767".into(),
            ],
            default_year: 2026,
            locale: "ru-RU".into(),
        });
        assert!(report.validation.passed, "{:?}", report.validation.reasons);
        assert_eq!(report.validation.holdout_pair_index, Some(3));
        assert!(report.validation.evaluated_fields >= 1);
        assert!(report.validation.intervention_fields >= 1);
        assert_eq!(
            report.validation.intervention_matches,
            report.validation.intervention_fields
        );
    }

    #[test]
    fn holdout_validation_fails_when_correct_output_does_not_follow_source() {
        let report = learn_template_from_examples(&TemplateLearningInput {
            blank_template_text: "Карточка\nИНН: __________".into(),
            completed_examples: vec![
                "Карточка\nИНН: 7736050003".into(),
                "Карточка\nИНН: 7707083893".into(),
                "Карточка\nИНН: 7812014560".into(),
                "Карточка\nИНН: 7736050003".into(),
            ],
            source_examples: vec![
                "Организация\nИНН: 7736050003".into(),
                "Организация\nИНН: 7707083893".into(),
                "Организация\nИНН: 7812014560".into(),
                "Организация\nИНН: 7708004767".into(),
            ],
            default_year: 2026,
            locale: "ru-RU".into(),
        });
        assert!(!report.validation.passed);
        assert!(!report.validation.mismatched_field_ids.is_empty());
    }

    #[test]
    fn three_pairs_can_learn_but_cannot_claim_independent_validation() {
        let report = learn_template_from_examples(&TemplateLearningInput {
            blank_template_text: "Карточка\nИНН: __________".into(),
            completed_examples: vec![
                "Карточка\nИНН: 7736050003".into(),
                "Карточка\nИНН: 7707083893".into(),
                "Карточка\nИНН: 7812014560".into(),
            ],
            source_examples: vec![
                "Организация\nИНН: 7736050003".into(),
                "Организация\nИНН: 7707083893".into(),
                "Организация\nИНН: 7812014560".into(),
            ],
            default_year: 2026,
            locale: "ru-RU".into(),
        });
        assert!(!report.fields.is_empty());
        assert!(!report.validation.passed);
        assert_eq!(report.validation.holdout_pair_index, None);
    }
'''
closing = "\n}\n"
index = text.rfind(closing)
if index < 0:
    raise SystemExit("core tests module closing brace not found")
text = text[:index] + core_tests + text[index:]
core.write_text(text, encoding="utf-8")

# TypeScript contract carries the canonical verdict to the UI.
types = Path("src/lib/types.ts")
text = types.read_text(encoding="utf-8")
type_marker = "export interface TemplateLearningReport {"
verdict_type = '''export interface TemplateLearningValidationVerdict {
  passed: boolean;
  holdout_pair_index?: number | null;
  evaluated_fields: number;
  matched_fields: number;
  intervention_fields: number;
  intervention_matches: number;
  mismatched_field_ids: string[];
  reasons: string[];
}

'''
if verdict_type.strip() not in text:
    if type_marker not in text:
        raise SystemExit("TS TemplateLearningReport marker not found")
    text = text.replace(type_marker, verdict_type + type_marker, 1)
old = "  confidence: number;\n  requires_confirmation: boolean;\n  warnings: string[];"
new = "  confidence: number;\n  requires_confirmation: boolean;\n  validation: TemplateLearningValidationVerdict;\n  warnings: string[];"
if old not in text:
    raise SystemExit("TS learning report tail not found")
text = text.replace(old, new, 1)
types.write_text(text, encoding="utf-8")

# UI boundary: independent verdict is mandatory before source evidence can be
# offered for explicit confirmation.
handler = Path("src/lib/pendingTemplateIntelligence.ts")
text = handler.read_text(encoding="utf-8")
helper = '''export function publicationEligibleLearningFields<T extends { source_matches: string[] }>(report: { fields: T[]; validation: { passed: boolean } }): T[] {
  if (!report.validation.passed) return [];
  return sourceEvidencedLearningFields(report.fields);
}

'''
marker = "export function createPendingTemplateIntelligenceHandlers(context: PendingTemplateIntelligenceContext) {"
if helper.strip() not in text:
    if marker not in text:
        raise SystemExit("handler marker not found")
    text = text.replace(marker, helper + marker, 1)
text = text.replace(
    "    if (pairs.length < 3 || pairs.length > 10) {\n      context.setStatus('Для обучения подготовьте от 3 до 10 пар: исходник → правильный готовый документ.');",
    "    if (pairs.length < 4 || pairs.length > 10) {\n      context.setStatus('Для доказательного обучения подготовьте от 4 до 10 пар: минимум 3 обучающие и 1 контрольная Source → Correct Output.');",
    1,
)
old = "    const evidencedFields = sourceEvidencedLearningFields(learned.fields);\n    if (!evidencedFields.length) {"
new = "    if (!learned.validation.passed) {\n      context.setStatus(`Контрольная пара не прошла независимую проверку. Шаблон не изменён. ${learned.validation.reasons.join(' ')}`);\n      return;\n    }\n\n    const evidencedFields = publicationEligibleLearningFields(learned);\n    if (!evidencedFields.length) {"
if old not in text:
    raise SystemExit("handler evidence block not found")
text = text.replace(old, new, 1)
text = text.replace(
    "Парами источник → правильный результат подтверждено полей: ${evidencedFields.length}.",
    "Независимая контрольная пара пройдена: ${learned.validation.matched_fields}/${learned.validation.evaluated_fields} проверяемых полей, новых значений перенесено ${learned.validation.intervention_matches}/${learned.validation.intervention_fields}. Подтверждено полей для карты: ${evidencedFields.length}.",
    1,
)
text = text.replace(
    "Шаблон обучен на ${pairs.length} парах: явно подтверждено и размечено полей — ${applied.applied_field_ids.length}.",
    "Шаблон обучен на ${pairs.length - 1} парах и проверен на 1 независимой контрольной паре: явно подтверждено и размечено полей — ${applied.applied_field_ids.length}.",
    1,
)
handler.write_text(text, encoding="utf-8")

# Modal explains the hold-out semantics instead of merely asking for examples.
modal = Path("src/components/TemplateSetupModal.tsx")
text = modal.read_text(encoding="utf-8")
text = text.replace("1. Источники (3–10)", "1. Источники (4–10)")
text = text.replace("2. Правильные результаты (3–10)", "2. Правильные результаты (4–10)")
text = text.replace("learningSources.length < 3", "learningSources.length < 4")
old_hint = "Выберите одинаковое число исходников и правильных результатов в одинаковом порядке. Программа сопоставляет каждую пару Source → Correct Output; одна высокая «уверенность» сама по себе ничего не публикует."
new_hint = "Выберите 4–10 пар в одинаковом порядке. Последняя пара резервируется как независимая контрольная и не участвует в обучении: карта должна перенести на ней новое значение Source → Correct Output. Одна высокая «уверенность» ничего не доказывает и не публикует."
if old_hint not in text:
    raise SystemExit("learning hint not found")
text = text.replace(old_hint, new_hint, 1)
modal.write_text(text, encoding="utf-8")

# Frontend regression: even perfect confidence/source evidence cannot bypass a
# failed independent verdict.
test = Path("src/lib/pendingTemplateIntelligence.test.ts")
text = test.read_text(encoding="utf-8")
text = text.replace(
    "import { sourceEvidencedLearningFields } from './pendingTemplateIntelligence';",
    "import { publicationEligibleLearningFields, sourceEvidencedLearningFields } from './pendingTemplateIntelligence';",
    1,
)
extra = '''

  it('blocks high-confidence source evidence when holdout validation failed', () => {
    const fields = publicationEligibleLearningFields({
      fields: [{ field_id: 'document.number', confidence: 0.99, source_matches: ['A-17'] }],
      validation: { passed: false },
    });
    expect(fields).toEqual([]);
  });

  it('allows source-evidenced fields only after a passed holdout verdict', () => {
    const fields = publicationEligibleLearningFields({
      fields: [
        { field_id: 'document.number', confidence: 0.42, source_matches: ['A-17'] },
        { field_id: 'amount.total', confidence: 0.99, source_matches: [] },
      ],
      validation: { passed: true },
    });
    expect(fields.map((field) => field.field_id)).toEqual(['document.number']);
  });
'''
last = text.rfind("\n});")
if last < 0:
    raise SystemExit("frontend test suite closing marker not found")
text = text[:last] + extra + text[last:]
test.write_text(text, encoding="utf-8")
