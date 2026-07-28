import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

function installTemplateMock(staticCopy: boolean) {
  const calls: string[] = [];
  __setInvokeForTests(async (name: string) => {
    calls.push(name);
    if (name === 'first_run_state') return { pack: { pack_id: 'default', name: 'Набор', documents: [] }, has_user_buttons: false } as never;
    if (name === 'ensure_created_documents_folder') return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false } as never;
    if (name === 'install_background_watcher') return { platform: 'windows', installed: true, watch_folder: 'C:/Users/Test/Desktop/Созданные документы', commands: [], warnings: [] } as never;
    if (name === 'get_intake_capabilities') return [] as never;
    if (name === 'import_template_file') return { template_path: 'x.docx', extracted_text: 'Акт выполненных работ' } as never;
    if (name === 'analyze_template_file') return { document: { popup_fields: [] } } as never;
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
      return { pack_id: 'default', name: 'Набор', documents: [{ ...sampleDocument, is_static_copy: staticCopy }] } as never;
    }
    if (name === 'get_workflow_plan' || name === 'get_workflow_plan_batch') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
    if (name === 'parse_source_file') return { source_text: 'Исходный документ', source_path: 'C:/app-data/source.docx', source_kind: 'word', semantic_case: { values: {} }, report: { recognized_title: 'Исходный документ', warnings: [] } } as never;
    if (name === 'semantic_extract') return { fields: [], warnings: [], model_applied: false, prompt: '' } as never;
    if (name === 'render_docx_batch') return { output_folder: 'C:/Users/Test/Desktop/Созданные документы/Готово', created_files: ['C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx'], created_documents: [{ document_id: 'template_1', label: 'Акт выполненных работ', path: 'C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx' }] } as never;
    return {} as never;
  });
  return calls;
}

function templateFile(name = 'Акт выполненных работ.docx') {
  return new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], name, {
    type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  });
}

async function openTemplateSetup() {
  fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
  return screen.getByTestId('template-file-input');
}

async function selectTemplateAndCreateButton() {
  const input = await openTemplateSetup();
  fireEvent.change(input, { target: { files: [templateFile()] } });
  await screen.findByLabelText('Название документа для Акт выполненных работ.docx');
  fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (1)' }));
}

describe('App', () => {
  beforeEach(() => { localStorage.setItem('dokkomplekt.created-documents-folder-prompt.v1', 'done'); });
  afterEach(() => {
    vi.restoreAllMocks();
    __resetInvokeForTests();
  });

  it('offers and creates the desktop work folder on a clean profile', async () => {
    localStorage.removeItem('dokkomplekt.created-documents-folder-prompt.v1');
    const calls = installTemplateMock(false);
    render(<App />);
    const dialog = await screen.findByRole('dialog', { name: 'Первичная настройка' });
    fireEvent.click(screen.getByRole('button', { name: 'Создать папку' }));
    await waitFor(() => expect(calls).toContain('ensure_created_documents_folder'));
    await waitFor(() => expect(calls).toContain('install_background_watcher'));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Первичная настройка' })).toBeNull());
    expect(dialog).toBeTruthy();
  });

  it('starts without built-in examples and shows only the clear first-run action', async () => {
    installTemplateMock(false);
    render(<App />);
    expect(await screen.findByRole('button', { name: 'Создать свои кнопки' })).toBeTruthy();
    expect(screen.queryByText('Встроенный пример')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Дополнительные настройки' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Настройки' })).toBeNull();
    expect(screen.queryByRole('region', { name: 'Дополнительные инструменты' })).toBeNull();
  });

  it('adds a document through the simple Rust-backed setup path', async () => {
    const calls = installTemplateMock(false);
    render(<App />);
    await selectTemplateAndCreateButton();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
    expect(calls).toContain('confirm_template_setup');
  });

  it('creates a button for an ordinary DOCX without placeholders', async () => {
    const calls = installTemplateMock(true);
    render(<App />);
    await selectTemplateAndCreateButton();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
    expect(calls).toContain('confirm_template_setup');
  });

  it('selects newly created document buttons and toggles the whole tile with one click', async () => {
    installTemplateMock(true);
    render(<App />);
    await selectTemplateAndCreateButton();
    const tile = await screen.findByRole('button', { name: 'Акт выполненных работ' });
    expect(tile.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(tile);
    await waitFor(() => expect(tile.getAttribute('aria-pressed')).toBe('false'));
  });

  it('creates the full selected set from a source with the one main button', async () => {
    const calls = installTemplateMock(true);
    render(<App />);
    await selectTemplateAndCreateButton();
    const source = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Исходник.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    fireEvent.change(screen.getByTestId('source-file-input'), { target: { files: [source] } });
    await screen.findByText('Исходник.docx');
    fireEvent.click(screen.getByRole('button', { name: 'Создать комплект' }));
    await waitFor(() => expect(calls).toContain('render_docx_batch'));
    expect(await screen.findByRole('heading', { name: 'Создано документов: 1' })).toBeTruthy();
  });

  it('allows an accidentally selected template to be removed before button creation', async () => {
    installTemplateMock(true);
    render(<App />);
    const input = await openTemplateSetup();
    fireEvent.change(input, { target: { files: [templateFile()] } });
    await screen.findByLabelText('Название документа для Акт выполненных работ.docx');
    fireEvent.click(screen.getByRole('button', { name: 'Убрать Акт выполненных работ.docx' }));
    expect(await screen.findByTestId('template-file-input')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Создать кнопки (1)' })).toBeNull();
  });

  it('blocks indistinguishable duplicate button labels', async () => {
    installTemplateMock(true);
    render(<App />);
    const input = await openTemplateSetup();
    fireEvent.change(input, { target: { files: [templateFile('Первый.docx'), templateFile('Второй.docx')] } });
    await screen.findByLabelText('Название документа для Первый.docx');
    await screen.findByLabelText('Название документа для Второй.docx');
    const confirm = screen.getByRole('button', { name: 'Создать кнопки (2)' }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    expect(screen.getByText('Названия кнопок должны отличаться.')).toBeTruthy();
  });
});
