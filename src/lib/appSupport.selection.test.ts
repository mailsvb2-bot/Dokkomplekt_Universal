import { describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec, DomainKind } from './types';
import { defaultSelectedDocumentIds, loadOutputRoot, OUTPUT_ROOT_KEY, saveOutputRoot, shouldSelectDocumentByDefault } from './appSupport';

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

  it('does not replace a remembered folder with an empty edit', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
    saveOutputRoot('   ');
    expect(loadOutputRoot()).toBe('C:/Documents/Ready');
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
