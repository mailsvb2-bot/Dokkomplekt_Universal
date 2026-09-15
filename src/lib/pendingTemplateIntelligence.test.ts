import { describe, expect, it } from 'vitest';
import { sourceEvidencedLearningFields } from './pendingTemplateIntelligence';

describe('template learning evidence boundary', () => {
  it('keeps low-confidence fields when matched source evidence exists', () => {
    const fields = sourceEvidencedLearningFields([
      { field_id: 'document.number', confidence: 0.41, source_matches: ['A-17'] },
      { field_id: 'amount.total', confidence: 0.99, source_matches: [] },
    ]);
    expect(fields.map((field) => field.field_id)).toEqual(['document.number']);
  });

  it('rejects whitespace-only source evidence regardless of confidence', () => {
    const fields = sourceEvidencedLearningFields([
      { field_id: 'document.date', confidence: 0.99, source_matches: ['   '] },
    ]);
    expect(fields).toEqual([]);
  });
});
