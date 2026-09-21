import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { AdvancedToolsPanel } from './AdvancedToolsPanel';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import type { DocumentTemplateSpec, TemplateCandidateDto } from '../lib/types';

const originalFetch = globalThis.fetch;

type ReplaceRequest = {
  delete_block_ids: string[];
  blocks: Array<{ block_id: string; title: string; content: string }>;
};

describe('starter content packs', () => {
  beforeEach(() => {
    globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
      const raw = typeof input === 'string' ? input : input.toString();
      const pathname = new URL(raw, 'http://localhost').pathname;
      const payload = readFileSync(join(process.cwd(), 'public', pathname));
      return new Response(payload, { status: 200 });
    }) as typeof fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    __resetInvokeForTests();
    vi.restoreAllMocks();
  });

  it('verifies bundled bytes and installs a draft pack through the Rust setup route', async () => {
    const commands: string[] = [];
    const installed: DocumentTemplateSpec[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      commands.push(command);
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'import_template_file') {
        const req = payload?.req as { document_id: string; file_name?: string };
        return {
          template_path: `/app-data/${req.document_id}.docx`,
          extracted_text: req.file_name ?? req.document_id,
        } as T;
      }
      if (command === 'prepare_template_setup') {
        const candidates = (payload?.req as { candidates: TemplateCandidateDto[] }).candidates;
        return candidates.map((candidate) => ({
          document_id: candidate.document_id,
          template_path: candidate.template_path,
          detected_title: candidate.preferred_button_label ?? candidate.document_id,
          suggested_button_label: candidate.preferred_button_label ?? candidate.document_id,
          editable_button_label: candidate.preferred_button_label ?? candidate.document_id,
          role_id: candidate.document_id.split('.').at(-1) ?? 'document',
          is_static_copy: false,
          analysis: {},
          popup_fields: [],
        })) as T;
      }
      if (command === 'confirm_template_setup') {
        const rows = (payload?.req as { rows: Array<Record<string, unknown>> }).rows;
        const documents = rows.map((row) => ({
          id: String(row.document_id),
          button_label: String(row.editable_button_label),
          template_path: String(row.template_path),
          category: 'Accounting' as const,
          role_id: String(row.role_id),
          required_fields: [],
          placeholders: ['document.date'],
          is_static_copy: false,
        }));
        installed.push(...documents);
        return { pack_id: 'default', name: 'Documents', documents } as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const onDocumentsChanged = vi.fn();
    render(
      <AdvancedToolsPanel
        documents={[]}
        selectedDocumentIds={[]}
        outputRoot="output"
        onStatus={vi.fn()}
        onDocumentsChanged={onDocumentsChanged}
      />,
    );

    const accounting = screen.getByText('Бухгалтерия').closest('article');
    expect(accounting).toBeTruthy();
    fireEvent.click(within(accounting as HTMLElement).getByRole('button', { name: 'Установить starter-пак' }));

    await waitFor(() => expect(onDocumentsChanged).toHaveBeenCalledTimes(1));
    expect(installed).toHaveLength(3);
    expect(commands.filter((command) => command === 'import_template_file')).toHaveLength(3);
    expect(commands).toContain('prepare_template_setup');
    expect(commands).toContain('confirm_template_setup');
  });
});

