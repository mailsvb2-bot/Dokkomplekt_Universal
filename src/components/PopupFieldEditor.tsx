import type { PopupFieldConfig, PromptAskMode, PromptInputKind } from '../lib/types';

const KIND_OPTIONS: Array<[PromptInputKind, string]> = [
  ['text', 'Короткий текст'],
  ['long_text', 'Большой текст'],
  ['date', 'Дата'],
  ['number', 'Число'],
  ['money', 'Денежная сумма'],
  ['inn', 'ИНН'],
  ['kpp', 'КПП'],
  ['ogrn', 'ОГРН / ОГРНИП'],
  ['snils', 'СНИЛС'],
  ['passport', 'Паспорт / документ'],
  ['vin', 'VIN'],
  ['icd10', 'МКБ-10 / классификатор'],
  ['select', 'Выбор из списка'],
  ['yes_no', 'Да / Нет'],
];

const ASK_OPTIONS: Array<[PromptAskMode, string]> = [
  ['if_missing', 'Только если не найдено'],
  ['confirm', 'Показать найденное для проверки'],
  ['always', 'Спрашивать каждый раз'],
];

interface PopupFieldEditorProps {
  fields: PopupFieldConfig[];
  onChange(fields: PopupFieldConfig[]): void;
  compact?: boolean;
}

export function newPopupField(fieldId = ''): PopupFieldConfig {
  return {
    field_id: fieldId,
    title: fieldId ? humanizeFieldId(fieldId) : 'Новый вопрос',
    required: true,
    input_kind: inferInputKind(fieldId),
    ask_mode: 'if_missing',
    options: [],
    allow_custom_option: false,
    help_text: null,
    section: 'Данные документа',
    default_value: null,
    linked_to: null,
    order: 500,
  };
}

export function ensurePopupField(fields: PopupFieldConfig[], fieldId: string): PopupFieldConfig[] {
  const normalized = fieldId.trim();
  if (!normalized || fields.some((field) => field.field_id === normalized)) return fields;
  return [...fields, newPopupField(normalized)];
}

