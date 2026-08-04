import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
  onRemovePendingTemplate: vi.fn(),
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
    expect(screen.getByText('Не найдены места заполнения')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);

    rerender(<TemplateSetupModal {...base} templateText="Документ № {{document.number}}" />);
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('cleans a dangling number mark from the suggested button label', async () => {
    const onPendingTemplateLabelChange = vi.fn();
    render(<TemplateSetupModal {...base} onPendingTemplateLabelChange={onPendingTemplateLabelChange} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Счёт на оплату.docx',
      button_label: 'Счёт на оплату №',
      extracted_text: 'Счёт на оплату № {{document.number}}',
      popup_fields: [],
    }]} />);
    await waitFor(() => expect(onPendingTemplateLabelChange).toHaveBeenCalledWith('d1', 'Счёт на оплату'));
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

  it('creates a button from an ordinary unmarked Word template without an extra consent trap', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Пример.docx',
      button_label: 'Пример',
      extracted_text: 'Обычный пользовательский шаблон без технических placeholder-ов',
      popup_fields: [],
    }]} />);
    expect(screen.getByText('3. Всё готово')).toBeTruthy();
    expect(screen.getByText(/будет добавлен как рабочая кнопка/)).toBeTruthy();
    const confirm = screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(false);
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('requires unique non-empty button names and lets the user exclude a file', () => {
    const onRemovePendingTemplate = vi.fn();
    render(<TemplateSetupModal {...base} onRemovePendingTemplate={onRemovePendingTemplate} pendingTemplates={[
      { document_id: 'd1', file_name: 'Акт.docx', button_label: 'Акт', extracted_text: 'Акт', popup_fields: [] },
      { document_id: 'd2', file_name: 'Акт 2.docx', button_label: ' акт ', extracted_text: 'Акт 2', popup_fields: [] },
    ]} />);
    expect(screen.getByText('3. Исправьте названия кнопок')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Создать кнопки (2)' }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Не создавать кнопку для Акт 2.docx' }));
    expect(onRemovePendingTemplate).toHaveBeenCalledWith('d2');
  });
});
