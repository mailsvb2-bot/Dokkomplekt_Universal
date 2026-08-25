import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { AdditionalMaterialsPanel, safeKey } from './AdditionalMaterialsPanel';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';

const medicalDiary: DocumentTemplateSpec = {
  id: 'diary', button_label: 'Дневники', template_path: 'diary.docx', category: 'Medical', role_id: 'diaries',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const legal: DocumentTemplateSpec = {
  id: 'contract', button_label: 'Договор', template_path: 'contract.docx', category: 'Legal', role_id: 'contract',
  required_fields: [], placeholders: [], is_static_copy: false,
};

describe('AdditionalMaterialsPanel', () => {
  afterEach(() => { __resetInvokeForTests(); });

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

  it('shows the selected diary files and their import result after file picking', async () => {
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        return { source_path: `/app-data/${name}`, source_kind: 'txt', extracted_text: `Текст ${name}`, warnings: [] } as T;
      }
      if (command === 'save_clause_block') return [] as T;
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);

    const label = screen.getByText('выбрать отдельные файлы').closest('label');
    const input = label?.querySelector('input[type="file"]') as HTMLInputElement | null;
    expect(input).toBeTruthy();
    expect(input?.multiple).toBe(true);
    const first = new File(['статус 1'], 'Дневники F20.0.txt', { type: 'text/plain' });
    const second = new File(['статус 2'], 'Дневники F32.1.txt', { type: 'text/plain' });
    fireEvent.change(input as HTMLInputElement, { target: { files: [first, second] } });

    const selection = await screen.findByRole('region', { name: 'Выбранные файлы дневников' });
    expect(within(selection).getByText('Выбрано файлов: 2')).toBeTruthy();
    expect(within(selection).getByText('Дневники F20.0.txt')).toBeTruthy();
    expect(within(selection).getByText('Дневники F32.1.txt')).toBeTruthy();
    await waitFor(() => expect(within(selection).getAllByText('Сохранён')).toHaveLength(2));
    expect(screen.getByRole('status').textContent).toContain('2 из 2 файл');
  });

  it('keeps the Texts control wired as a folder picker', () => {
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const label = screen.getByText('Тексты').closest('label');
    const input = label?.querySelector('input[type="file"]') as HTMLInputElement | null;
    expect(input).toBeTruthy();
    expect(input?.multiple).toBe(true);
    expect(input?.hasAttribute('webkitdirectory')).toBe(true);
  });

  it('normalizes source names without embedding psychiatric aliases in UI logic', () => {
    expect(safeKey('Дневники ВЭ — Лёгкая депрессия с датами.docx')).toBe('вэлегкаядепрессиясдатами');
  });
});
