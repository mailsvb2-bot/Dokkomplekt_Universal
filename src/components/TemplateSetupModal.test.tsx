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

  it('does not create a manual text template until a field is marked', () => {
    const { rerender } = render(<TemplateSetupModal {...base} templateText="Пример с Ивановым Иваном" />);
    expect(screen.getByText('Нужно указать места заполнения')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);

    rerender(<TemplateSetupModal {...base} templateText="Документ № {{document.number}}" />);
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('creates every prepared template as a button', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Акт.docx',
      button_label: 'Акт',
      extracted_text: 'Акт № {{document.number}}',
      popup_fields: [],
    }]} />);
    expect(screen.getByText('2. Проверьте названия кнопок')).toBeTruthy();
    expect(screen.getByText('3. Всё готово')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (1)' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('blocks an unmarked example before its text can be copied as new-document data', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Пример.docx',
      button_label: 'Пример',
      extracted_text: 'Пример документа с Ивановым Иваном Ивановичем',
      popup_fields: [],
    }]} />);
    expect(screen.getByText('3. Нужна разметка')).toBeTruthy();
    expect(screen.getByText(/Текст примера не будет скопирован/)).toBeTruthy();
    const confirm = screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    fireEvent.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
