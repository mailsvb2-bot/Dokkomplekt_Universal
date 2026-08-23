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
  it('offers safe legacy inference as an explicit optional choice', () => {
    const onAutoInferStaticTemplatesChange = vi.fn();
    render(<TemplateSetupModal {...base} onAutoInferStaticTemplatesChange={onAutoInferStaticTemplatesChange} />);
    const checkbox = screen.getByRole('checkbox', { name: /Безопасно попробовать авторазметку старых шаблонов/ });
    expect((checkbox as HTMLInputElement).checked).toBe(false);
    fireEvent.click(checkbox);
    expect(onAutoInferStaticTemplatesChange).toHaveBeenCalledWith(true);
  });

  it('shows a high-confidence workspace profile without asking for profession selection', () => {
    render(<TemplateSetupModal {...base} workspaceInference={{
      suggested_domain: 'Medical',
      confidence: 0.93,
      level: 'high',
      auto_apply: true,
      mixed_domains: false,
      domain_scores: { medical: 18, legal: 1, hr: 1, accounting: 0, education: 0 },
      evidence: [{ document_id: 'd1', title: 'Выписной эпикриз', role_id: 'discharge', attributed_domain: 'Medical', score: 8, field_ids: ['medical.diagnosis'] }],
      reasons: [],
    }} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Выписной эпикриз.docx',
      button_label: 'Выписной эпикриз',
      extracted_text: 'Диагноз Лечение МКБ-10',
      popup_fields: [],
      domain_override: null,
    }]} />);

    expect(screen.getByTestId('workspace-inference-high').textContent).toContain('Программа поняла рабочий профиль: медицина');
    expect(screen.getByTestId('workspace-inference-high').textContent).toContain('93%');
    const manualProfile = screen.getByLabelText('Профиль для Выписной эпикриз.docx');
    const advanced = manualProfile.closest('details');
    expect(advanced).toBeTruthy();
    expect((advanced as HTMLDetailsElement).open).toBe(false);
  });

  it('offers one-click confirmation when workspace inference is only medium confidence', () => {
    const onApplyWorkspaceDomain = vi.fn();
    render(<TemplateSetupModal {...base} onApplyWorkspaceDomain={onApplyWorkspaceDomain} workspaceInference={{
      suggested_domain: 'Legal',
      confidence: 0.64,
      level: 'medium',
      auto_apply: false,
      mixed_domains: false,
      domain_scores: { medical: 0, legal: 7, hr: 2, accounting: 0, education: 0 },
      evidence: [],
      reasons: [],
    }} pendingTemplates={[{
      document_id: 'd1', file_name: 'Договор.docx', button_label: 'Договор', extracted_text: 'Договор', popup_fields: [], domain_override: null,
    }]} />);

    expect(screen.getByTestId('workspace-inference-medium').textContent).toContain('Похоже, рабочий профиль: юридическая работа');
    fireEvent.click(screen.getByRole('button', { name: 'Да, применить ко всем кнопкам' }));
    expect(onApplyWorkspaceDomain).toHaveBeenCalledWith('Legal');
  });

  it('keeps ambiguous workspaces usable without forcing a profession choice', () => {
    render(<TemplateSetupModal {...base} workspaceInference={{
      suggested_domain: null,
      confidence: 0.22,
      level: 'low',
      auto_apply: false,
      mixed_domains: false,
      domain_scores: { medical: 0, legal: 1, hr: 0, accounting: 0, education: 0 },
      evidence: [],
      reasons: [],
    }} pendingTemplates={[{
      document_id: 'd1', file_name: 'Акт.docx', button_label: 'Акт', extracted_text: 'Акт', popup_fields: [], domain_override: null,
    }]} />);

    expect(screen.getByTestId('workspace-inference-low').textContent).toContain('Профессию выбирать не нужно');
    expect((screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('allows correcting one template to any built-in profile', () => {
    const onPendingTemplateDomainChange = vi.fn();
    render(<TemplateSetupModal
      {...base}
      onPendingTemplateDomainChange={onPendingTemplateDomainChange}
      pendingTemplates={[{
        document_id: 'd1',
        file_name: 'Договор.docx',
        button_label: 'Договор',
        extracted_text: 'Договор',
        popup_fields: [],
        domain_override: null,
      }]}
    />);

    fireEvent.change(screen.getByLabelText('Профиль для Договор.docx'), {
      target: { value: 'Legal' },
    });

    expect(onPendingTemplateDomainChange).toHaveBeenCalledWith('d1', 'Legal');
  });

});
