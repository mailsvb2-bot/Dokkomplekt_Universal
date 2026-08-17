import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { AdditionalMaterialsPanel, safeKey } from './AdditionalMaterialsPanel';

const medicalDiary: DocumentTemplateSpec = {
  id: 'diary', button_label: 'Дневники', template_path: 'diary.docx', category: 'Medical', role_id: 'diaries',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const legal: DocumentTemplateSpec = {
  id: 'contract', button_label: 'Договор', template_path: 'contract.docx', category: 'Legal', role_id: 'contract',
  required_fields: [], placeholders: [], is_static_copy: false,
};

describe('AdditionalMaterialsPanel', () => {
  it('keeps diary-specific inputs invisible for non-medical work', () => {
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    expect(screen.getByText('Дополнительные источники / материалы')).toBeTruthy();
    expect(screen.queryByText('Медицинские дневники')).toBeNull();
  });

  it('uses the donor program calendar and asks only for Texts in the normal diary flow', () => {
    render(<AdditionalMaterialsPanel documents={[medicalDiary, legal]} selectedDocumentIds={['diary']} busy={false} />);
    expect(screen.getByText('Медицинские дневники')).toBeTruthy();
    expect(screen.getByText('Тексты')).toBeTruthy();
    expect(screen.queryByText('Даты')).toBeNull();
    expect(screen.getByText(/Отдельная папка «Даты 01–31» для обычного создания не нужна/)).toBeTruthy();
    expect(screen.getByText(/сама построит календарь D0\+1 → выписка/)).toBeTruthy();
  });

  it('normalizes source names without embedding psychiatric aliases in UI logic', () => {
    expect(safeKey('Дневники ВЭ — Лёгкая депрессия с датами.docx')).toBe('вэлегкаядепрессиясдатами');
  });
});
