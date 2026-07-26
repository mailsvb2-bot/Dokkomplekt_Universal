import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { App } from './App';
import { __resetInvokeForTests, __setInvokeForTests } from './lib/api';

const sampleDocument = {
  id: 'template_1',
  button_label: 'Выписной эпикриз',
  template_path: 'x.docx',
  category: 'Medical',
  role_id: 'discharge',
  required_fields: [],
  placeholders: [],
  is_static_copy: false
};

describe('App', () => {
  afterEach(() => __resetInvokeForTests());

  it('starts without built-in document buttons and shows create-buttons path', () => {
    render(<App />);
    expect(screen.getByText('Создать свои кнопки')).toBeTruthy();
    expect(screen.queryByText('Дневники наблюдения')).toBeNull();
  });

  it('creates document button through the Rust-backed API path', async () => {
    __setInvokeForTests(async (name: string) => {
      if (name === 'prepare_template_setup') {
        return [{
          document_id: 'template_1',
          template_path: 'x.docx',
          detected_title: 'Выписной эпикриз',
          suggested_button_label: 'Выписной эпикриз',
          editable_button_label: 'Выписной эпикриз',
          role_id: 'discharge',
          is_static_copy: false,
          analysis: {},
        }] as never;
      }
      if (name === 'confirm_template_setup') {
        return { pack_id: 'default', name: 'Пакет', documents: [sampleDocument] } as never;
      }
      if (name === 'get_workflow_plan') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
      return {} as never;
    });

    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Создать свои кнопки' }));
    expect(screen.getByRole('dialog', { name: 'Настройка шаблона' })).toBeTruthy();
    expect(screen.getByText('Вы выбрали документ')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Создать кнопку из шаблона' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Выписной эпикриз' })).toBeTruthy());
  });
});
