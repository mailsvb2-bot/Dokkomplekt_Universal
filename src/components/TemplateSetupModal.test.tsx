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
  onMarkupPendingTemplate: vi.fn(async () => undefined),
  onLearnPendingTemplate: vi.fn(async () => undefined),
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
    expect(screen.getByText('Выберите шаблоны документов')).toBeTruthy();
    expect(screen.getByText(/Каждый DOCX или DOCM сразу станет отдельной кнопкой/)).toBeTruthy();
    expect(screen.getByText('Создать одну кнопку из вставленного текста')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('allows a non-empty manual text template without mandatory markup', () => {
    const { rerender } = render(<TemplateSetupModal {...base} />);
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(true);
    rerender(<TemplateSetupModal {...base} templateText="Пример с Ивановым Иваном" />);
    expect((screen.getByRole('button', { name: 'Создать кнопку' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('cleans a dangling number mark from the suggested button label', async () => {
    const onPendingTemplateLabelChange = vi.fn();
    render(<TemplateSetupModal {...base} onPendingTemplateLabelChange={onPendingTemplateLabelChange} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Счёт на оплату.docx',
      button_label: 'Счёт на оплату №',
      extracted_text: 'Счёт № {{document.number}}',
      popup_fields: [],
    }]} />);
    await waitFor(() => expect(onPendingTemplateLabelChange).toHaveBeenCalledWith('d1', 'Счёт на оплату'));
  });

  it('creates every prepared template as a button', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[
      { document_id: 'd1', file_name: 'Акт.docx', button_label: 'Акт', extracted_text: 'Акт № {{document.number}}', popup_fields: [] },
      { document_id: 'd2', file_name: 'Договор.docx', button_label: 'Договор', extracted_text: 'Договор', popup_fields: [] },
    ]} />);
    expect(screen.getByText('Проверьте названия кнопок')).toBeTruthy();
    expect(screen.getByText('Кнопки готовы к созданию')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (2)' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('allows an unmarked DOCX as an immediately usable static-copy button', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Пример.docx',
      button_label: 'Пример',
      extracted_text: 'Пример документа',
      popup_fields: [],
    }]} />);
    expect(screen.getByText(/Неразмеченные шаблоны сохранят свою форму и будут доступны сразу/)).toBeTruthy();
    const confirm = screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(false);
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