describe('medical diary donor parity', () => {
  afterEach(() => {
    __resetInvokeForTests();
    vi.restoreAllMocks();
  });

  it('imports DOCX diary sources through the native picker and binds an ICD code from the file name', async () => {
    const commands: string[] = [];
    const replaceRequests: ReplaceRequest[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      commands.push(command);
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'pick_learning_files') {
        expect((payload as { req?: { kind?: string } })?.req?.kind).toBe('medical_diary');
        return {
          files: [{
            file_name: 'Дневники F20.0 с датами.docx',
            staged_path: '/app-data/diary-source.docx',
            content_sha256: 'diary-source-sha',
            extracted_text: 'Статус из таблицы DOCX',
          }],
        } as T;
      }
      if (command === 'replace_clause_blocks') {
        replaceRequests.push((payload?.req ?? payload) as ReplaceRequest);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const medicalDocument: DocumentTemplateSpec = {
      id: 'medical.diaries',
      button_label: 'Дневники наблюдения',
      template_path: '/templates/diaries.docx',
      category: 'Medical',
      role_id: 'diaries',
      required_fields: [],
      placeholders: ['diary.text'],
      is_static_copy: false,
    };
    render(
      <AdvancedToolsPanel
        documents={[medicalDocument]}
        selectedDocumentIds={[]}
        outputRoot="output"
        onStatus={vi.fn()}
        onDocumentsChanged={vi.fn()}
      />,
    );

    const button = screen.getByRole('button', { name: 'Импортировать «Тексты» (TXT/DOCX/DOCM)' });
    fireEvent.click(button);

    await waitFor(() => expect(replaceRequests).toHaveLength(1));
    expect(commands).toContain('pick_learning_files');
    expect(commands).not.toContain('import_learning_example_file');
    expect(commands).toContain('replace_clause_blocks');
    const [request] = replaceRequests;
    expect(request.delete_block_ids).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]);
    expect(request.blocks).toEqual(expect.arrayContaining([
      expect.objectContaining({
        block_id: 'professional.medical.diary.regular.f200',
        content: 'Статус из таблицы DOCX',
      }),
      expect.objectContaining({
        block_id: 'professional.medical.diary.final.f200',
        content: '',
      }),
    ]));
  });


  it('does not publish a partial snapshot when one native-picked supported diary file is empty', async () => {
    const replaceRequests: ReplaceRequest[] = [];
    const onStatus = vi.fn();
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'pick_learning_files') {
        expect((payload as { req?: { kind?: string } })?.req?.kind).toBe('medical_diary');
        return {
          files: [
            {
              file_name: 'Дневники F20.0.docx',
              staged_path: '/app-data/regular.docx',
              content_sha256: 'regular-sha',
              extracted_text: 'Корректный статус',
            },
            {
              file_name: 'Итоговый F20.0 пустой.docx',
              staged_path: '/app-data/final-empty.docx',
              content_sha256: 'final-empty-sha',
              extracted_text: '   ',
            },
          ],
        } as T;
      }
      if (command === 'replace_clause_blocks') {
        replaceRequests.push((payload?.req ?? payload) as ReplaceRequest);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const medicalDocument: DocumentTemplateSpec = {
      id: 'medical.diaries', button_label: 'Дневники наблюдения', template_path: '/templates/diaries.docx',
      category: 'Medical', role_id: 'diaries', required_fields: [], placeholders: ['diary.text'], is_static_copy: false,
    };
    render(<AdvancedToolsPanel documents={[medicalDocument]} selectedDocumentIds={[]} outputRoot="output" onStatus={onStatus} onDocumentsChanged={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Импортировать «Тексты» (TXT/DOCX/DOCM)' }));

    await waitFor(() => expect(onStatus).toHaveBeenCalled());
    expect(replaceRequests).toHaveLength(0);
  });

  it('surfaces a native diary extraction error and does not replace the canonical snapshot', async () => {
    const replaceRequests: ReplaceRequest[] = [];
    const onStatus = vi.fn();
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'pick_learning_files') {
        expect((payload as { req?: { kind?: string } })?.req?.kind).toBe('medical_diary');
        return {
          files: [{
            file_name: 'Дневники F20.0 повреждённый.docx',
            staged_path: '/app-data/corrupt.docx',
            content_sha256: 'corrupt-sha',
            extracted_text: null,
            import_error: 'DOCX package is corrupt',
          }],
        } as T;
      }
      if (command === 'replace_clause_blocks') {
        replaceRequests.push((payload?.req ?? payload) as ReplaceRequest);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const medicalDocument: DocumentTemplateSpec = {
      id: 'medical.diaries', button_label: 'Дневники наблюдения', template_path: '/templates/diaries.docx',
      category: 'Medical', role_id: 'diaries', required_fields: [], placeholders: ['diary.text'], is_static_copy: false,
    };
    render(<AdvancedToolsPanel documents={[medicalDocument]} selectedDocumentIds={[]} outputRoot="output" onStatus={onStatus} onDocumentsChanged={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Импортировать «Тексты» (TXT/DOCX/DOCM)' }));

    await waitFor(() => expect(onStatus).toHaveBeenCalledWith(
      expect.stringContaining('DOCX package is corrupt'),
    ));
    expect(onStatus).toHaveBeenCalledWith(
      expect.stringContaining('Дневники F20.0 повреждённый.docx'),
    );
    expect(replaceRequests).toHaveLength(0);
  });

  it('publishes native-picked regular and final files for one diagnosis as one atomic canonical snapshot', async () => {
    const replaceRequests: ReplaceRequest[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'pick_learning_files') {
        expect((payload as { req?: { kind?: string } })?.req?.kind).toBe('medical_diary');
        return {
          files: [
            {
              file_name: 'Дневники F20 . 0.docx',
              staged_path: '/app-data/regular.docx',
              content_sha256: 'regular-sha',
              extracted_text: 'Подтверждённый обычный статус',
            },
            {
              file_name: 'Итоговый F20.0.docx',
              staged_path: '/app-data/final.docx',
              content_sha256: 'final-sha',
              extracted_text: 'Подтверждённый итоговый статус',
            },
          ],
        } as T;
      }
      if (command === 'replace_clause_blocks') {
        replaceRequests.push((payload?.req ?? payload) as ReplaceRequest);
        return true as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const medicalDocument: DocumentTemplateSpec = {
      id: 'medical.diaries',
      button_label: 'Дневники наблюдения',
      template_path: '/templates/diaries.docx',
      category: 'Medical',
      role_id: 'diaries',
      required_fields: [],
      placeholders: ['diary.text'],
      is_static_copy: false,
    };
    render(
      <AdvancedToolsPanel
        documents={[medicalDocument]}
        selectedDocumentIds={[]}
        outputRoot="output"
        onStatus={vi.fn()}
        onDocumentsChanged={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Импортировать «Тексты» (TXT/DOCX/DOCM)' }));

    await waitFor(() => expect(replaceRequests).toHaveLength(1));
    const [request] = replaceRequests;
    expect(request.delete_block_ids).toEqual([
      'professional.medical.diary.regular.f200',
      'professional.medical.diary.final.f200',
    ]);
    expect(request.blocks).toEqual(expect.arrayContaining([
      expect.objectContaining({
        block_id: 'professional.medical.diary.regular.f200',
        content: 'Подтверждённый обычный статус',
      }),
      expect.objectContaining({
        block_id: 'professional.medical.diary.final.f200',
        content: 'Подтверждённый итоговый статус',
      }),
    ]));
  });
});

describe('template learning native picker controls', () => {
  afterEach(() => {
    __resetInvokeForTests();
    vi.restoreAllMocks();
  });

  it('uses the Rust-backed native picker instead of hidden WebView file inputs', async () => {
    const pickerKinds: string[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: 'learning controls ready' } as T;
      }
      if (command === 'pick_learning_files') {
        const kind = String((payload as { req?: { kind?: string } })?.req?.kind ?? '');
        pickerKinds.push(kind);
        return { files: [{ file_name: 'blank.docx', staged_path: 'C:/AppData/template-learning-inputs/session/blank.docx', content_sha256: 'blank-sha' }] } as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    const { container } = render(
      <AdvancedToolsPanel
        documents={[]}
        selectedDocumentIds={[]}
        outputRoot="output"
        onStatus={vi.fn()}
        onDocumentsChanged={vi.fn()}
      />,
    );

    await screen.findByText('learning controls ready');
    const blankButton = screen.getByRole('button', { name: 'Пустой DOCX/DOCM' });
    fireEvent.click(blankButton);

    await screen.findByText('blank.docx');
    expect(pickerKinds).toEqual(['blank']);
    expect(container.querySelector('.templateLearningCard input[type="file"]')).toBeNull();
  });

  it('accumulates repeated native selections until four matched pairs are ready and deduplicates by content hash', async () => {
    let outputIndex = 0;
    let sourceIndex = 0;
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: 'learning controls ready' } as T;
      }
      if (command === 'pick_learning_files') {
        const kind = String((payload as { req?: { kind?: string } })?.req?.kind ?? '');
        if (kind === 'blank') {
          return { files: [{ file_name: 'blank.docx', staged_path: 'C:/learning/blank.docx', content_sha256: 'blank-sha' }] } as T;
        }
        if (kind === 'correct_output') {
          outputIndex += 1;
          return { files: [{ file_name: `correct-${outputIndex}.docx`, staged_path: `C:/learning/correct-${outputIndex}.docx`, content_sha256: `correct-sha-${outputIndex}` }] } as T;
        }
        sourceIndex += 1;
        const stableIndex = Math.min(sourceIndex, 4);
        return { files: [{ file_name: `source-${stableIndex}.txt`, staged_path: `C:/learning/source-${sourceIndex}.txt`, content_sha256: `source-sha-${stableIndex}` }] } as T;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    render(
      <AdvancedToolsPanel
        documents={[]}
        selectedDocumentIds={[]}
        outputRoot="output"
        onStatus={vi.fn()}
        onDocumentsChanged={vi.fn()}
      />,
    );

    await screen.findByText('learning controls ready');
    fireEvent.click(screen.getByRole('button', { name: 'Пустой DOCX/DOCM' }));
    await screen.findByText('blank.docx');

    for (let index = 1; index <= 4; index += 1) {
      fireEvent.click(screen.getByRole('button', { name: /4–10 правильных результатов/ }));
      await screen.findByRole('status', { name: `Собрано: Correct Output ${index}/4–10, Source 0/4–10.` });
    }
    const analyzeButton = screen.getByRole('button', { name: 'Проверить пары и предложить карту' }) as HTMLButtonElement;
    expect(analyzeButton.disabled).toBe(true);

    for (let index = 1; index <= 4; index += 1) {
      fireEvent.click(screen.getByRole('button', { name: /4–10 исходных документов Source/ }));
      if (index < 4) {
        await screen.findByRole('status', { name: `Собрано: Correct Output 4/4–10, Source ${index}/4–10.` });
      }
    }
    expect(await screen.findByText('Готово к проверке. Пар: 4.')).toBeTruthy();
    expect(analyzeButton.disabled).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: /4–10 исходных документов Source/ }));
    await screen.findByText('Готово к проверке. Пар: 4.');
    expect(screen.getByText('4 файл(ов)')).toBeTruthy();
  });
});
