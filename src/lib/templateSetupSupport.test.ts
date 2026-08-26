import { describe, expect, it } from 'vitest';
import { buildTemplateConfirmationRows, partitionPickedTemplates, templatePickerCompletionMessage, templateSetupCompletionMessage } from './templateSetupSupport';
import type { PopupFieldConfig, TemplateConfirmationRowDto } from './types';

function popupField(fieldId: string): PopupFieldConfig {
  return {
    field_id: fieldId,
    title: fieldId,
    required: false,
    input_kind: 'text',
    ask_mode: 'if_missing',
    options: [],
    allow_custom_option: false,
    order: 0,
  };
}

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
    expect(result[0].domain_override_is_explicit).toBe(false);
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
    expect(result[0].domain_override_is_explicit).toBe(true);
  });

  it('uses popup defaults rebuilt for the inferred workspace domain', () => {
    const rows: TemplateConfirmationRowDto[] = [{
      document_id: 'doc', template_path: 'doc.docx', detected_title: 'Согласие', suggested_button_label: 'Согласие',
      editable_button_label: 'Согласие', role_id: 'consent', is_static_copy: false, analysis: {},
      popup_fields: [popupField('medical.position')], domain_override: 'Medical',
    }];
    const result = buildTemplateConfirmationRows(rows, [{
      document_id: 'doc', template_path: 'doc.docx', extracted_text: '{{Должность}}', file_name: 'doc.docx',
      button_label: 'Согласие', popup_fields: [popupField('employee.position')], popup_fields_edited: false,
      domain_override: null,
    }], '', [], null);

    expect(result[0].popup_fields?.map((field) => field.field_id)).toEqual(['medical.position']);
    expect(result[0].popup_fields_edited).toBe(false);
  });

  it('preserves popup fields that the user actually edited', () => {
    const rows: TemplateConfirmationRowDto[] = [{
      document_id: 'doc', template_path: 'doc.docx', detected_title: 'Согласие', suggested_button_label: 'Согласие',
      editable_button_label: 'Согласие', role_id: 'consent', is_static_copy: false, analysis: {},
      popup_fields: [popupField('medical.position')], domain_override: 'Medical',
    }];
    const result = buildTemplateConfirmationRows(rows, [{
      document_id: 'doc', template_path: 'doc.docx', extracted_text: '{{Должность}}', file_name: 'doc.docx',
      button_label: 'Согласие', popup_fields: [popupField('custom.position')], popup_fields_edited: true,
      domain_override: null,
    }], '', [], null);

    expect(result[0].popup_fields?.map((field) => field.field_id)).toEqual(['custom.position']);
    expect(result[0].popup_fields_edited).toBe(true);
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


describe('template picker partial failures', () => {
  const good = { file_name: 'good.docx', template_path: '/safe/good.docx', extracted_text: 'GOOD' };
  const broken = { file_name: 'bad.docx', template_path: '', extracted_text: '', import_error: 'broken zip' };

  it('keeps valid selected templates while separating rejected ones', () => {
    const result = partitionPickedTemplates([good, broken]);
    expect(result.acceptedTemplates).toEqual([good]);
    expect(result.rejectedTemplates).toEqual([broken]);
    expect(result.rejectedDetails).toBe('bad.docx: broken zip');
  });

  it('reports partial failure without hiding successful imports', () => {
    expect(templatePickerCompletionMessage(1, [broken])).toContain('Шаблоны выбраны: 1.');
    expect(templatePickerCompletionMessage(1, [broken])).toContain('Пропущено проблемных шаблонов: 1');
  });
});
