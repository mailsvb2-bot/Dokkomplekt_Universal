import { describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec } from './types';
import { groupDocumentsByDomain } from './documentGrouping';

function doc(id: string, category: DocumentTemplateSpec['category']): DocumentTemplateSpec {
  return { id, button_label: id, template_path: `${id}.docx`, category, role_id: 'unknown', required_fields: [], placeholders: [], is_static_copy: false };
}

describe('groupDocumentsByDomain', () => {
  it('keeps mixed professional contours separate without changing document identity', () => {
    const groups = groupDocumentsByDomain([doc('claim', 'Legal'), doc('hire', 'Hr'), doc('invoice', 'Accounting')]);
    expect(groups).toHaveLength(3);
    expect(groups.flatMap((group) => group.documents.map((item) => item.id)).sort()).toEqual(['claim', 'hire', 'invoice']);
  });

  it('does not add visual grouping complexity to one-domain workspaces', () => {
    const groups = groupDocumentsByDomain([doc('primary', 'Medical'), doc('discharge', 'Medical')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].title).toBe('Медицина');
    expect(groups[0].documents).toHaveLength(2);
  });
});
