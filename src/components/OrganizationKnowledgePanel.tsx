import { useEffect, useMemo, useState } from 'react';
import type { OrganizationKnowledgeCategory, OrganizationKnowledgeRecord, SemanticCase } from '../lib/types';
import {
  applyOrganizationKnowledge,
  deleteOrganizationKnowledge,
  listOrganizationKnowledge,
  upsertOrganizationKnowledge,
} from '../lib/api';

interface Props {
  onStatus(message: string): void;
  onCaseChanged?(semanticCase: SemanticCase): void;
}

const categories: Array<{ id: OrganizationKnowledgeCategory; label: string }> = [
  { id: 'organization', label: 'Реквизиты организации' },
  { id: 'employee', label: 'Сотрудники' },
  { id: 'position', label: 'Должности' },
  { id: 'signatory', label: 'Подписанты' },
  { id: 'department', label: 'Подразделения' },
  { id: 'counter', label: 'Нумераторы' },
  { id: 'print_form', label: 'Печатные формы' },
  { id: 'authority', label: 'Полномочия' },
  { id: 'template_rule', label: 'Правила шаблонов' },
];

export function OrganizationKnowledgePanel({ onStatus, onCaseChanged }: Props) {
  const [records, setRecords] = useState<OrganizationKnowledgeRecord[]>([]);
  const [category, setCategory] = useState<OrganizationKnowledgeCategory>('organization');
  const [recordId, setRecordId] = useState('');
  const [label, setLabel] = useState('');
  const [fieldsText, setFieldsText] = useState('org.name=\norg.inn=');
  const [validFrom, setValidFrom] = useState('');
  const [validUntil, setValidUntil] = useState('');
  const [note, setNote] = useState('');
  const [active, setActive] = useState(true);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listOrganizationKnowledge(undefined, true).then((items) => setRecords(Array.isArray(items) ? items : [])).catch(() => undefined);
  }, []);

  const visible = useMemo(() => records.filter((record) => record.category === category), [category, records]);

  async function execute<T>(name: string, action: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    try {
      return await action();
    } catch (error) {
      onStatus(`Ошибка «${name}»: ${message(error)}`);
      return undefined;
    } finally {
      setBusy(false);
    }
  }

  function parseFields(): Record<string, string> {
    const fields: Record<string, string> = {};
    for (const rawLine of fieldsText.split(/\r?\n/)) {
      const line = rawLine.trim();
      if (!line) continue;
      const separator = line.indexOf('=');
      if (separator <= 0) throw new Error(`строка должна иметь вид field.id=значение: ${line}`);
      const fieldId = line.slice(0, separator).trim();
      const value = line.slice(separator + 1).trim();
      if (!value) throw new Error(`пустое значение: ${fieldId}`);
      fields[fieldId] = value;
    }
    return fields;
  }

  async function save() {
    let fields: Record<string, string>;
    try {
      fields = parseFields();
    } catch (error) {
      onStatus(message(error));
      return;
    }
    const result = await execute('сохранение организационных знаний', () => upsertOrganizationKnowledge({
      record_id: recordId.trim(),
      category,
      label: label.trim(),
      fields,
      valid_from: validFrom || null,
      valid_until: validUntil || null,
      active,
      note: note.trim(),
    }));
    if (result) {
      setRecords(result);
      onStatus(`Запись «${label.trim()}» сохранена локально и не применяется скрытно.`);
    }
  }

  function edit(record: OrganizationKnowledgeRecord) {
    setCategory(record.category);
    setRecordId(record.record_id);
    setLabel(record.label);
    setFieldsText(Object.entries(record.fields).map(([fieldId, value]) => `${fieldId}=${value}`).join('\n'));
    setValidFrom(record.valid_from ?? '');
    setValidUntil(record.valid_until ?? '');
    setActive(record.active);
    setNote(record.note ?? '');
  }

  async function remove(recordIdValue: string) {
    const result = await execute('удаление знания', () => deleteOrganizationKnowledge(recordIdValue));
    if (result) setRecords(result);
  }

  async function apply(record: OrganizationKnowledgeRecord) {
    const result = await execute('применение знания', () => applyOrganizationKnowledge(record.record_id));
    if (result) {
      onCaseChanged?.(result);
      onStatus(`Запись «${record.label}» применена явным действием пользователя.`);
    }
  }

  return (
    <section className="utilityCard organizationKnowledgeCard">
      <strong>Организационные знания</strong>
      <small>Реквизиты, сотрудники, должности, подписанты, подразделения, нумераторы, формы, полномочия и правила хранятся отдельно от шаблонов. Ничего не включается скрытно.</small>
      <label>Категория
        <select value={category} onChange={(event) => setCategory(event.target.value as OrganizationKnowledgeCategory)}>
          {categories.map((item) => <option value={item.id} key={item.id}>{item.label}</option>)}
        </select>
      </label>
      <div className="knowledgeFormGrid">
        <label>ID записи<input value={recordId} onChange={(event) => setRecordId(event.target.value)} placeholder="org.main" /></label>
        <label>Название<input value={label} onChange={(event) => setLabel(event.target.value)} placeholder="Основная организация" /></label>
        <label>Действует с<input type="date" value={validFrom} onChange={(event) => setValidFrom(event.target.value)} /></label>
        <label>Действует до<input type="date" value={validUntil} onChange={(event) => setValidUntil(event.target.value)} /></label>
      </div>
      <label>Смысловые поля, по одному на строку
        <textarea value={fieldsText} onChange={(event) => setFieldsText(event.target.value)} spellCheck={false} placeholder={'org.name=ООО Ромашка\norg.inn=7707083893'} />
      </label>
      <label>Примечание<input value={note} onChange={(event) => setNote(event.target.value)} /></label>
      <label><input type="checkbox" checked={active} onChange={(event) => setActive(event.target.checked)} /> запись активна</label>
      <button type="button" disabled={busy} onClick={() => void save()}>Сохранить запись</button>
      <div className="knowledgeList">
        {visible.length === 0 && <small>В этой категории записей пока нет.</small>}
        {visible.map((record) => (
          <div className="knowledgeItem" key={record.record_id}>
            <div><b>{record.label}</b><small>{record.record_id} · {Object.keys(record.fields).length} полей{record.active ? '' : ' · неактивна'}</small></div>
            <div className="compactActions">
              <button type="button" disabled={busy} onClick={() => void apply(record)}>Применить</button>
              <button type="button" disabled={busy} onClick={() => edit(record)}>Изменить</button>
              <button type="button" disabled={busy} onClick={() => void remove(record.record_id)}>Удалить</button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
