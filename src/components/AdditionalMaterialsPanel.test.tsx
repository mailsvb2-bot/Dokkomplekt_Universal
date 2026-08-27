import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { AdditionalMaterialsPanel, medicalDiagnosisKey, safeKey } from './AdditionalMaterialsPanel';
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
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0" />);

    const label = screen.getByText('Тексты').closest('label');
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
    expect(screen.getByRole('status').textContent).toContain('сохранено 2 из 2');
  });

  it('fails early when explicit diary text is picked before the current diagnosis is known', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'save_clause_block') savedBlocks.push((payload as { req?: { block_id?: string } })?.req?.block_id ?? '');
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['docx'], 'психотерапия.docx')] } });
    expect((await screen.findByRole('status')).textContent).toContain('Сначала укажите или подтвердите диагноз');
    expect(savedBlocks).toHaveLength(0);
  });

  it('binds explicitly selected diary Word text to the current diagnosis instead of guessing from the filename', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') {
        return { source_path: '/app-data/psychotherapy.docx', source_kind: 'docx', extracted_text: 'Достаточно длинный профессиональный текст дневника, выбранный врачом для текущего пациента.', warnings: [] } as T;
      }
      if (command === 'save_clause_block') {
        savedBlocks.push((payload as { req?: { block_id?: string } })?.req?.block_id ?? '');
        return [] as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Шизофрения параноидная" />);

    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['docx'], 'психотерапия.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => expect(savedBlocks).toEqual(['professional.medical.diary.regular.f200шизофренияпараноидная']));
    expect(screen.getByRole('status').textContent).toContain('Тексты привязаны к текущему диагнозу: F20.0 Шизофрения параноидная');
  });

  it('keeps good diary files when a folder also contains junk or one broken document', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        if (name === 'broken.docx') throw new Error('DOCX повреждён');
        return { source_path: `/app-data/${name}`, source_kind: 'txt', extracted_text: `Текст ${name}`, warnings: [] } as T;
      }
      if (command === 'save_clause_block') {
        const blockId = (payload as { req?: { block_id?: string } })?.req?.block_id ?? '';
        savedBlocks.push(blockId);
        return [] as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);

    const input = screen.getByText('выбрать папку «Тексты»').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    const good = new File(['статус'], 'Дневники F20.0.txt', { type: 'text/plain' });
    const broken = new File(['bad'], 'broken.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    const junk = new File(['system'], 'desktop.ini', { type: 'text/plain' });
    fireEvent.change(input, { target: { files: [good, broken, junk] } });

    const selection = await screen.findByRole('region', { name: 'Выбранные файлы дневников' });
    await waitFor(() => expect(within(selection).getByText('Сохранён')).toBeTruthy());
    expect(within(selection).getByText(/Ошибка импорта: DOCX повреждён/)).toBeTruthy();
    expect(within(selection).getByText('Пропущен: неподдерживаемый формат')).toBeTruthy();
    expect(savedBlocks).toEqual(['professional.medical.diary.regular.f200']);
    expect(screen.getByRole('status').textContent).toContain('сохранено 1 из 3; пропущено 1; ошибок 1');
  });

  it('shows Word files from the main Texts button and keeps whole-folder import explicit', () => {
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const fileInput = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement | null;
    const folderInput = screen.getByText('выбрать папку «Тексты»').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement | null;
    expect(fileInput).toBeTruthy();
    expect(fileInput?.multiple).toBe(true);
    expect(fileInput?.getAttribute('accept')).toContain('.docx');
    expect(fileInput?.getAttribute('accept')).toContain('.doc');
    expect(fileInput?.hasAttribute('webkitdirectory')).toBe(false);
    expect(folderInput?.hasAttribute('webkitdirectory')).toBe(true);
  });

  it('normalizes source names and diagnoses with separate keys', () => {
    expect(safeKey('Дневники ВЭ — Лёгкая депрессия с датами.docx')).toBe('вэлегкаядепрессиясдатами');
    expect(medicalDiagnosisKey('F20.0 Шизофрения параноидная')).toBe('f200шизофренияпараноидная');
  });
});
