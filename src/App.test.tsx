import { afterEach, describe, expect, it } from 'vitest';
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
  is_static_copy: false,
};

describe('App', () => {
  afterEach(() => __resetInvokeForTests());

  it('starts without built-in examples and shows the template setup path', () => {
    render(<App />);
    expect(screen.getByRole('button', { name: 'Добавить шаблоны' })).toBeTruthy();
    expect(screen.queryByText('Встроенный пример')).toBeNull();
  });

  it('adds a document through the Rust-backed setup path', async () => {
    __setInvokeForTests(async (name: string) => {
      if (name === 'prepare_template_setup') {
        return [{
          document_id: 'template_1',
          template_path: 'x.docx',
          detected_title: 'Акт выполненных работ',
          suggested_button_label: 'Акт выполненных работ',
          editable_button_label: 'Акт выполненных работ',
          role_id: 'generic',
          is_static_copy: false,
          analysis: {},
        }] as never;
      }
      if (name === 'confirm_template_setup') {
        return { pack_id: 'default', name: 'Набор', documents: [sampleDocument] } as never;
      }
      if (name === 'get_workflow_plan') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
      return {} as never;
    });

    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Добавить шаблоны' }));
    expect(screen.getByRole('dialog', { name: 'Добавление шаблонов' })).toBeTruthy();
    expect(screen.getByText('Выбранный документ')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Добавить документ' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Акт выполненных работ' })).toBeTruthy());
  });
});
