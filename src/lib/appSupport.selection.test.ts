import { describe, expect, it } from 'vitest';
import type { BundleDecision, DocumentRoutingRecommendation, DocumentTemplateSpec, DomainKind } from './types';
import { bundleSelectionFromDecision, defaultSelectedDocumentIds, loadOutputFolderParts, loadOutputRoot, OUTPUT_NAMING_CONFIRMED_KEY, OUTPUT_ROOT_KEY, saveOutputFolderParts, saveOutputRoot, shouldSelectDocumentByDefault } from './appSupport';

function document(roleId: string, category: DomainKind, label = 'Переименовано пользователем'): DocumentTemplateSpec {
  return {
    id: `doc-${roleId}`,
    button_label: label,
    template_path: `${roleId}.docx`,
    category,
    role_id: roleId,
    required_fields: [],
    placeholders: [],
    is_static_copy: false,
    popup_fields: [],
    popup_configured: false,
  };
}


describe('output root persistence', () => {
  it('remembers the user-selected generic output folder across restarts', () => {
    localStorage.removeItem(OUTPUT_ROOT_KEY);
    expect(loadOutputRoot()).toBe('');
    saveOutputRoot('  D:/Работа/Готовые документы  ');
    expect(loadOutputRoot()).toBe('D:/Работа/Готовые документы');
  });

  it('explicitly clearing the saved folder removes both the path and its confirmation flag', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
    saveOutputRoot('   ');
    expect(loadOutputRoot()).toBe('');
    expect(localStorage.getItem(OUTPUT_ROOT_KEY)).toBeNull();
    expect(localStorage.getItem(OUTPUT_NAMING_CONFIRMED_KEY)).toBeNull();
  });

  it('persists an intentionally empty output-folder naming rule without inventing defaults', () => {
    saveOutputFolderParts([], true);
    expect(loadOutputFolderParts()).toEqual([]);
  });

  it('migrates the old repository-relative fallback back to an unconfigured state', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'output/Готовые документы');
    expect(loadOutputRoot()).toBe('');
    localStorage.setItem(OUTPUT_ROOT_KEY, 'output\\Готовые документы\\');
    expect(loadOutputRoot()).toBe('');
  });
});

describe('default document selection', () => {
  it('starts with no guessed kit for every profession', () => {
    const documents = [
      document('primary', 'Medical'),
      document('discharge', 'Medical'),
      document('diaries', 'Medical'),
      document('discharge', 'Legal'),
      document('contract', 'Hr'),
      document('invoice', 'Accounting'),
      document('lesson-plan', 'Education'),
      document('custom', { Custom: 'architecture' }),
    ];
    expect(defaultSelectedDocumentIds(documents)).toEqual([]);
    for (const item of documents) expect(shouldSelectDocumentByDefault(item)).toBe(false);
  });

  it('does not special-case identical role ids by profession', () => {
    expect(shouldSelectDocumentByDefault(document('discharge', 'Medical'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('discharge', 'Legal'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('diaries', 'Medical'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('diaries', 'Education'))).toBe(false);
  });
});

describe('bundle decision presentation', () => {
  const routing: DocumentRoutingRecommendation = {
    domain: 'Legal', domain_confidence: 0.9, predicted_role: 'contract', cluster_id: 'contract', cluster_confidence: 0.9,
    recommended_document_ids: ['doc-contract'], matches: [], auto_select: false, review_required: true, reasons: [],
  };

  it('replaces selection with the exact review proposal without claiming automatic execution', () => {
    const decision: BundleDecision = { document_ids: ['doc-contract'], source: 'review_proposal', confidence: 0.9, auto_apply: false, review_required: true, question: 'Подтвердите', reasons: [] };
    expect(bundleSelectionFromDecision(decision, routing, [document('contract', 'Legal', 'Договор')])).toEqual({
      documentIds: ['doc-contract'],
      summary: ' Предложен комплект: Договор. Подтвердите состав перед созданием.',
    });
  });
});
