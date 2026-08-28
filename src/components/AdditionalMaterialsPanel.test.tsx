import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { DocumentTemplateSpec } from '../lib/types';
import { AdditionalMaterialsPanel, medicalDiagnosisKey, medicalDiaryFileKey, safeKey } from './AdditionalMaterialsPanel';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';

const medicalDiary: DocumentTemplateSpec = {
  id: 'diary', button_label: 'Дневники', template_path: 'diary.docx', category: 'Medical', role_id: 'diaries',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const legal: DocumentTemplateSpec = {
  id: 'contract', button_label: 'Договор', template_path: 'contract.docx', category: 'Legal', role_id: 'contract',
  required_fields: [], placeholders: [], is_static_copy: false,
};

type ReplaceClauseBlocksPayload = {
  req?: {
    delete_block_ids?: string[];
    blocks?: Array<{ block_id?: string; title?: string; content?: string }>;
  };
};

function replacementBlocks(payload?: Record<string, unknown>) {
  return (payload as ReplaceClauseBlocksPayload | undefined)?.req?.blocks ?? [];
}

function replacementDeleteIds(payload?: Record<string, unknown>) {
  return (payload as ReplaceClauseBlocksPayload | undefined)?.req?.delete_block_ids ?? [];
}

describe('AdditionalMaterialsPanel', () => {
  afterEach(() => { __resetInvokeForTests(); });

  it('keeps diary-specific inputs invisible for non-medical work', () => {
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    expect(screen.getByText('Дополнительные источники / материалы')).toBeTruthy();
    expect(screen.queryByText('Медицинские дневники')).toBeNull();
  });

  it('keeps Cyrillic generic material keys compatible with the universal backend contract', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'import_learning_example_file') return {
        source_path: '/app-data/договор.pdf', source_kind: 'pdf', extracted_text: 'Условия договора и реквизиты сторон.', warnings: [],
      } as T;
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'save_clause_block') {
        savedBlocks.push((payload as { req?: { block_id?: string } })?.req?.block_id ?? '');
        return [] as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    const input = screen.getByText('Добавить файлы').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['pdf'], 'договор.pdf', { type: 'application/pdf' })] } });

    await waitFor(() => {
      expect(savedBlocks).toContain('professional.material.legal.договор');
      expect(savedBlocks).toContain('professional.materials.index');
    });
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
      if (command === 'replace_clause_blocks') return true as T;
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
      if (command === 'replace_clause_blocks') savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
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
      if (command === 'replace_clause_blocks') {
        savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Шизофрения параноидная" />);

    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    const file = new File(['docx'], 'психотерапия.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => expect(savedBlocks).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]));
    expect(screen.getByRole('status').textContent).toContain('Тексты привязаны к текущему диагнозу: F20.0 Шизофрения параноидная');
  });

  it('invalidates explicitly selected diary texts when the diagnosis code changes but keeps them for wording-only edits', async () => {
    __setInvokeForTests(async <T,>(command: string) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') return { source_path: '/app-data/a.docx', source_kind: 'docx', extracted_text: 'Подтверждённый профессиональный текст дневника.', warnings: [] } as T;
      if (command === 'replace_clause_blocks') return true as T;
      throw new Error(`Unexpected command: ${command}`);
    });
    const view = render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Исходная формулировка" />);
    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['docx'], 'a.docx')] } });
    await screen.findByRole('region', { name: 'Выбранные файлы дневников' });

    view.rerender(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Уточнённая формулировка" />);
    expect(screen.getByRole('region', { name: 'Выбранные файлы дневников' })).toBeTruthy();

    view.rerender(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F32.1 Другой диагноз" />);
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Выбранные файлы дневников' })).toBeNull());
    expect(screen.getByRole('status').textContent).toContain('Диагноз изменён');
  });

  it('replaces stale diagnosis text and clears a stale final slot atomically on explicit re-import', async () => {
    const saved: Array<{ blockId: string; content: string }> = [];
    const deleted: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [{
        block_id: 'professional.medical.diary.regular.f200',
        title: 'old',
        content: 'СТАРЫЙ ТЕКСТ, который больше не должен использоваться',
        updated_at: '2026-08-01T00:00:00Z',
      }] as T;
      if (command === 'import_learning_example_file') return {
        source_path: '/app-data/new.docx', source_kind: 'docx', extracted_text: 'НОВЫЙ подтверждённый врачом текст', warnings: [],
      } as T;
      if (command === 'replace_clause_blocks') {
        deleted.push(...replacementDeleteIds(payload));
        saved.push(...replacementBlocks(payload).map(block => ({ blockId: block.block_id ?? '', content: block.content ?? '' })));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Новая формулировка" />);
    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['docx'], 'актуальный.docx')] } });
    await waitFor(() => expect(saved).toHaveLength(2));
    const regular = saved.find(block => block.blockId === 'professional.medical.diary.regular.f200');
    const final = saved.find(block => block.blockId === 'professional.medical.diary.final.f200');
    expect(regular?.content).toContain('НОВЫЙ подтверждённый врачом текст');
    expect(regular?.content).not.toContain('СТАРЫЙ ТЕКСТ');
    expect(final?.content).toBe('');
    expect(deleted).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]);
  });

  it('groups multiple folder files with the same ICD code into one diagnosis source', async () => {
    const saved: Array<{ blockId: string; content: string }> = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        return { source_path: `/app-data/${name}`, source_kind: 'txt', extracted_text: `Содержимое ${name}`, warnings: [] } as T;
      }
      if (command === 'replace_clause_blocks') {
        saved.push(...replacementBlocks(payload).map(block => ({ blockId: block.block_id ?? '', content: block.content ?? '' })));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const input = screen.getByText('выбрать папку «Тексты»').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['a'], 'Дневники F20.0 — вариант 1.txt'),
      new File(['b'], 'F20.0 вариант 2.txt'),
    ] } });
    await waitFor(() => expect(saved).toHaveLength(2));
    const regular = saved.find(block => block.blockId === 'professional.medical.diary.regular.f200');
    const final = saved.find(block => block.blockId === 'professional.medical.diary.final.f200');
    expect(regular?.content).toContain('вариант 1');
    expect(regular?.content).toContain('вариант 2');
    expect(final?.content).toBe('');
  });

  it('re-importing a folder replaces stale text for affected diagnosis keys', async () => {
    const saved: Array<{ blockId: string; content: string }> = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'import_learning_example_file') return {
        source_path: '/app-data/current.txt', source_kind: 'txt', extracted_text: 'АКТУАЛЬНЫЙ текст из текущей папки', warnings: [],
      } as T;
      if (command === 'replace_clause_blocks') {
        saved.push(...replacementBlocks(payload).map(block => ({ blockId: block.block_id ?? '', content: block.content ?? '' })));
        return true as T;
      }
      if (command === 'list_clause_blocks') throw new Error('diary folder re-import must not merge stale stored content');
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const input = screen.getByText('выбрать папку «Тексты»').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['current'], 'F20.0 актуальный.txt')] } });
    await waitFor(() => expect(saved).toHaveLength(2));
    expect(saved).toEqual(expect.arrayContaining([
      { blockId: 'professional.medical.diary.regular.f200', content: 'АКТУАЛЬНЫЙ текст из текущей папки' },
      { blockId: 'professional.medical.diary.final.f200', content: '' },
    ]));
  });

  it('keeps the previous diagnosis set intact when one explicitly selected file cannot be read', async () => {
    const replacements: ReplaceClauseBlocksPayload[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        if (name.includes('broken')) throw new Error('DOCX повреждён');
        return { source_path: `/app-data/${name}`, source_kind: 'docx', extracted_text: 'Новый корректный регулярный текст.', warnings: [] } as T;
      }
      if (command === 'replace_clause_blocks') {
        replacements.push(payload as ReplaceClauseBlocksPayload);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0" />);
    const input = screen.getByText('Тексты').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['ok'], 'regular.docx'),
      new File(['bad'], 'финал broken.docx'),
    ] } });

    const selection = await screen.findByRole('region', { name: 'Выбранные файлы дневников' });
    await waitFor(() => expect(within(selection).getByText(/Ошибка импорта: DOCX повреждён/)).toBeTruthy());
    expect(within(selection).getByText('Не сохранён: другой файл этого диагноза не прочитан')).toBeTruthy();
    expect(replacements).toHaveLength(0);
    expect(screen.getByRole('status').textContent).toContain('сохранено 0 из 2; пропущено 0; ошибок 2');
  });

  it('blocks only the damaged diagnosis while updating complete diagnoses from the same folder', async () => {
    const replacements: ReplaceClauseBlocksPayload[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        if (name.includes('broken')) throw new Error('DOCX повреждён');
        return { source_path: `/app-data/${name}`, source_kind: 'docx', extracted_text: `Текст ${name}`, warnings: [] } as T;
      }
      if (command === 'replace_clause_blocks') {
        replacements.push(payload as ReplaceClauseBlocksPayload);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const input = screen.getByText('выбрать папку «Тексты»').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['ok20'], 'F20.0 regular.docx'),
      new File(['bad20'], 'F20.0 финал broken.docx'),
      new File(['ok32'], 'F32.1 regular.docx'),
    ] } });

    await waitFor(() => expect(replacements).toHaveLength(1));
    expect(replacementDeleteIds(replacements[0] as unknown as Record<string, unknown>)).toEqual([
      'professional.medical.diary.regular.f321',
      'professional.medical.diary.final.f321',
    ]);
    expect(replacementBlocks(replacements[0] as unknown as Record<string, unknown>).map(block => block.block_id)).toEqual([
      'professional.medical.diary.regular.f321',
      'professional.medical.diary.final.f321',
    ]);
    expect(replacementBlocks(replacements[0] as unknown as Record<string, unknown>).find(block => block.block_id === 'professional.medical.diary.final.f321')?.content).toBe('');
    const selection = screen.getByRole('region', { name: 'Выбранные файлы дневников' });
    expect(within(selection).getByText('Не сохранён: другой файл этого диагноза не прочитан')).toBeTruthy();
    expect(within(selection).getByText(/Ошибка импорта: DOCX повреждён/)).toBeTruthy();
    expect(within(selection).getByText('Сохранён')).toBeTruthy();
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
      if (command === 'replace_clause_blocks') {
        savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
        return true as T;
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
    expect(savedBlocks).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]);
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

  it('normalizes source names and diagnoses with separate stable keys', () => {
    expect(safeKey('Дневники ВЭ — Лёгкая депрессия с датами.docx')).toBe('вэлегкаядепрессиясдатами');
    expect(medicalDiagnosisKey('F20.0 Шизофрения параноидная')).toBe('f200');
    expect(medicalDiagnosisKey('f20.0 Другая формулировка того же диагноза')).toBe('f200');
    expect(medicalDiagnosisKey('Депрессивный эпизод лёгкой степени')).toBe('депрессивныйэпизодлегкойстепени');
    expect(medicalDiaryFileKey('Дневники F20.0 — вариант 1.docx')).toBe('f200');
    expect(medicalDiaryFileKey('F20.0 вариант 2.txt')).toBe('f200');
  });
});
