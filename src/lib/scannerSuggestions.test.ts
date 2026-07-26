import { describe, expect, it } from 'vitest';
import { bestScannerSuggestion, suggestScannerFields, type ScannerFieldSuggestion } from './scannerSuggestions';

const documents = [
  { id: 'contract', button_label: 'Договор', template_path: 'a.docx', category: 'Legal' as const, role_id: 'contract', required_fields: [], placeholders: ['contract.number', 'contract.date'], is_static_copy: false },
  { id: 'act', button_label: 'Акт', template_path: 'b.docx', category: 'Legal' as const, role_id: 'act', required_fields: [], placeholders: ['contract.number'], is_static_copy: false },
];

describe('scanner field suggestions', () => {
  it('suggests a contract number and shows every destination document', () => {
    const suggestions = suggestScannerFields({ selectedText: 'Д-148/26', contextText: 'Договор № Д-148/26 от 12.05.2026', documents, activeDocumentId: 'contract' });
    expect(suggestions[0].field_id).toBe('contract.number');
    expect(suggestions[0].destinations).toEqual(['Договор', 'Акт']);
  });

  it('recognizes typed requisites without requiring a technical field id from the user', () => {
    const suggestions = suggestScannerFields({ selectedText: '7736050003', contextText: 'ИНН 7736050003', documents: [] });
    expect(suggestions[0]).toMatchObject({ field_id: 'org.inn', input_kind: 'inn' });
  });

  it('creates a safe custom field from a nearby unknown label', () => {
    const suggestions = suggestScannerFields({ selectedText: '30 дней', contextText: 'Срок оказания услуг: 30 дней', documents: [] });
    expect(suggestions.some((item) => item.field_id === 'custom.srok_okazaniya_uslug')).toBe(true);
  });

  it('keeps medical suggestions profile-specific', () => {
    const suggestions = suggestScannerFields({ selectedText: 'F32.1 Депрессивный эпизод', contextText: 'Диагноз: F32.1 Депрессивный эпизод', documents: [], domainHint: 'Medical' });
    expect(suggestions[0].field_id).toBe('medical.diagnosis');
  });

  it('matches short keywords only as whole tokens, never inside Russian words', () => {
    const suggestions = suggestScannerFields({
      selectedText: 'г. Москва, ул. Ленина, д. 5',
      contextText: 'Работник ознакомлен с договором. Адрес: г. Москва, ул. Ленина, д. 5',
      documents: [],
      domainHint: 'Hr',
    });
    expect(suggestions[0].field_id).toBe('subject.address');
    expect(suggestions.find((item) => item.field_id === 'document.date')?.reason ?? '').not.toContain('«от»');
    expect(suggestions.find((item) => item.field_id === 'period.end_date')?.reason ?? '').not.toContain('«до»');
  });

  it('returns null when the top two suggestions are too close and confidence is low', () => {
    const ambiguous: ScannerFieldSuggestion[] = [
      { field_id: 'custom.one', title: 'Первое', confidence: 0.68, reason: '', input_kind: 'text', destinations: [], existing: false },
      { field_id: 'custom.two', title: 'Второе', confidence: 0.61, reason: '', input_kind: 'text', destinations: [], existing: false },
    ];
    expect(bestScannerSuggestion(ambiguous)).toBeNull();
  });

  it('does not auto-confirm a sole low-confidence fallback', () => {
    const weak: ScannerFieldSuggestion[] = [
      { field_id: 'custom.weak', title: 'Слабый вариант', confidence: 0.48, reason: '', input_kind: 'text', destinations: [], existing: false },
    ];
    expect(bestScannerSuggestion(weak)).toBeNull();
  });

  it('returns the recommendation when confidence or separation is sufficient', () => {
    const clear: ScannerFieldSuggestion[] = [
      { field_id: 'org.inn', title: 'ИНН', confidence: 0.76, reason: '', input_kind: 'inn', destinations: [], existing: false },
      { field_id: 'document.number', title: 'Номер', confidence: 0.45, reason: '', input_kind: 'text', destinations: [], existing: false },
    ];
    expect(bestScannerSuggestion(clear)?.field_id).toBe('org.inn');
  });

  it('raises a previously learned custom field when its label appears again', () => {
    const suggestions = suggestScannerFields({
      selectedText: 'АБ-42',
      contextText: 'Код внутреннего проекта: АБ-42',
      documents: [],
      learnedRules: [{
        rule_id: 'rule-1',
        field_id: 'custom.project_code',
        title: 'Код внутреннего проекта',
        label_hint: 'Код внутреннего проекта',
        before_text: 'Код внутреннего проекта: ',
        after_text: '',
        sample_value: 'АБ-10',
        input_kind: 'text',
        created_at: '1',
      }],
    });
    expect(suggestions[0]).toMatchObject({ field_id: 'custom.project_code', title: 'Код внутреннего проекта' });
    expect(suggestions[0].reason).toContain('раньше обучил');
  });
});