export function PopupFieldEditor({ fields, onChange, compact = false }: PopupFieldEditorProps) {
  function update(index: number, patch: Partial<PopupFieldConfig>) {
    onChange(fields.map((field, current) => current === index ? { ...field, ...patch } : field));
  }

  function remove(index: number) {
    onChange(fields.filter((_field, current) => current !== index));
  }

  return (
    <section className={compact ? 'popupEditor compact' : 'popupEditor'} aria-label="Конструктор уточняющих вопросов">
      <div className="popupEditorHead">
        <div>
          <strong>Уточняющие вопросы</strong>
          <small>Один объединённый popup откроется перед созданием документа или комплекта.</small>
        </div>
        <button className="softBtn" type="button" onClick={() => onChange([...fields, newPopupField()])}>
          + Добавить вопрос
        </button>
      </div>

      {!fields.length ? (
        <div className="popupEmpty">Вопросов пока нет. Добавьте поле вручную или выделите значение сканером курсора.</div>
      ) : (
        <div className="popupFieldList">
          {fields.map((field, index) => (
            <article className="popupFieldCard" key={`${field.field_id || 'new'}:${index}`}>
              <div className="popupFieldTop">
                <span className="popupFieldNumber">{index + 1}</span>
                <input
                  className="popupTitleInput"
                  value={field.title}
                  onChange={(event) => update(index, { title: event.target.value })}
                  placeholder="Текст вопроса для специалиста"
                  aria-label={`Текст вопроса ${index + 1}`}
                />
                <button className="iconBtn danger" type="button" onClick={() => remove(index)} aria-label={`Удалить вопрос ${index + 1}`}>×</button>
              </div>

              <div className="popupFieldGrid">
                <label>
                  <span>Смысловое поле</span>
                  <input
                    value={field.field_id}
                    onChange={(event) => update(index, {
                      field_id: event.target.value,
                      input_kind: field.input_kind === 'text' ? inferInputKind(event.target.value) : field.input_kind,
                    })}
                    placeholder="например contract.number"
                  />
                </label>
                <label>
                  <span>Тип ответа</span>
                  <select value={field.input_kind} onChange={(event) => update(index, { input_kind: event.target.value as PromptInputKind })}>
                    {KIND_OPTIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                  </select>
                </label>
                <label>
                  <span>Когда спрашивать</span>
                  <select value={field.ask_mode} onChange={(event) => update(index, { ask_mode: event.target.value as PromptAskMode })}>
                    {ASK_OPTIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                  </select>
                </label>
                <label>
                  <span>Раздел окна</span>
                  <input value={field.section ?? ''} onChange={(event) => update(index, { section: event.target.value || null })} placeholder="Данные договора" />
                </label>
                <label>
                  <span>Значение по умолчанию</span>
                  <input value={field.default_value ?? ''} onChange={(event) => update(index, { default_value: event.target.value || null })} placeholder="@today или текст" />
                </label>
                <label>
                  <span>Подсказка</span>
                  <input value={field.help_text ?? ''} onChange={(event) => update(index, { help_text: event.target.value || null })} placeholder="Как правильно заполнить" />
                </label>
                <label>
                  <span>Повторять значение поля</span>
                  <input
                    value={field.linked_to ?? ''}
                    onChange={(event) => update(index, { linked_to: event.target.value || null })}
                    placeholder="например medical.commission_date"
                  />
                  <small>Связанное значение копируется, пока специалист не изменит это поле вручную.</small>
                </label>
              </div>

              {field.input_kind === 'select' && (
                <div className="popupSelectOptions">
                  <label>
                    <span>Варианты через точку с запятой</span>
                    <input
                      value={(field.options ?? []).join('; ')}
                      onChange={(event) => update(index, { options: event.target.value.split(';').map((item) => item.trim()).filter(Boolean) })}
                      placeholder="Вариант 1; Вариант 2; Вариант 3"
                    />
                  </label>
                  <label className="inlineCheck">
                    <input type="checkbox" checked={field.allow_custom_option} onChange={(event) => update(index, { allow_custom_option: event.target.checked })} />
                    Разрешить свой вариант
                  </label>
                </div>
              )}

              <div className="popupFieldFlags">
                <label className="inlineCheck">
                  <input type="checkbox" checked={field.required} onChange={(event) => update(index, { required: event.target.checked })} />
                  Обязательное поле
                </label>
                <label>
                  <span>Порядок</span>
                  <input type="number" min={0} max={9999} value={field.order} onChange={(event) => update(index, { order: Number(event.target.value) || 0 })} />
                </label>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function humanizeFieldId(fieldId: string): string {
  const known: Record<string, string> = {
    'document.number': 'Номер документа',
    'document.date': 'Дата документа',
    'subject.name': 'ФИО / наименование субъекта',
    'org.name': 'Организация',
    'counterparty.name': 'Контрагент',
    'contract.number': 'Номер договора',
    'contract.date': 'Дата договора',
    'contract.party_a': 'Сторона 1',
    'contract.party_b': 'Сторона 2',
    'medical.case_number': 'Номер истории болезни',
    'medical.diagnosis': 'Диагноз',
    'medical.treatment': 'Лечение',
    'medical.discharge_date': 'Дата выписки',
    'hr.order_number': 'Номер приказа',
    'hr.order_date': 'Дата приказа',
    'accounting.invoice_number': 'Номер счёта',
    'accounting.invoice_date': 'Дата счёта',
  };
  return known[fieldId] ?? (fieldId.split(/[._-]+/).filter(Boolean).map((part) => part[0]?.toUpperCase() + part.slice(1)).join(' ') || 'Новый вопрос');
}

function inferInputKind(fieldId: string): PromptInputKind {
  const id = fieldId.toLowerCase();
  if (id.includes('date') || id.endsWith('.from') || id.endsWith('.until')) return 'date';
  if (id.includes('diagnosis') || id.includes('icd')) return 'icd10';
  if (id.endsWith('.inn')) return 'inn';
  if (id.endsWith('.kpp')) return 'kpp';
  if (id.endsWith('.ogrn')) return 'ogrn';
  if (id.includes('snils')) return 'snils';
  if (id.includes('vin')) return 'vin';
  if (id.includes('amount') || id.includes('salary') || id.includes('price')) return 'money';
  if (id.includes('count') || id.includes('quantity') || id.endsWith('.days')) return 'number';
  if (/(treatment|conclusion|recommend|description|complaint|status|notes)/.test(id)) return 'long_text';
  return 'text';
}
