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

function installTemplateMock(staticCopy: boolean) {
  const calls: string[] = [];
  __setInvokeForTests(async (name: string) => {
    calls.push(name);
    if (name === 'first_run_state') return { pack: { pack_id: 'default', name: 'Набор', documents: [] }, has_user_buttons: false, message: 'Создайте свои кнопки' } as never;
    if (name === 'get_intake_capabilities') return [] as never;
    if (name === 'import_template_file') return { template_path: 'x.docx', extracted_text: staticCopy ? 'Акт выполненных работ' : 'Акт № {{document.number}}' } as never;
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
      return { pack_id: 'default', name: 'Набор', documents: [{ ...sampleDocument, is_static_copy: staticCopy }] } as never;
    }
    if (name === 'get_workflow_plan') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
    return {} as never;
  });
  return calls;
}

async function selectTemplateAndCreateButton() {
  fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
  const input = screen.getByTestId('template-file-input');
  const file = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Акт выполненных работ.docx', {
    type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  });
  fireEvent.change(input, { target: { files: [file] } });
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
    const calls = installTemplateMock(false);
    render(<App />);
    await selectTemplateAndCreateButton();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
    expect(calls).toContain('confirm_template_setup');
  });

  it('does not publish an unmarked example as a static copy', async () => {
    const calls = installTemplateMock(true);
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: 'Создать свои кнопки' }));
    const input = screen.getByTestId('template-file-input');
    const file = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Акт выполненных работ.docx', {
      type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    });
    fireEvent.change(input, { target: { files: [file] } });
    await screen.findByText('3. Нужна разметка');
    expect((screen.getByRole('button', { name: 'Создать кнопки (1)' }) as HTMLButtonElement).disabled).toBe(true);
    expect(calls).not.toContain('confirm_template_setup');
  });
});
