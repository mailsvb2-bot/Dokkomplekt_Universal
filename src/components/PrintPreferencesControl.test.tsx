import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PrintPreferencesControl } from './PrintPreferencesControl';

const getPrinterInventory = vi.fn();
const updatePrintPreferences = vi.fn();

vi.mock('../lib/api', () => ({
  getPrinterInventory: (...args: unknown[]) => getPrinterInventory(...args),
  updatePrintPreferences: (...args: unknown[]) => updatePrintPreferences(...args),
}));

afterEach(() => {
  vi.clearAllMocks();
});

describe('PrintPreferencesControl predecessor parity', () => {
  it('loads printers and persists an explicit printer choice', async () => {
    getPrinterInventory.mockResolvedValue({
      platform: 'windows',
      printers: [
        { name: 'Office Printer', is_default: true, driver: 'Driver', port: 'PORT1' },
        { name: 'Archive Printer', is_default: false, driver: 'Driver', port: 'PORT2' },
      ],
      preferences: { printer_name: null, duplex_mode: 'simplex', tray: null },
      discovery_error: null,
      advanced_options_note: 'Windows printing',
    });
    updatePrintPreferences.mockResolvedValue({
      platform: 'windows',
      printers: [
        { name: 'Office Printer', is_default: true, driver: 'Driver', port: 'PORT1' },
        { name: 'Archive Printer', is_default: false, driver: 'Driver', port: 'PORT2' },
      ],
      preferences: { printer_name: 'Archive Printer', duplex_mode: 'simplex', tray: null },
      discovery_error: null,
      advanced_options_note: 'Windows printing',
    });

    render(<PrintPreferencesControl busy={false} />);
    const printer = await screen.findByLabelText('Принтер');
    expect(screen.getByRole('option', { name: /Office Printer.*по умолчанию/ })).toBeTruthy();

    fireEvent.change(printer, { target: { value: 'Archive Printer' } });
    await waitFor(() => expect(updatePrintPreferences).toHaveBeenCalledWith({
      printer_name: 'Archive Printer',
      duplex_mode: 'simplex',
      tray: null,
    }));
    await waitFor(() => expect((screen.getByLabelText('Принтер') as HTMLSelectElement).value).toBe('Archive Printer'));
  });

  it('keeps system-default printing as an explicit safe option', async () => {
    getPrinterInventory.mockResolvedValue({
      platform: 'windows',
      printers: [],
      preferences: { printer_name: null, duplex_mode: 'simplex', tray: null },
      discovery_error: null,
      advanced_options_note: '',
    });

    render(<PrintPreferencesControl busy={false} />);
    const printer = await screen.findByLabelText('Принтер');
    expect((printer as HTMLSelectElement).value).toBe('');
    expect(screen.getByRole('option', { name: 'Системный принтер по умолчанию' })).toBeTruthy();
  });
});
