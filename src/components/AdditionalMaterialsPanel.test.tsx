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
const hr: DocumentTemplateSpec = {
  id: 'hr-order', button_label: 'Приказ', template_path: 'order.docx', category: 'Hr', role_id: 'order',
  required_fields: [], placeholders: [], is_static_copy: false,
};
const generic: DocumentTemplateSpec = {
  id: 'generic-note', button_label: 'Заметка', template_path: 'note.docx', category: 'Generic', role_id: 'note',
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

function pickedDiaryFile(fileName: string, extractedText?: string, importError?: string) {
  return {
    file_name: fileName,
    staged_path: `/app-data/native-diary/${fileName}`,
    content_sha256: `sha256-${fileName}`,
    extracted_text: extractedText ?? null,
    import_error: importError ?? null,
  };
}

describe('AdditionalMaterialsPanel', () => {
  afterEach(() => { __resetInvokeForTests(); });

  it('keeps diary-specific inputs invisible for non-medical work', () => {
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    expect(screen.getByText('Дополнительные источники / материалы')).toBeTruthy();
    expect(screen.queryByText('Медицинские дневники')).toBeNull();
  });

  it('keeps Cyrillic generic material keys compatible with the universal backend contract and publishes content plus index atomically', async () => {
    const savedBlocks: string[] = [];
    let replaceCalls = 0;
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'import_learning_example_file') return {
        source_path: '/app-data/договор.pdf', source_kind: 'pdf', extracted_text: 'Условия договора и реквизиты сторон.', warnings: [],
      } as T;
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'replace_clause_blocks') {
        replaceCalls += 1;
        savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    const input = screen.getByText('Добавить файлы').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [new File(['pdf'], 'договор.pdf', { type: 'application/pdf' })] } });

    await waitFor(() => {
      expect(replaceCalls).toBe(1);
      expect(savedBlocks).toContain('professional.material.legal.договор');
      expect(savedBlocks).toContain('professional.materials.index');
    });
  });

  it('deduplicates generic replacements when different source names normalize to one canonical block ID', async () => {
    let savedPayload: ReplaceClauseBlocksPayload | null = null;
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_learning_example_file') {
        const name = (payload as { req?: { file_name?: string } })?.req?.file_name ?? '';
        return { source_path: `/app-data/${name}`, source_kind: 'txt', extracted_text: `Текст из ${name}`, warnings: [] } as T;
      }
      if (command === 'replace_clause_blocks') {
        savedPayload = payload as ReplaceClauseBlocksPayload;
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[legal]} selectedDocumentIds={['contract']} busy={false} />);
    const input = screen.getByText('Добавить файлы').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['pdf'], 'policy.pdf', { type: 'application/pdf' }),
      new File(['docx'], 'policy.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' }),
    ] } });

    await waitFor(() => expect(savedPayload).not.toBeNull());
    const blocks = replacementBlocks(savedPayload as unknown as Record<string, unknown>);
    const ids = blocks.map(block => block.block_id ?? '');
    expect(ids).toEqual(['professional.material.legal.policy', 'professional.materials.index']);
    expect(new Set(ids).size).toBe(ids.length);
    expect(blocks[0]?.content).toBe('Текст из policy.docx');
    const index = JSON.parse(blocks[1]?.content ?? '[]') as Array<{ block_id: string; file_name: string }>;
    expect(index).toEqual([{
      block_id: 'professional.material.legal.policy',
      file_name: 'policy.docx',
      domain: 'legal',
      imported_at: expect.any(String),
    }]);
    expect(replacementDeleteIds(savedPayload as unknown as Record<string, unknown>)).toEqual(ids);
  });

  it('rejects a multi-domain import before extraction when expanded atomic blocks exceed the backend limit', async () => {
    const commands: string[] = [];
    __setInvokeForTests(async <T,>(command: string) => {
      commands.push(command);
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel
      documents={[legal, hr, generic]}
      selectedDocumentIds={['contract', 'hr-order', 'generic-note']}
      busy={false}
    />);
    const input = screen.getByText('Добавить файлы').closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    const files = Array.from({ length: 171 }, (_, index) => new File([String(index)], `source-${index}.txt`, { type: 'text/plain' }));
    fireEvent.change(input, { target: { files } });

    await waitFor(() => expect(screen.getByRole('status').textContent).toContain('514 профильных блоков'));
    expect(screen.getByRole('status').textContent).toContain('лимите 512');
    expect(commands).toEqual([]);
  });

  it('uses the donor program calendar and asks only for Texts in the normal diary flow', () => {
    render(<AdditionalMaterialsPanel documents={[medicalDiary, legal]} selectedDocumentIds={['diary']} busy={false} />);
    expect(screen.getByText('Медицинские дневники')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Тексты' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'выбрать папку «Тексты»' })).toBeTruthy();
    expect(screen.queryByText('Даты')).toBeNull();
    expect(screen.getByText(/Отдельная папка «Даты 01–31» для обычного создания не нужна/)).toBeTruthy();
    expect(screen.getByText(/сама построит календарь D0\+1 → выписка/)).toBeTruthy();
  });

  it('shows the selected diary files and their import result after native file picking', async () => {
    __setInvokeForTests(async <T,>(command: string) => {
      if (command === 'pick_learning_files') return {
        files: [
          pickedDiaryFile('Дневники F20.0.txt', 'Текст Дневники F20.0.txt'),
          pickedDiaryFile('Дневники F32.1.txt', 'Текст Дневники F32.1.txt'),
        ],
      } as T;
      if (command === 'replace_clause_blocks') return true as T;
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0" />);

    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));

    const selection = await screen.findByRole('region', { name: 'Выбранные файлы дневников' });
    expect(within(selection).getByText('Выбрано файлов: 2')).toBeTruthy();
    expect(within(selection).getByText('Дневники F20.0.txt')).toBeTruthy();
    expect(within(selection).getByText('Дневники F32.1.txt')).toBeTruthy();
    await waitFor(() => expect(within(selection).getAllByText('Сохранён')).toHaveLength(2));
    const status = screen.getByRole('status');
    expect(status.textContent).toContain('сохранено 2 из 2');
    expect(status.id).toBe('additional-materials-status');
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(status.getAttribute('aria-label')).toContain('сохранено 2 из 2');
  });

  it('fails early when explicit diary text is picked before the current diagnosis is known', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'replace_clause_blocks') savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));
    expect((await screen.findByRole('status')).textContent).toContain('Сначала укажите или подтвердите диагноз');
    expect(savedBlocks).toHaveLength(0);
  });

  it('binds explicitly selected diary Word text to the current diagnosis instead of guessing from the filename', async () => {
    const savedBlocks: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'pick_learning_files') return {
        files: [pickedDiaryFile('психотерапия.docx', 'Достаточно длинный профессиональный текст дневника, выбранный врачом для текущего пациента.')],
      } as T;
      if (command === 'replace_clause_blocks') {
        savedBlocks.push(...replacementBlocks(payload).map(block => block.block_id ?? ''));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Шизофрения параноидная" />);

    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));

    await waitFor(() => expect(savedBlocks).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]));
    expect(screen.getByRole('status').textContent).toContain('Тексты привязаны к текущему диагнозу: F20.0 Шизофрения параноидная');
  });

  it('invalidates explicitly selected diary texts when the diagnosis code changes but keeps them for wording-only edits', async () => {
    __setInvokeForTests(async <T,>(command: string) => {
      if (command === 'pick_learning_files') return {
        files: [pickedDiaryFile('a.docx', 'Подтверждённый профессиональный текст дневника.')],
      } as T;
      if (command === 'replace_clause_blocks') return true as T;
      throw new Error(`Unexpected command: ${command}`);
    });
    const view = render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Исходная формулировка" />);
    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));
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
      if (command === 'pick_learning_files') return {
        files: [pickedDiaryFile('актуальный.docx', 'НОВЫЙ подтверждённый врачом текст')],
      } as T;
      if (command === 'replace_clause_blocks') {
        deleted.push(...replacementDeleteIds(payload));
        saved.push(...replacementBlocks(payload).map(block => ({ blockId: block.block_id ?? '', content: block.content ?? '' })));
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0 Новая формулировка" />);
    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));
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
    const input = document.querySelector('#medical-diary-text-folder') as HTMLInputElement;
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
    const input = document.querySelector('#medical-diary-text-folder') as HTMLInputElement;
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
      if (command === 'pick_learning_files') return {
        files: [
          pickedDiaryFile('regular.docx', 'Новый корректный регулярный текст.'),
          pickedDiaryFile('финал broken.docx', undefined, 'DOCX повреждён'),
        ],
      } as T;
      if (command === 'replace_clause_blocks') {
        replacements.push(payload as ReplaceClauseBlocksPayload);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} medicalDiagnosis="F20.0" />);
    fireEvent.click(screen.getByRole('button', { name: 'Тексты' }));

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
    const input = document.querySelector('#medical-diary-text-folder') as HTMLInputElement;
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

    const input = document.querySelector('#medical-diary-text-folder') as HTMLInputElement;
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

  it('shows keyboard-accessible diary picker buttons and keeps whole-folder import explicit', () => {
    render(<AdditionalMaterialsPanel documents={[medicalDiary]} selectedDocumentIds={['diary']} busy={false} />);
    const textsButton = screen.getByRole('button', { name: 'Тексты' });
    const folderButton = screen.getByRole('button', { name: 'выбрать папку «Тексты»' });
    const fileInput = document.querySelector('#medical-diary-text-files') as HTMLInputElement | null;
    const folderInput = document.querySelector('#medical-diary-text-folder') as HTMLInputElement | null;
    expect(textsButton.getAttribute('aria-controls')).toBeNull();
    expect(folderButton.getAttribute('aria-controls')).toBe('medical-diary-text-folder');
    expect(fileInput).toBeNull();
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
