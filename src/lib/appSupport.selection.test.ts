import { describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec, DomainKind } from './types';
import { shouldSelectDocumentByDefault } from './appSupport';

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

describe('default document selection', () => {
  it('keeps medical discharge and diaries off regardless of renamed button labels', () => {
    expect(shouldSelectDocumentByDefault(document('discharge', 'Medical', 'Мой документ'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('diaries', 'Medical', 'Ежедневные записи'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('medical.discharge', 'Medical'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('medical.diary', 'Medical'))).toBe(false);
  });

  it('keeps other medical roles selected by default', () => {
    for (const role of ['primary', 'rvk_act', 'commission', 'vk_mse', 'sick_leave_vk', 'reception']) {
      expect(shouldSelectDocumentByDefault(document(role, 'Medical'))).toBe(true);
    }
  });

  it('never applies medical defaults to other professions even when role ids collide', () => {
    for (const category of ['Generic', 'Legal', 'Hr', 'Education', 'Accounting'] as DomainKind[]) {
      expect(shouldSelectDocumentByDefault(document('discharge', category))).toBe(true);
      expect(shouldSelectDocumentByDefault(document('diaries', category))).toBe(true);
    }
  });
});
