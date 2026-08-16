import { describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec, DomainKind } from './types';
import {
  defaultSelectedDocumentIds,
  isMedicalDiaryDocument,
  loadOutputFolderParts,
  loadOutputNamingConfirmed,
  loadOutputRoot,
  OUTPUT_NAMING_CONFIRMED_KEY,
  OUTPUT_PREFS_KEY,
  OUTPUT_ROOT_KEY,
  outputNamingPreset,
  saveOutputFolderParts,
  saveOutputRoot,
  shouldSelectDocumentByDefault,
} from './appSupport';

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
    expect(loadOutputRoot()).toBe('output/Готовые документы');
    saveOutputRoot('  D:/Работа/Готовые документы  ');
    expect(loadOutputRoot()).toBe('D:/Работа/Готовые документы');
  });

  it('does not replace a remembered folder with an empty edit', () => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Documents/Ready');
    saveOutputRoot('   ');
    expect(loadOutputRoot()).toBe('C:/Documents/Ready');
  });
});

describe('output folder naming onboarding', () => {
  it('starts unconfirmed instead of pretending number + date was the user choice', () => {
    localStorage.removeItem(OUTPUT_PREFS_KEY);
    localStorage.removeItem(OUTPUT_NAMING_CONFIRMED_KEY);
    expect(loadOutputFolderParts()).toEqual([]);
    expect(loadOutputNamingConfirmed()).toBe(false);
  });

  it('persists an explicit profession-neutral preset and remembers it', () => {
    localStorage.removeItem(OUTPUT_PREFS_KEY);
    localStorage.removeItem(OUTPUT_NAMING_CONFIRMED_KEY);
    const preset = outputNamingPreset('organization_document');
    expect(preset?.parts).toEqual(['OrganizationName', 'DocumentNumber']);
    expect(saveOutputFolderParts(preset?.parts ?? [])).toEqual(['OrganizationName', 'DocumentNumber']);
    expect(loadOutputFolderParts()).toEqual(['OrganizationName', 'DocumentNumber']);
    expect(loadOutputNamingConfirmed()).toBe(true);
  });

  it('treats a pre-existing saved preference as already confirmed during upgrade', () => {
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['ShortInitials', 'ShortPeriodRange']));
    localStorage.removeItem(OUTPUT_NAMING_CONFIRMED_KEY);
    expect(loadOutputNamingConfirmed()).toBe(true);
  });

  it('clearing every part removes confirmation instead of restoring a silent default', () => {
    saveOutputFolderParts(['DocumentNumber']);
    expect(saveOutputFolderParts([])).toEqual([]);
    expect(loadOutputFolderParts()).toEqual([]);
    expect(loadOutputNamingConfirmed()).toBe(false);
  });
});

describe('medical diary role visibility', () => {
  it('recognizes only Medical diary roles', () => {
    expect(isMedicalDiaryDocument(document('diaries', 'Medical'))).toBe(true);
    expect(isMedicalDiaryDocument(document('medical.diary', 'Medical'))).toBe(true);
    expect(isMedicalDiaryDocument(document('diaries', 'Legal'))).toBe(false);
    expect(isMedicalDiaryDocument(document('primary', 'Medical'))).toBe(false);
  });
});

describe('default document selection', () => {
  it('keeps medical discharge and diaries off regardless of renamed button labels', () => {
    expect(shouldSelectDocumentByDefault(document('discharge', 'Medical', 'Мой документ'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('diaries', 'Medical', 'Ежедневные записи'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('medical.discharge', 'Medical'))).toBe(false);
    expect(shouldSelectDocumentByDefault(document('medical.diary', 'Medical'))).toBe(false);
  });


it('applies the same defaults to a whole pack after setup, startup, or reload', () => {
  const documents = [
    document('primary', 'Medical'),
    document('discharge', 'Medical'),
    document('diaries', 'Medical'),
    document('discharge', 'Legal'),
  ];
  expect(defaultSelectedDocumentIds(documents)).toEqual([
    'doc-primary',
    'doc-discharge',
  ]);
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
