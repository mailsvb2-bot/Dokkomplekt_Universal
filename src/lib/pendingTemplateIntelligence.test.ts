import { describe, expect, it } from 'vitest';
import { hasPublishableLearningProof, publicationEligibleLearningFields, sourceEvidencedLearningFields } from './pendingTemplateIntelligence';

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

  it('blocks high-confidence source evidence when holdout validation failed', () => {
    const fields = publicationEligibleLearningFields({
      fields: [{ field_id: 'document.number', confidence: 0.99, source_matches: ['A-17'] }],
      validation: { verdict: 'failed', publishable: false, passed: false }, validation_id: 'proof-failed',
    });
    expect(fields).toEqual([]);
  });

  it('allows source-evidenced fields only after a passed holdout verdict', () => {
    const fields = publicationEligibleLearningFields({
      fields: [
        { field_id: 'document.number', confidence: 0.42, source_matches: ['A-17'] },
        { field_id: 'amount.total', confidence: 0.99, source_matches: [] },
      ],
      validation: { verdict: 'passed', publishable: true, passed: true }, validation_id: 'proof-passed',
    });
    expect(fields.map((field) => field.field_id)).toEqual(['document.number']);
  });

  it('blocks a passed-looking flag when the canonical verdict is not publishable', () => {
    const fields = publicationEligibleLearningFields({
      fields: [{ field_id: 'document.number', confidence: 0.99, source_matches: ['A-17'] }],
      validation: { verdict: 'failed', publishable: false, passed: true }, validation_id: 'proof-fake',
    });
    expect(fields).toEqual([]);
  });

  it('requires a backend-issued validation id even when verdict flags look passed', () => {
    const report = {
      fields: [{ field_id: 'document.number', confidence: 0.99, source_matches: ['A-17'] }],
      validation: { verdict: 'passed', publishable: true, passed: true },
      validation_id: null,
    };
    expect(hasPublishableLearningProof(report)).toBe(false);
    expect(publicationEligibleLearningFields(report)).toEqual([]);
  });

});
