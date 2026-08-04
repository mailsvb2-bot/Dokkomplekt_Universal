import { useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { AppDialogProvider, useAppDialog } from './AppDialogProvider';

function Harness() {
  const dialogs = useAppDialog();
  const [result, setResult] = useState('');
  return <>
    <button onClick={async () => setResult(await dialogs.confirm({ title: 'Удалить?', message: 'Последствие', confirmLabel: 'Удалить', danger: true }) ? 'yes' : 'no')}>confirm</button>
    <button onClick={async () => setResult(await dialogs.prompt({ title: 'Переименовать', label: 'Название', required: true, confirmLabel: 'Сохранить' }) ?? 'cancel')}>prompt</button>
    <output>{result}</output>
  </>;
}

describe('AppDialogProvider', () => {
  it('returns a confirmed product-native decision', async () => {
    render(<AppDialogProvider><Harness /></AppDialogProvider>);
    fireEvent.click(screen.getByRole('button', { name: 'confirm' }));
    const dialog = screen.getByRole('dialog', { name: 'Удалить?' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Удалить' }));
    expect(await screen.findByText('yes')).toBeTruthy();
  });

  it('blocks a required prompt until text is entered', async () => {
    render(<AppDialogProvider><Harness /></AppDialogProvider>);
    fireEvent.click(screen.getByRole('button', { name: 'prompt' }));
    const dialog = screen.getByRole('dialog', { name: 'Переименовать' });
    const save = within(dialog).getByRole('button', { name: 'Сохранить' });
    expect(save).toHaveProperty('disabled', true);
    fireEvent.change(within(dialog).getByLabelText('Название *'), { target: { value: 'Новый документ' } });
    fireEvent.click(save);
    expect(await screen.findByText('Новый документ')).toBeTruthy();
  });
});
