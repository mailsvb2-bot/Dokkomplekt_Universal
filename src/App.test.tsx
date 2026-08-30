import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { App } from './App';
import { __resetInvokeForTests, __setInvokeForTests } from './lib/api';

const sampleDocument = {
  id: 'template_1',
  button_label: 'Акт выполненных работ',
  template_path: 'x.docx',
  category: 'Generic',
  role_id: 'generic',
  required_fields: [],
  placeholders: [],
  is_static_copy: true,
};

function installTemplateMock(staticCopy: boolean, includeRejected = false, rejectBrowserBad = false) {
  const calls: string[] = [];
  const confirmRequests: Array<Record<string, unknown> | undefined> = [];
  __setInvokeForTests(async (name: string, payload?: Record<string, unknown>) => {
    calls.push(name);
    if (name === 'first_run_state') return { pack: { pack_id: 'default', name: 'Набор', documents: [] }, has_user_buttons: false, message: 'Создайте свои кнопки' } as never;
    if (name === 'get_intake_capabilities') return [] as never;
    if (name === 'pick_template_files') return { files: [
      { file_name: 'Акт выполненных работ.docx', template_path: 'x.docx', extracted_text: staticCopy ? 'Акт выполненных работ' : 'Акт № {{document.number}}' },
      ...(includeRejected ? [{ file_name: 'Повреждённый.docx', template_path: '', extracted_text: '', import_error: 'Файл не распознан как DOCX' }] : []),
    ] } as never;
    if (name === 'import_template_file') {
      const req = payload?.req as { file_name?: string | null } | undefined;
      if (rejectBrowserBad && req?.file_name === 'Повреждённый.docx') throw new Error('Файл повреждён');
      return { template_path: 'x.docx', extracted_text: staticCopy ? 'Акт выполненных работ' : 'Акт № {{document.number}}' } as never;
    }
    if (name === 'analyze_template_file') return { document: { ...sampleDocument, is_static_copy: staticCopy, popup_fields: [] }, analysis_json: {}, core_pipeline_json: {} } as never;
    if (name === 'prepare_template_setup') {
      return [{
        document_id: 'template_1',
        template_path: 'x.docx',
        detected_title: 'Акт выполненных работ',
        suggested_button_label: 'Акт выполненных работ',
        editable_button_label: 'Акт выполненных работ',
        role_id: 'generic',
        is_static_copy: staticCopy,
        analysis: { is_static: staticCopy },
        popup_fields: [],
      }] as never;
    }
    if (name === 'confirm_template_setup') {
      confirmRequests.push(payload);
      return { pack_id: 'default', name: 'Набор', documents: [{ ...sampleDocument, is_static_copy: staticCopy }] } as never;
    }
    if (name === 'get_workflow_plan') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
    return {} as never;
  });
  return { calls, confirmRequests };
}

async function selectTemplateAndCreateButton() {
  const create = await screen.findByRole('button', { name: 'Создать свои кнопки' }) as HTMLButtonElement;
  await waitFor(() => expect(create.disabled).toBe(false));
  fireEvent.click(create);
  await screen.findByLabelText('Название документа для Акт выполненных работ.docx');
  fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (1)' }));
}

describe('App', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    __resetInvokeForTests();
  });

  it('starts without built-in examples and shows one clear create-buttons action', async () => {
    installTemplateMock(false);
    render(<App />);
    expect(await screen.findByRole('button', { name: 'Создать свои кнопки' })).toBeTruthy();
    expect(screen.queryByText('Встроенный пример')).toBeNull();
  });

  it('adds a document through the simple Rust-backed setup path', async () => {
    const { calls } = installTemplateMock(false);
    render(<App />);
    await selectTemplateAndCreateButton();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
    expect(calls).toContain('confirm_template_setup');
  });

  it('creates an unmarked template button without forcing optional auto-inference', async () => {
    const { calls, confirmRequests } = installTemplateMock(true);
    render(<App />);
    await selectTemplateAndCreateButton();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
    expect(calls).toContain('pick_template_files');
    expect(calls).toContain('confirm_template_setup');
    expect(confirmRequests.at(-1)).toMatchObject({
      req: { auto_infer_static_templates: false },
    });
  });

  it('keeps good templates when another selected DOCX is broken', async () => {
    installTemplateMock(false, true);
    render(<App />);
    const create = await screen.findByRole('button', { name: 'Создать свои кнопки' });
    await waitFor(() => expect((create as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(create);
    await screen.findByLabelText('Название документа для Акт выполненных работ.docx');
    expect(await screen.findByText(/Пропущено проблемных шаблонов: 1/)).toBeTruthy();
    expect(screen.getByText(/Повреждённый\.docx: Файл не распознан как DOCX/)).toBeTruthy();
  });

  it('keeps good modal-selected templates and reports a broken sibling file', async () => {
    installTemplateMock(false, false, true);
    render(<App />);
    const create = await screen.findByRole('button', { name: 'Создать свои кнопки' });
    await waitFor(() => expect((create as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(create);
    await screen.findByLabelText('Название документа для Акт выполненных работ.docx');
    const addMore = screen.getByText('Добавить ещё шаблоны').closest('label');
    const input = addMore?.querySelector('input[type="file"]') as HTMLInputElement;
    expect(input).toBeTruthy();
    fireEvent.change(input, { target: { files: [
      new File(['ok'], 'Дополнительный.docx'),
      new File(['bad'], 'Повреждённый.docx'),
    ] } });
    expect(await screen.findByLabelText('Название документа для Дополнительный.docx')).toBeTruthy();
    expect(await screen.findByText(/Пропущено проблемных шаблонов: 1/)).toBeTruthy();
    expect(screen.getByText(/Повреждённый\.docx: Файл повреждён/)).toBeTruthy();
  });

});
