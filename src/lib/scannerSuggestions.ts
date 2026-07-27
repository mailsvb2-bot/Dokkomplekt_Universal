import type { DocumentTemplateSpec, DomainKind, LearnedScannerRule, PromptInputKind } from './types';

export interface ScannerFieldSuggestion {
  field_id: string;
  title: string;
  confidence: number;
  reason: string;
  input_kind: PromptInputKind;
  destinations: string[];
  existing: boolean;
}

interface SuggestionInput {
  selectedText: string;
  contextText?: string;
  documents?: DocumentTemplateSpec[];
  activeDocumentId?: string | null;
  domainHint?: DomainKind | null;
  learnedRules?: LearnedScannerRule[];
}

type FieldDefinition = {
  field_id: string;
  title: string;
  kind: PromptInputKind;
  domains: string[];
  keywords: string[];
  valueTest?: (value: string) => boolean;
};

const DATE_RE = /^(?:0?[1-9]|[12]\d|3[01])[.\/-](?:0?[1-9]|1[0-2])[.\/-](?:\d{2}|\d{4})$/;
const MONEY_RE = /^-?\s*\d{1,3}(?:[\s\u00a0]\d{3})*(?:[.,]\d{1,2})?\s*(?:₽|руб(?:\.|лей|ля)?|р\.?|RUB)?$/i;
const PHONE_RE = /^\+?[\d\s()\-]{10,20}$/;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const VIN_RE = /^[A-HJ-NPR-Z0-9]{17}$/i;
const SNILS_RE = /^\d{3}[- ]?\d{3}[- ]?\d{3}[- ]?\d{2}$/;
const PASSPORT_RE = /^\d{2}\s?\d{2}\s?\d{6}$/;
const INN_RE = /^(?:\d[\s-]?){10}(?:(?:\d[\s-]?){2})?$/;
const KPP_RE = /^\d{4}[0-9A-Z]{2}\d{3}$/i;
const OGRN_RE = /^\d{13}(?:\d{2})?$/;
const ICD_RE = /^[A-ZА-Я]\s?\d{2}(?:[.,]\d+)?(?:\s+.+)?$/i;
const NUMBER_RE = /^[A-ZА-Я0-9][A-ZА-Я0-9/_.\-]{0,40}$/i;
const FIO_RE = /^(?:[А-ЯЁA-Z][а-яёa-z'’\-]+\s+){1,3}[А-ЯЁA-Z][а-яёa-z'’\-]+$/;

const FIELDS: FieldDefinition[] = [
  { field_id: 'document.number', title: 'Номер документа', kind: 'text', domains: ['generic'], keywords: ['номер документа', 'документ №', 'регистрационный номер', 'исх. №', 'исх №'], valueTest: NUMBER_RE.test.bind(NUMBER_RE) },
  { field_id: 'document.date', title: 'Дата документа', kind: 'date', domains: ['generic'], keywords: ['дата документа', 'дата составления', 'от'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'subject.name', title: 'ФИО / наименование человека', kind: 'text', domains: ['generic', 'legal', 'hr', 'education', 'medical'], keywords: ['фио', 'ф.и.о', 'гражданин', 'сотрудник', 'работник', 'обучающийся', 'студент', 'пациент'], valueTest: FIO_RE.test.bind(FIO_RE) },
  { field_id: 'subject.birth_date', title: 'Дата рождения', kind: 'date', domains: ['hr', 'education', 'medical', 'legal'], keywords: ['дата рождения', 'родился', 'родилась', 'г.р.'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'subject.address', title: 'Адрес', kind: 'long_text', domains: ['generic', 'legal', 'hr', 'medical'], keywords: ['адрес', 'место жительства', 'зарегистрирован', 'проживает'] },
  { field_id: 'subject.phone', title: 'Телефон', kind: 'text', domains: ['generic'], keywords: ['телефон', 'тел.', 'мобильный'], valueTest: PHONE_RE.test.bind(PHONE_RE) },
  { field_id: 'subject.email', title: 'Электронная почта', kind: 'text', domains: ['generic'], keywords: ['e-mail', 'email', 'электронная почта', 'почта'], valueTest: EMAIL_RE.test.bind(EMAIL_RE) },
  { field_id: 'subject.snils', title: 'СНИЛС', kind: 'snils', domains: ['hr', 'medical'], keywords: ['снилс'], valueTest: SNILS_RE.test.bind(SNILS_RE) },
  { field_id: 'subject.passport', title: 'Паспорт', kind: 'passport', domains: ['legal', 'hr'], keywords: ['паспорт', 'серия и номер'], valueTest: PASSPORT_RE.test.bind(PASSPORT_RE) },
  { field_id: 'org.name', title: 'Организация', kind: 'text', domains: ['generic', 'legal', 'hr', 'accounting', 'education', 'medical'], keywords: ['организация', 'наименование организации', 'работодатель', 'поставщик', 'исполнитель', 'учреждение'] },
  { field_id: 'counterparty.name', title: 'Контрагент', kind: 'text', domains: ['legal', 'accounting'], keywords: ['контрагент', 'заказчик', 'покупатель', 'получатель', 'плательщик'] },
  { field_id: 'counterparty.inn', title: 'ИНН контрагента', kind: 'inn', domains: ['legal', 'accounting'], keywords: ['инн контрагента', 'инн заказчика', 'инн покупателя'], valueTest: INN_RE.test.bind(INN_RE) },
  { field_id: 'counterparty.kpp', title: 'КПП контрагента', kind: 'kpp', domains: ['legal', 'accounting'], keywords: ['кпп контрагента', 'кпп заказчика', 'кпп покупателя'], valueTest: KPP_RE.test.bind(KPP_RE) },
  { field_id: 'org.inn', title: 'ИНН', kind: 'inn', domains: ['legal', 'accounting', 'hr'], keywords: ['инн'], valueTest: INN_RE.test.bind(INN_RE) },
  { field_id: 'org.kpp', title: 'КПП', kind: 'kpp', domains: ['legal', 'accounting'], keywords: ['кпп'], valueTest: KPP_RE.test.bind(KPP_RE) },
  { field_id: 'org.ogrn', title: 'ОГРН / ОГРНИП', kind: 'ogrn', domains: ['legal', 'accounting'], keywords: ['огрн', 'огрнип'], valueTest: OGRN_RE.test.bind(OGRN_RE) },
  { field_id: 'contract.number', title: 'Номер договора', kind: 'text', domains: ['legal'], keywords: ['договор №', 'номер договора', 'контракт №', 'соглашение №'], valueTest: NUMBER_RE.test.bind(NUMBER_RE) },
  { field_id: 'contract.date', title: 'Дата договора', kind: 'date', domains: ['legal'], keywords: ['дата договора', 'договор от', 'контракт от'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'contract.party_a', title: 'Первая сторона договора', kind: 'text', domains: ['legal'], keywords: ['заказчик', 'арендодатель', 'продавец', 'сторона 1', 'первая сторона'] },
  { field_id: 'contract.party_b', title: 'Вторая сторона договора', kind: 'text', domains: ['legal'], keywords: ['исполнитель', 'арендатор', 'покупатель', 'сторона 2', 'вторая сторона'] },
  { field_id: 'contract.subject', title: 'Предмет договора', kind: 'long_text', domains: ['legal'], keywords: ['предмет договора', 'предмет контракта'] },
  { field_id: 'amount.total', title: 'Итоговая сумма', kind: 'money', domains: ['legal', 'accounting'], keywords: ['итого', 'общая сумма', 'сумма договора', 'к оплате', 'всего'], valueTest: MONEY_RE.test.bind(MONEY_RE) },
  { field_id: 'amount.vat', title: 'НДС', kind: 'money', domains: ['accounting', 'legal'], keywords: ['ндс', 'в том числе ндс'], valueTest: MONEY_RE.test.bind(MONEY_RE) },
  { field_id: 'period.start_date', title: 'Дата начала', kind: 'date', domains: ['generic', 'legal', 'hr', 'education', 'medical'], keywords: ['дата начала', 'срок с', 'начало', 'приступить с', 'поступил', 'поступила'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'period.end_date', title: 'Дата окончания', kind: 'date', domains: ['generic', 'legal', 'hr', 'education', 'medical'], keywords: ['дата окончания', 'срок по', 'окончание', 'до', 'выписан', 'выписана'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'hr.order_number', title: 'Номер приказа', kind: 'text', domains: ['hr'], keywords: ['приказ №', 'номер приказа'], valueTest: NUMBER_RE.test.bind(NUMBER_RE) },
  { field_id: 'hr.order_date', title: 'Дата приказа', kind: 'date', domains: ['hr'], keywords: ['дата приказа', 'приказ от'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'employee.position', title: 'Должность', kind: 'text', domains: ['hr', 'medical'], keywords: ['должность', 'принять на должность', 'работает'] },
  { field_id: 'employee.department', title: 'Подразделение', kind: 'text', domains: ['hr'], keywords: ['подразделение', 'отдел', 'департамент'] },
  { field_id: 'employee.salary', title: 'Оклад / заработная плата', kind: 'money', domains: ['hr'], keywords: ['оклад', 'заработная плата', 'тарифная ставка'], valueTest: MONEY_RE.test.bind(MONEY_RE) },
  { field_id: 'accounting.invoice_number', title: 'Номер счёта', kind: 'text', domains: ['accounting'], keywords: ['счёт №', 'счет №', 'номер счёта', 'номер счета'], valueTest: NUMBER_RE.test.bind(NUMBER_RE) },
  { field_id: 'accounting.invoice_date', title: 'Дата счёта', kind: 'date', domains: ['accounting'], keywords: ['дата счёта', 'дата счета', 'счёт от', 'счет от'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'education.student_name', title: 'ФИО обучающегося', kind: 'text', domains: ['education'], keywords: ['обучающийся', 'студент', 'ученик', 'слушатель'], valueTest: FIO_RE.test.bind(FIO_RE) },
  { field_id: 'education.group', title: 'Группа / класс', kind: 'text', domains: ['education'], keywords: ['группа', 'класс', 'курс'] },
  { field_id: 'medical.case_number', title: 'Номер записи / дела', kind: 'text', domains: ['medical'], keywords: ['история болезни', 'номер карты', 'медицинская карта', 'иб №'], valueTest: NUMBER_RE.test.bind(NUMBER_RE) },
  { field_id: 'medical.admission_date', title: 'Дата начала', kind: 'date', domains: ['medical'], keywords: ['дата поступления', 'дата госпитализации', 'поступил', 'поступила'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'medical.discharge_date', title: 'Дата завершения', kind: 'date', domains: ['medical'], keywords: ['дата выписки', 'выписан', 'выписана'], valueTest: DATE_RE.test.bind(DATE_RE) },
  { field_id: 'medical.diagnosis', title: 'Заключение', kind: 'icd10', domains: ['medical'], keywords: ['диагноз', 'заключение', 'мкб', 'icd'], valueTest: ICD_RE.test.bind(ICD_RE) },
  { field_id: 'medical.treatment', title: 'Назначения / действия', kind: 'long_text', domains: ['medical'], keywords: ['лечение', 'назначенное лечение', 'назначения', 'терапия'] },
  { field_id: 'medical.complaints', title: 'Исходные сведения', kind: 'long_text', domains: ['medical'], keywords: ['жалобы'] },
  { field_id: 'medical.status', title: 'Текущее состояние', kind: 'long_text', domains: ['medical'], keywords: ['статус', 'осмотр', 'объективно', 'состояние'] },
  { field_id: 'medical.recommendations', title: 'Рекомендации', kind: 'long_text', domains: ['medical'], keywords: ['рекомендации', 'рекомендовано'] },
  { field_id: 'vehicle.vin', title: 'VIN автомобиля', kind: 'vin', domains: ['legal'], keywords: ['vin', 'идентификационный номер'], valueTest: VIN_RE.test.bind(VIN_RE) },
];

export function suggestScannerFields(input: SuggestionInput): ScannerFieldSuggestion[] {
  const value = clean(input.selectedText);
  const context = clean(input.contextText ?? '');
  const haystack = normalizeForMatching(`${context} ${value}`);
  const haystackTokens = tokenize(haystack);
  const localHaystackTokens = tokenize(localContextAroundSelection(context, value));
  const documents = input.documents ?? [];
  const learnedRules = input.learnedRules ?? [];
  const active = documents.find((document) => document.id === input.activeDocumentId);
  const domain = domainKey(input.domainHint ?? active?.category ?? null);
  const usedFields = new Map<string, string[]>();
  for (const document of documents) {
    const fields = new Set([
      ...(document.placeholders ?? []),
      ...(document.required_fields ?? []),
      ...((document.popup_fields ?? []).map((field) => field.field_id)),
    ]);
    for (const field of fields) {
      const labels = usedFields.get(field) ?? [];
      labels.push(document.button_label);
      usedFields.set(field, Array.from(new Set(labels)));
    }
  }

  const knownOrder = new Map(FIELDS.map((definition, index) => [definition.field_id, index]));
  const scored: Array<ScannerFieldSuggestion & { order: number }> = FIELDS.map((definition, index) => {
    let score = 0.08;
    const reasons: string[] = [];
    const nearbyKeyword = definition.keywords.find((keyword) => containsTokenSequence(localHaystackTokens, tokenize(keyword)));
    const distantKeyword = nearbyKeyword ? undefined : definition.keywords.find((keyword) => containsTokenSequence(haystackTokens, tokenize(keyword)));
    if (nearbyKeyword) {
      score += 0.56;
      reasons.push(`рядом найдено «${nearbyKeyword}»`);
    } else if (distantKeyword) {
      score += 0.18;
      reasons.push(`в тексте встречается «${distantKeyword}»`);
    }
    if (definition.valueTest?.(value)) {
      score += 0.28;
      reasons.push('формат значения подходит');
    }
    const learnedEvidence = learnedEvidenceForField(learnedRules, definition.field_id, haystack, haystackTokens);
    if (learnedEvidence > 0) {
      score += learnedEvidence;
      reasons.push('специалист раньше обучил программу этому полю');
    }
    const destinations = usedFields.get(definition.field_id) ?? [];
    if (destinations.length) {
      score += 0.12;
      reasons.push('поле уже используется в шаблонах');
    }
    if (active && fieldIdsForDocument(active).has(definition.field_id)) {
      score += 0.18;
      reasons.push(`поле есть в «${active.button_label}»`);
    }
    if (domain && definition.domains.includes(domain)) {
      score += 0.08;
      reasons.push('подходит выбранной профессии');
    }
    return {
      field_id: definition.field_id,
      title: definition.title,
      confidence: Math.min(0.99, score),
      reason: reasons.join('; ') || 'универсальный вариант',
      input_kind: definition.kind,
      destinations,
      existing: destinations.length > 0,
      order: index,
    };
  }).filter((item) => item.confidence >= 0.18);

  for (const rule of learnedRules) {
    if (knownOrder.has(rule.field_id)) continue;
    const evidence = learnedEvidenceForRule(rule, haystack, haystackTokens);
    if (evidence <= 0) continue;
    const destinations = usedFields.get(rule.field_id) ?? [];
    const activeMatch = Boolean(active && fieldIdsForDocument(active).has(rule.field_id));
    let confidence = 0.28 + evidence;
    const reasons = ['специалист раньше обучил программу этому полю'];
    if (destinations.length) {
      confidence += 0.12;
      reasons.push('поле уже используется в шаблонах');
    }
    if (activeMatch) {
      confidence += 0.18;
      reasons.push(`поле есть в «${active?.button_label ?? ''}»`);
    }
    scored.push({
      field_id: rule.field_id,
      title: rule.title || humanizeFieldId(rule.field_id),
      confidence: Math.min(0.99, confidence),
      reason: reasons.join('; '),
      input_kind: rule.input_kind,
      destinations,
      existing: destinations.length > 0,
      order: FIELDS.length + learnedRules.indexOf(rule),
    });
  }

  const custom = customSuggestion(value, context, documents);
  if (custom) scored.push({ ...custom, order: Number.MAX_SAFE_INTEGER });

  return scored
    .sort((left, right) =>
      right.confidence - left.confidence
      || Number(right.existing) - Number(left.existing)
      || left.order - right.order
      || left.field_id.localeCompare(right.field_id, 'en'))
    .filter((item, index, all) => all.findIndex((candidate) => candidate.field_id === item.field_id) === index)
    .slice(0, 8)
    .map(({ order: _order, ...item }) => item);
}

export function bestScannerSuggestion(suggestions: ScannerFieldSuggestion[]): ScannerFieldSuggestion | null {
  if (!suggestions.length) return null;
  const first = suggestions[0];
  const second = suggestions[1];
  if (first.confidence >= 0.72) return first;
  if (second && first.confidence - second.confidence >= 0.16) return first;
  return null;
}

function learnedEvidenceForField(
  rules: LearnedScannerRule[],
  fieldId: string,
  haystack: string,
  haystackTokens: string[],
): number {
  return rules
    .filter((rule) => rule.field_id === fieldId)
    .reduce((best, rule) => Math.max(best, learnedEvidenceForRule(rule, haystack, haystackTokens)), 0);
}

function learnedEvidenceForRule(rule: LearnedScannerRule, haystack: string, haystackTokens: string[]): number {
  const labelTokens = tokenize(rule.label_hint);
  if (labelTokens.length && containsTokenSequence(haystackTokens, labelTokens)) return 0.42;

  const anchors = [rule.before_text, rule.after_text]
    .map((anchor) => normalizeForMatching(anchor))
    .filter((anchor) => anchor.length >= 5)
    .map((anchor) => anchor.slice(0, 80));
  if (anchors.some((anchor) => haystack.includes(anchor))) return 0.32;
  return 0;
}

function localContextAroundSelection(context: string, value: string): string {
  const normalizedContext = normalizeForMatching(context);
  const normalizedValue = normalizeForMatching(value);
  const position = normalizedValue ? normalizedContext.indexOf(normalizedValue) : -1;
  if (position < 0) return `${normalizedContext.slice(-80)} ${normalizedValue}`;
  const start = Math.max(0, position - 32);
  const end = Math.min(normalizedContext.length, position + normalizedValue.length + 30);
  return normalizedContext.slice(start, end);
}

function normalizeForMatching(value: string): string {
  return clean(value).toLocaleLowerCase('ru-RU').replace(/ё/g, 'е');
}

function tokenize(value: string): string[] {
  return normalizeForMatching(value).match(/[\p{L}\p{N}]+|№/gu) ?? [];
}

function containsTokenSequence(haystack: string[], needle: string[]): boolean {
  if (!needle.length || needle.length > haystack.length) return false;
  outer: for (let start = 0; start <= haystack.length - needle.length; start += 1) {
    for (let offset = 0; offset < needle.length; offset += 1) {
      if (haystack[start + offset] !== needle[offset]) continue outer;
    }
    return true;
  }
  return false;
}

function humanizeFieldId(fieldId: string): string {
  return fieldId
    .split(/[._-]+/)
    .filter(Boolean)
    .map(capitalize)
    .join(' ') || 'Пользовательское поле';
}

function customSuggestion(value: string, context: string, documents: DocumentTemplateSpec[]): ScannerFieldSuggestion | null {
  const label = inferNearestLabel(context, value);
  if (!label) return null;
  const slug = transliterate(label)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 52);
  if (!slug) return null;
  const fieldId = `custom.${slug}`;
  const destinations = documents.filter((document) => fieldIdsForDocument(document).has(fieldId)).map((document) => document.button_label);
  return {
    field_id: fieldId,
    title: capitalize(label),
    confidence: 0.63,
    reason: `название взято из подписи рядом со значением: «${label}»`,
    input_kind: DATE_RE.test(value) ? 'date' : MONEY_RE.test(value) ? 'money' : value.length > 100 ? 'long_text' : 'text',
    destinations,
    existing: destinations.length > 0,
  };
}

function inferNearestLabel(context: string, value: string): string {
  const normalized = context.replace(/[\r\n\t]+/g, ' ').replace(/\s+/g, ' ').trim();
  const position = normalized.indexOf(value);
  const left = (position >= 0 ? normalized.slice(0, position) : normalized).slice(-100);
  const candidate = left.split(/[|;.!?]/).at(-1)?.replace(/[№#:\-–—=\s]+$/g, '').trim() ?? '';
  if (candidate.length < 3 || candidate.length > 70 || /^\d+$/.test(candidate)) return '';
  return candidate;
}

function fieldIdsForDocument(document: DocumentTemplateSpec): Set<string> {
  return new Set([
    ...(document.placeholders ?? []),
    ...(document.required_fields ?? []),
    ...((document.popup_fields ?? []).map((field) => field.field_id)),
  ]);
}

function domainKey(domain: DomainKind | null): string {
  if (!domain) return '';
  if (typeof domain === 'string') return domain.toLowerCase();
  return 'custom';
}

function clean(value: string): string {
  return String(value ?? '').replace(/[\u0007\r\n]+/g, ' ').replace(/\s+/g, ' ').trim();
}

function capitalize(value: string): string {
  return value ? value[0].toLocaleUpperCase('ru-RU') + value.slice(1) : value;
}

function transliterate(value: string): string {
  const table: Record<string, string> = {
    а: 'a', б: 'b', в: 'v', г: 'g', д: 'd', е: 'e', ё: 'e', ж: 'zh', з: 'z', и: 'i', й: 'y', к: 'k', л: 'l', м: 'm', н: 'n', о: 'o', п: 'p', р: 'r', с: 's', т: 't', у: 'u', ф: 'f', х: 'h', ц: 'c', ч: 'ch', ш: 'sh', щ: 'sch', ъ: '', ы: 'y', ь: '', э: 'e', ю: 'yu', я: 'ya',
  };
  return value.split('').map((character) => {
    const lower = character.toLocaleLowerCase('ru-RU');
    const mapped = table[lower];
    if (mapped === undefined) return character;
    return character === lower ? mapped : mapped.toUpperCase();
  }).join('');
}
