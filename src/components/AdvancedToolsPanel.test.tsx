import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { AdvancedToolsPanel } from './AdvancedToolsPanel';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import type { DocumentTemplateSpec, TemplateCandidateDto } from '../lib/types';

const originalFetch = globalThis.fetch;

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
