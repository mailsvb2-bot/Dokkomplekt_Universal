import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GuidedScannerModal } from './GuidedScannerModal';

const session = {
  session_id: 'scan-1',
  mode: 'source' as const,
  original_path: 'source.docx',
  opened_path: 'source.docx',
  working_copy: false,
  word_was_running: false,
  automation_available: true,
  message: 'opened',
};

const capture = {
  session_id: 'scan-1',
  selected_text: 'г. Москва, ул. Ленина, д. 5',
  context_text: 'Адрес: г. Москва, ул. Ленина, д. 5',
  before_text: 'Адрес: ',
  after_text: '',
  selection_start: 7,
  selection_end: 34,
  expanded_from_cursor: false,
  document_path: 'source.docx',
  document_closed: false,
};

const suggestions = [
  { field_id: 'subject.address', title: 'Адрес', confidence: 0.68, reason: 'рядом найдено «адрес»', input_kind: 'long_text' as const, destinations: [], existing: false },
  { field_id: 'custom.delivery_address', title: 'Адрес доставки', confidence: 0.63, reason: 'вариант из подписи', input_kind: 'long_text' as const, destinations: [], existing: false },
];

describe('GuidedScannerModal uncertain state', () => {
  it('does not silently fall back to the first suggestion', () => {
    const onSelectedFieldIdChange = vi.fn();
    render(
      <GuidedScannerModal
        mode="source"
        session={session}
        capture={capture}
        suggestions={suggestions}
        selectedFieldId=""
        rememberRule={false}
        addQuestion={false}
        markupAction="replace"
        busy={false}
        onCapture={vi.fn()}
        onReturnToWord={vi.fn()}
        onRetry={vi.fn()}
        onSelectedFieldIdChange={onSelectedFieldIdChange}
        onRememberRuleChange={vi.fn()}
        onAddQuestionChange={vi.fn()}
        onMarkupActionChange={vi.fn()}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(/не смогла уверенно понять значение/i)).toBeTruthy();
    expect((screen.getByRole('button', { name: /Да, всё правильно/i }) as HTMLButtonElement).disabled).toBe(true);
    const alternatives = screen.getByText('Нет, это другое значение').closest('details');
    expect(alternatives?.open).toBe(true);

    const firstRadio = document.querySelector('input[name="scanner-field"]') as HTMLInputElement;
    fireEvent.click(firstRadio);
    expect(onSelectedFieldIdChange).toHaveBeenCalledWith('subject.address');
  });
});
