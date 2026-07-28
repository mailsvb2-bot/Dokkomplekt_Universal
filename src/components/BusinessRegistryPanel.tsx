import { useState, type ChangeEvent } from 'react';
import {
  applyBusinessRegistryRecord,
  exportOneCCounterparties,
  importBusinessRegistry,
  lookupBusinessRegistry,
} from '../lib/api';
import type { BusinessRegistryRecord, SemanticCase } from '../lib/types';
import { plainActionError, useActionRunner } from '../hooks/useActionRunner';

interface BusinessRegistryPanelProps {
  outputRoot: string;
  onStatus(message: string): void;
  onCaseChanged(semanticCase: SemanticCase): void;
}

function normalizeImportedPayload(value: unknown): BusinessRegistryRecord[] {
  if (Array.isArray(value)) return value as BusinessRegistryRecord[];
  if (value && typeof value === 'object' && Array.isArray((value as { records?: unknown }).records)) {
    return (value as { records: BusinessRegistryRecord[] }).records;
  }
  throw new Error('JSON должен быть массивом контрагентов или объектом с полем records.');
}

export function BusinessRegistryPanel(props: BusinessRegistryPanelProps) {
  const [inn, setInn] = useState('');
  const [record, setRecord] = useState<BusinessRegistryRecord | null>(null);
  const [replace, setReplace] = useState(false);
  const [target, setTarget] = useState<'organization' | 'counterparty'>('counterparty');
  const [exportPath, setExportPath] = useState(`${props.outputRoot.replace(/[\\/]$/, '')}/Контрагенты_1С.json`);
  const { busy, run } = useActionRunner(props.onStatus, plainActionError);

  async function importJson(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    const imported = await run('импорт справочника контрагентов', async () => {
      const records = normalizeImportedPayload(JSON.parse(await file.text()));
      return importBusinessRegistry(records, replace);
    });
    if (imported) props.onStatus(
      `Проверенный справочник обновлён: импортировано ${imported.imported_records}, всего ${imported.total_records} записей.`,
    );
  }

  async function lookup() {
    const found = await run('поиск контрагента', () => lookupBusinessRegistry(inn));
    if (found === undefined) return;
    setRecord(found);
    props.onStatus(found ? `Найден контрагент: ${found.name}.` : 'ИНН не найден в локальном проверенном справочнике.');
  }

  async function apply() {
    const semanticCase = await run('применение реквизитов контрагента', () => applyBusinessRegistryRecord(inn, target));
    if (!semanticCase) return;
    props.onCaseChanged(semanticCase);
    props.onStatus(`Реквизиты подтверждены и подставлены в блок ${target === 'organization' ? 'организации' : 'контрагента'}.`);
  }

  async function exportOneC() {
    const path = await run('экспорт контрагентов для 1С', () => exportOneCCounterparties(exportPath, inn.trim() ? [inn] : []));
    if (path) props.onStatus(`Обменный JSON для 1С создан: ${path}`);
  }

  return (
    <div className="utilityCard businessRegistryCard">
      <strong>Контрагенты · ИНН / ЕГРЮЛ / 1С</strong>
      <small>
        Импортируйте выгрузку из разрешённого источника ЕГРЮЛ/ЕГРИП или учётной системы. Реквизиты хранятся
        локально в зашифрованной базе и подставляются только после явного подтверждения.
      </small>
      <label className="fileBtn softBtn">
        <i className="ti ti-database-import" aria-hidden="true" /> Импортировать JSON
        <input type="file" accept=".json,application/json" onChange={importJson} disabled={busy} style={{ display: 'none' }} />
      </label>
      <label>
        <input type="checkbox" checked={replace} onChange={(event) => setReplace(event.target.checked)} />{' '}
        заменить справочник целиком
      </label>
      <div className="ztRow">
        <input value={inn} onChange={(event) => setInn(event.target.value)} placeholder="ИНН контрагента" inputMode="numeric" />
        <button className="utilBtn" onClick={lookup} disabled={busy || !inn.trim()}>Найти</button>
      </div>
      {record && (
        <div className="registryResult" role="status">
          <b>{record.name}</b>
          <span>ИНН {record.inn}{record.kpp ? ` · КПП ${record.kpp}` : ''}{record.ogrn ? ` · ОГРН ${record.ogrn}` : ''}</span>
          {record.legal_address && <span>{record.legal_address}</span>}
          {record.source && <small>Источник: {record.source}{record.source_updated_at ? ` · ${record.source_updated_at}` : ''}</small>}
        </div>
      )}
      <select value={target} onChange={(event) => setTarget(event.target.value as 'organization' | 'counterparty')}>
        <option value="counterparty">Подставить как контрагента</option>
        <option value="organization">Подставить как свою организацию</option>
      </select>
      <button className="utilBtn" onClick={apply} disabled={busy || !record}>Подтвердить и подставить</button>
      <input value={exportPath} onChange={(event) => setExportPath(event.target.value)} placeholder="путь JSON для обмена с 1С" />
      <button className="utilBtn" onClick={exportOneC} disabled={busy}>Экспорт JSON для 1С</button>
    </div>
  );
}
