import { useEffect, useState } from 'react';
import { getPrinterInventory, updatePrintPreferences } from '../lib/api';
import type { PrintPreferences, PrinterInventory } from '../lib/types';

const TRAYS: Array<{ value: string; label: string }> = [
  ['', 'По умолчанию'],
  ['0', 'Автовыбор'],
  ['1', 'Верхний лоток'],
  ['2', 'Нижний лоток'],
  ['3', 'Средний лоток'],
  ['4', 'Ручная подача'],
  ['5', 'Конверты'],
  ['6', 'Ручная подача конвертов'],
  ['7', 'Автоподача'],
  ['8', 'Тракторная подача'],
  ['9', 'Малая форма'],
  ['10', 'Большая форма'],
  ['11', 'Большая ёмкость'],
  ['14', 'Кассета'],
  ['15', 'Форма'],
];

export function PrintPreferencesControl({ busy }: { busy: boolean }) {
  const [inventory, setInventory] = useState<PrinterInventory | null>(null);
  const [working, setWorking] = useState(false);
  const [status, setStatus] = useState('');

  async function refresh() {
    setWorking(true);
    setStatus('');
    try {
      setInventory(await getPrinterInventory());
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setWorking(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function save(preferences: PrintPreferences) {
    setWorking(true);
    setStatus('Сохраняем настройки печати…');
    try {
      const next = await updatePrintPreferences(preferences);
      setInventory(next);
      setStatus('Настройки печати сохранены.');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    } finally {
      setWorking(false);
    }
  }

  if (!inventory && !status) return <small>Определяем доступные принтеры…</small>;

  const preferences = inventory?.preferences ?? {
    printer_name: null,
    duplex_mode: 'simplex',
    tray: null,
  };
  const disabled = busy || working;

  return (
    <details className="resultCopies printerPreferences" data-testid="printer-preferences">
      <summary>Принтер и параметры печати</summary>
      <div className="printCopyList">
        <label className="printCopyRow">
          <span>Принтер</span>
          <select
            aria-label="Принтер"
            value={preferences.printer_name ?? ''}
            disabled={disabled}
            onChange={(event) => void save({ ...preferences, printer_name: event.target.value || null })}
          >
            <option value="">Системный принтер по умолчанию</option>
            {inventory?.printers.map(printer => (
              <option key={printer.name} value={printer.name}>
                {printer.name}{printer.is_default ? ' · по умолчанию' : ''}
              </option>
            ))}
          </select>
        </label>
        <label className="printCopyRow">
          <span>Двусторонняя печать</span>
          <select
            aria-label="Двусторонняя печать"
            value={preferences.duplex_mode}
            disabled={disabled}
            onChange={(event) => void save({ ...preferences, duplex_mode: event.target.value })}
          >
            <option value="simplex">Односторонняя</option>
            <option value="long_edge">По длинной стороне</option>
            <option value="short_edge">По короткой стороне</option>
            <option value="manual">Ручная двусторонняя</option>
          </select>
        </label>
        <label className="printCopyRow">
          <span>Лоток</span>
          <select
            aria-label="Лоток принтера"
            value={preferences.tray == null ? '' : String(preferences.tray)}
            disabled={disabled}
            onChange={(event) => void save({
              ...preferences,
              tray: event.target.value === '' ? null : Number(event.target.value),
            })}
          >
            {TRAYS.map(option => <option key={option.value || 'default'} value={option.value}>{option.label}</option>)}
          </select>
        </label>
      </div>
      <div className="additionalMaterialActions">
        <button type="button" className="textBtn" disabled={disabled} onClick={() => void refresh()}>
          Обновить список принтеров
        </button>
      </div>
      {inventory?.discovery_error ? <small className="skipWarning">Не удалось получить список принтеров: {inventory.discovery_error}</small> : null}
      {status ? <small role="status">{status}</small> : null}
      {inventory?.advanced_options_note ? <small>{inventory.advanced_options_note}</small> : null}
    </details>
  );
}
