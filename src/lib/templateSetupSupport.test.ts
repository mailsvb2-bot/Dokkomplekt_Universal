import { describe, expect, it } from 'vitest';
import { buildTemplateConfirmationRows, templateSetupCompletionMessage } from './templateSetupSupport';
import type { TemplateConfirmationRowDto } from './types';

describe('buildTemplateConfirmationRows workspace inference', () => {
  it('preserves the Rust auto-inferred workspace domain when the user did not override it', () => {
    const rows: TemplateConfirmationRowDto[] = [{
      document_id: 'doc',
      template_path: 'doc.docx',
      detected_title: 'Выписной эпикриз',
      suggested_button_label: 'Выписной эпикриз',
      editable_button_label: 'Выписной эпикриз',
      role_id: 'discharge',
      is_static_copy: false,
      analysis: {},
      popup_fields: [],
      domain_override: 'Medical',
      workspace_inference: {
        suggested_domain: 'Medical', confidence: 0.9, level: 'high', auto_apply: true,
        mixed_domains: false, domain_scores: { medical: 10 }, evidence: [], reasons: [],
      },
    }];

    const result = buildTemplateConfirmationRows(rows, [{
      document_id: 'doc', template_path: 'doc.docx', extracted_text: 'text', file_name: 'doc.docx',
      button_label: 'Выписной эпикриз', popup_fields: [], domain_override: null,
    }], '', [], null);

    expect(result[0].domain_override).toBe('Medical');
  });

  it('keeps an explicit specialist correction above automatic inference', () => {
    const rows: TemplateConfirmationRowDto[] = [{
      document_id: 'doc', template_path: 'doc.docx', detected_title: 'Report', suggested_button_label: 'Report',
      editable_button_label: 'Report', role_id: 'unknown', is_static_copy: false, analysis: {}, popup_fields: [],
      domain_override: 'Medical',
    }];
    const result = buildTemplateConfirmationRows(rows, [{
      document_id: 'doc', template_path: 'doc.docx', extracted_text: 'text', file_name: 'doc.docx',
      button_label: 'Report', popup_fields: [], domain_override: { Custom: 'архитектор' },
    }], '', [], null);

    expect(result[0].domain_override).toEqual({ Custom: 'архитектор' });
  });
});


describe('templateSetupCompletionMessage', () => {
  it('keeps the existing success message when every selected template was created', () => {
    expect(templateSetupCompletionMessage(2, 2)).toBe(
      'Кнопки созданы: 2. Теперь добавьте исходный документ.',
    );
  });

  it('reports mixed batches instead of claiming every button was created', () => {
    expect(templateSetupCompletionMessage(3, 2)).toContain(
      'Повторяющихся шаблонов пропущено: 1',
    );
  });

  it('reports an all-duplicate batch as a no-op', () => {
    expect(templateSetupCompletionMessage(2, 0)).toBe(
      'Новых кнопок не создано. Повторяющихся шаблонов пропущено: 2.',
    );
  });
});
