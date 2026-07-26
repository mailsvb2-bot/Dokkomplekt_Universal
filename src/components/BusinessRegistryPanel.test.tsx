import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { BusinessRegistryPanel } from './BusinessRegistryPanel';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';

describe('BusinessRegistryPanel', () => {
  afterEach(() => __resetInvokeForTests());

  it('imports, finds, confirms and exports a counterparty through Rust commands', async () => {
    const calls: Array<{ command: string; payload?: Record<string, unknown> }> = [];
    const statuses: string[] = [];
    let changedValues: Record<string, unknown> | null = null;
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      if (command === 'import_business_registry') return { total_records: 1, imported_records: 1, replaced: false } as never;
      if (command === 'lookup_business_registry') return { inn: '7736050003', name: 'ООО Ромашка', kpp: '773601001', ogrn: '1027700000000', legal_address: 'Москва', source: 'authorized-export', source_updated_at: '2026-07-21' } as never;
      if (command === 'apply_business_registry_record') return { values: { 'counterparty.inn': { field_id: 'counterparty.inn', value: '7736050003', source: 'registry', confidence: 1 } } } as never;
      if (command === 'export_one_c_counterparties') return 'C:/out/Контрагенты_1С.json' as never;
      throw new Error(`unexpected command ${command}`);
    });

    const { container } = render(
      <BusinessRegistryPanel
        outputRoot="C:/out"
        onStatus={(message) => statuses.push(message)}
        onCaseChanged={(semanticCase) => { changedValues = semanticCase.values; }}
      />,
    );

    const fileInput = container.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['[]'], 'registry.json', { type: 'application/json' });
    Object.defineProperty(file, 'text', { value: async () => JSON.stringify([{ inn: '7736050003', name: 'ООО Ромашка', source: 'authorized-export' }]) });
    fireEvent.change(fileInput, { target: { files: [file] } });
    await waitFor(() => expect(calls.some((call) => call.command === 'import_business_registry')).toBe(true));

    fireEvent.change(screen.getByPlaceholderText('ИНН контрагента'), { target: { value: '7736050003' } });
    fireEvent.click(screen.getByRole('button', { name: 'Найти' }));
    await screen.findByText('ООО Ромашка');
    fireEvent.click(screen.getByRole('button', { name: 'Подтвердить и подставить' }));
    await waitFor(() => expect(changedValues).not.toBeNull());
    fireEvent.click(screen.getByRole('button', { name: 'Экспорт JSON для 1С' }));
    await waitFor(() => expect(calls.some((call) => call.command === 'export_one_c_counterparties')).toBe(true));

    expect(calls.map((call) => call.command)).toEqual([
      'import_business_registry',
      'lookup_business_registry',
      'apply_business_registry_record',
      'export_one_c_counterparties',
    ]);
    expect(statuses.join(' ')).toContain('Проверенный справочник обновлён');
    expect(statuses.join(' ')).toContain('Реквизиты подтверждены');
  });
});
