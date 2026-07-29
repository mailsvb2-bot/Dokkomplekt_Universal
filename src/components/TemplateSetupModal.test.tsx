import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TemplateSetupModal } from './TemplateSetupModal';

const base = {
  templateText: '',
  buttonLabel: '',
  previewTitle: 'Документ',
  pendingTemplates: [],
  draftPopupFields: [],
  onTemplateTextChange: vi.fn(),
  onButtonLabelChange: vi.fn(),
  onDraftPopupFieldsChange: vi.fn(),
  onPendingTemplateLabelChange: vi.fn(),
  onPendingPopupFieldsChange: vi.fn(),
  onMarkupPendingTemplate: vi.fn(async () => undefined),
  onStartGuidedPendingScanner: vi.fn(),
  onAnalyze: vi.fn(),
  onPickFile: vi.fn(),
  onDropFiles: vi.fn(),
  onCancel: vi.fn(),
  onConfirm: vi.fn(),
};

describe('TemplateSetupModal', () => {
  it('keeps the first step simple and disables confirmation without input', () => {
    render(<TemplateSetupModal {...base} />);
    expect(screen.getByText('1. Выберите шаблоны')).toBeTruthy();
    expect(screen.getByText(/Шаблон задаёт форму и расположение полей/)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('creates every prepared template as a button', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Акт.docx',
      button_label: 'Акт',
      extracted_text: 'Акт',
      popup_fields: [],
    }]} />);
    expect(screen.getByText('2. Проверьте названия кнопок')).toBeTruthy();
    expect(screen.getByText('3. Создайте кнопки')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (1)' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
