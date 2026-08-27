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

  it('imports DOCX diary sources through universal intake and binds an ICD code from the file name', async () => {
    const commands: string[] = [];
    const replaceRequests: ReplaceRequest[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      commands.push(command);
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'import_learning_example_file') {
        const req = payload as { req?: { file_name?: string } };
        expect(req.req?.file_name).toBe('Дневники F20.0 с датами.docx');
        return {
          source_path: '/app-data/diary-source.docx',
          source_kind: 'docx',
          extracted_text: 'Статус из таблицы DOCX',
          warnings: [],
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

    const label = screen.getByText('Импортировать «Тексты» (TXT/DOCX/DOCM)').closest('label');
    const input = label?.querySelector('input[type="file"]') as HTMLInputElement | null;
    expect(input).toBeTruthy();
    expect(input?.getAttribute('accept')).toContain('.docx');
    expect(input?.getAttribute('accept')).toContain('.docm');

    const file = new File(
      [new Uint8Array([0x50, 0x4b, 0x03, 0x04])],
      'Дневники F20.0 с датами.docx',
      { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' },
    );
    fireEvent.change(input as HTMLInputElement, { target: { files: [file] } });

    await waitFor(() => expect(replaceRequests).toHaveLength(1));
    expect(commands).toContain('import_learning_example_file');
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


  it('does not publish a partial snapshot when one supported diary file is empty', async () => {
    const replaceRequests: ReplaceRequest[] = [];
    const importedFiles: string[] = [];
    const onStatus = vi.fn();
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'import_learning_example_file') {
        const req = payload as { req?: { file_name?: string } };
        const fileName = req.req?.file_name ?? '';
        importedFiles.push(fileName);
        return {
          source_path: `/app-data/${fileName}`,
          source_kind: 'docx',
          extracted_text: fileName.includes('пустой') ? '   ' : 'Корректный статус',
          warnings: [],
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
    const input = screen.getByText('Импортировать «Тексты» (TXT/DOCX/DOCM)')
      .closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['a'], 'Дневники F20.0.docx'),
      new File(['b'], 'Итоговый F20.0 пустой.docx'),
    ] } });

    await waitFor(() => expect(importedFiles).toHaveLength(2));
    await waitFor(() => expect(onStatus).toHaveBeenCalled());
    expect(replaceRequests).toHaveLength(0);
  });

  it('publishes regular and final files for one diagnosis as one atomic canonical snapshot', async () => {
    const replaceRequests: ReplaceRequest[] = [];
    __setInvokeForTests(async <T,>(command: string, payload?: Record<string, unknown>) => {
      if (command === 'list_clause_blocks') return [] as T;
      if (command === 'get_process_blueprints') {
        return { selected_process_id: null, processes: [], notice: '' } as T;
      }
      if (command === 'import_learning_example_file') {
        const req = payload as { req?: { file_name?: string } };
        const fileName = req.req?.file_name ?? '';
        return {
          source_path: `/app-data/${fileName}`,
          source_kind: 'docx',
          extracted_text: fileName.includes('Итоговый') ? 'Подтверждённый итоговый статус' : 'Подтверждённый обычный статус',
          warnings: [],
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

    const input = screen.getByText('Импортировать «Тексты» (TXT/DOCX/DOCM)')
      .closest('label')?.querySelector('input[type="file"]') as HTMLInputElement;
    fireEvent.change(input, { target: { files: [
      new File(['regular'], 'Дневники F20 . 0.docx'),
      new File(['final'], 'Итоговый F20.0.docx'),
    ] } });

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
