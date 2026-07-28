import { test, expect, type Page } from '@playwright/test';

async function installTauriMock(page: Page, options: { withDocument?: boolean; promptDone?: boolean } = {}) {
  await page.addInitScript(({ withDocument, promptDone }) => {
    if (promptDone) localStorage.setItem('dokkomplekt.created-documents-folder-prompt.v1', 'done');
    const pack = (documents: unknown[]) => ({ pack_id: 'default', name: 'Пакет', documents });
    const invoiceDoc = {
      id: 'template_1', button_label: 'Счёт на оплату', template_path: '/app-data/user-templates/template_1.docx',
      category: 'Accounting', role_id: 'generic', required_fields: [], placeholders: [], is_static_copy: true, popup_fields: [],
    };
    const calls: Array<{ command: string; payload?: unknown }> = [];
    (window as unknown as Record<string, unknown>).__E2E_CALLS__ = calls;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, payload?: unknown) => {
        calls.push({ command, payload });
        switch (command) {
          case 'first_run_state': return withDocument
            ? { pack: pack([invoiceDoc]), has_user_buttons: true, message: 'Набор готов.' }
            : { pack: pack([]), has_user_buttons: false, message: 'Встроенных кнопок нет.' };
          case 'ensure_created_documents_folder': return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false };
          case 'install_background_watcher': return { platform: 'windows', installed: true, watch_folder: 'C:/Users/Test/Desktop/Созданные документы', commands: [], warnings: [] };
          case 'update_background_watcher_preferences': return true;
          case 'get_intake_capabilities': return [];
          case 'import_template_file': return { template_path: invoiceDoc.template_path, extracted_text: 'Счёт на оплату' };
          case 'analyze_template_file': return { document: invoiceDoc };
          case 'prepare_template_setup': return [{ document_id: 'template_1', template_path: invoiceDoc.template_path, detected_title: 'Счёт на оплату', suggested_button_label: 'Счёт на оплату', editable_button_label: 'Счёт на оплату', role_id: 'generic', is_static_copy: true, analysis: { is_static: true }, popup_fields: [] }];
          case 'confirm_template_setup': return pack([invoiceDoc]);
          case 'parse_source_file': return { source_text: 'Исходный документ', source_path: 'C:/app-data/source.docx', source_kind: 'word', semantic_case: { values: {} }, report: { recognized_title: 'Исходный документ', warnings: [] } };
          case 'semantic_extract': return { fields: [], warnings: [], model_applied: false, prompt: '' };
          case 'get_workflow_plan_batch': return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] };
          case 'render_docx_batch': return { output_folder: 'C:/Users/Test/Desktop/Созданные документы/Готово', created_files: ['C:/Users/Test/Desktop/Созданные документы/Готово/Счёт на оплату.docx'], created_documents: [{ document_id: 'template_1', label: 'Счёт на оплату', path: 'C:/Users/Test/Desktop/Созданные документы/Готово/Счёт на оплату.docx' }] };
          default: throw new Error(`e2e mock: unexpected command ${command}`);
        }
      },
    };
  }, { withDocument: options.withDocument ?? false, promptDone: options.promptDone ?? false });
}

test('first interaction offers and creates the desktop work folder', async ({ page }) => {
  await installTauriMock(page);
  await page.goto('/');
  const dialog = page.getByRole('dialog', { name: 'Первичная настройка' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Создать папку' }).click();
  await expect(dialog).toHaveCount(0);
  const commands = await page.evaluate(() => ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map(call => call.command));
  expect(commands).toContain('ensure_created_documents_folder');
  expect(commands).toContain('install_background_watcher');
});

test('ordinary DOCX becomes a button without markup', async ({ page }) => {
  await installTauriMock(page, { promptDone: true });
  await page.goto('/');
  await page.getByRole('button', { name: 'Создать свои кнопки' }).click();
  await page.getByTestId('template-file-input').setInputFiles({ name: 'Счёт на оплату.docx', mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', buffer: Buffer.from([0x50, 0x4b, 0x03, 0x04]) });
  await page.getByRole('button', { name: 'Создать кнопки (1)' }).click();
  await expect(page.getByRole('button', { name: 'Счёт на оплату' })).toHaveAttribute('aria-pressed', 'true');
});

test('one main button creates the selected full pack', async ({ page }) => {
  await installTauriMock(page, { withDocument: true, promptDone: true });
  await page.goto('/');
  await page.getByTestId('source-file-input').setInputFiles({ name: 'Исходник.docx', mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', buffer: Buffer.from([0x50, 0x4b, 0x03, 0x04]) });
  await expect(page.getByText('Исходник.docx', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Создать комплект' }).click();
  await expect(page.getByRole('heading', { name: 'Создано документов: 1' })).toBeVisible();
  const commands = await page.evaluate(() => ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map(call => call.command));
  expect(commands).toContain('get_workflow_plan_batch');
  expect(commands).toContain('render_docx_batch');
});
