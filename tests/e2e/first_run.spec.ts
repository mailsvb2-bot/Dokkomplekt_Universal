import { test, expect, type Page } from '@playwright/test';

/**
 * Browser E2E прогон против Vite dev-сервера. Настоящего Rust-бэкенда в браузере нет,
 * поэтому перед загрузкой страницы устанавливается мок Tauri-моста с каноническими DTO.
 */
async function installTauriMock(page: Page) {
  await page.addInitScript(() => {
    const pack = (documents: unknown[]) => ({ pack_id: 'default', name: 'Пакет', documents });
    const invoiceDoc = {
      id: 'template_1',
      button_label: 'Счёт на оплату',
      template_path: '/app-data/user-templates/template_1.docx',
      category: 'Accounting',
      role_id: 'generic',
      required_fields: [],
      placeholders: ['document.number'],
      is_static_copy: false,
      popup_fields: [],
    };
    const calls: Array<{ command: string; payload?: unknown }> = [];
    (window as unknown as Record<string, unknown>).__E2E_CALLS__ = calls;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, payload?: unknown) => {
        calls.push({ command, payload });
        switch (command) {
          case 'first_run_state':
            return { pack: pack([]), has_user_buttons: false, message: 'Встроенных кнопок нет.' };
          case 'get_intake_capabilities':
            return [];
          case 'update_background_watcher_preferences':
            return true;
          case 'pick_folder':
            return { selected_path: '/tmp/dokkomplekt-e2e-output' };
          case 'ensure_output_root':
            return '/tmp/dokkomplekt-e2e-output';
          case 'pick_template_files':
            return { files: [{ file_name: 'Счёт на оплату.docx', template_path: '/app-data/user-templates/template_1.docx', extracted_text: 'Счёт на оплату № {{document.number}}' }] };
          case 'import_template_file':
            return { template_path: '/app-data/user-templates/template_1.docx', extracted_text: 'Счёт на оплату № {{document.number}}' };
          case 'analyze_template_file':
            return { document: { ...invoiceDoc, popup_fields: [] } };
          case 'prepare_template_setup': {
            const req = (payload as { req?: { candidates?: Array<{ document_id: string; template_path: string }> } })?.req;
            const candidate = req?.candidates?.[0];
            return [{
              document_id: candidate?.document_id ?? 'template_1',
              template_path: candidate?.template_path ?? invoiceDoc.template_path,
              detected_title: 'Счёт на оплату',
              suggested_button_label: 'Счёт на оплату',
              editable_button_label: 'Счёт на оплату',
              role_id: 'generic',
              is_static_copy: false,
              analysis: { is_static: false },
              popup_fields: [],
            }];
          }
          case 'confirm_template_setup':
            return pack([invoiceDoc]);
          case 'get_workflow_plan':
            return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] };
          default:
            throw new Error(`e2e mock: unexpected command ${command}`);
        }
      },
    };
  });
}

async function completeFolderNamingOnboarding(page: Page) {
  const dialog = page.getByRole('dialog', { name: 'Как называть папку комплекта?' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Выбрать папку на компьютере' }).click();
  await expect(dialog.getByTestId('output-root-choice')).toContainText('/tmp/dokkomplekt-e2e-output');
  await dialog.getByRole('button', { name: /Человек \+ месяц/ }).click();
  await dialog.getByRole('button', { name: 'Сохранить папку и правило' }).click();
  await expect(dialog).toBeHidden();
}

test('first run saves a naming rule before showing the create-buttons action', async ({ page }) => {
  await installTauriMock(page);
  await page.goto('/');
  await completeFolderNamingOnboarding(page);
  await expect(page.getByRole('button', { name: 'Создать свои кнопки' })).toBeVisible();
  await expect(page.getByText('Встроенный пример')).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => localStorage.getItem('dokkomplekt.output-root.v1'))).toBe('/tmp/dokkomplekt-e2e-output');
  await expect.poll(() => page.evaluate(() => localStorage.getItem('dokkomplekt.output-folder-naming-confirmed.v1'))).toBe('true');
  await expect.poll(() => page.evaluate(() => localStorage.getItem('dokkomplekt.output-folder-parts.v1'))).toBe(JSON.stringify(['ShortInitials', 'PeriodStartMonthName']));
});

test('marked DOCX becomes a button without copying example facts', async ({ page }) => {
  await installTauriMock(page);
  await page.goto('/');
  await completeFolderNamingOnboarding(page);
  await page.getByRole('button', { name: 'Создать свои кнопки' }).click();
  await expect(page.getByRole('dialog', { name: 'Добавление шаблонов' })).toBeVisible();
  await expect(page.getByLabel('Название документа для Счёт на оплату.docx')).toHaveValue('Счёт на оплату');
  await expect(page.getByText('Кнопки готовы к созданию')).toBeVisible();
  await page.getByRole('button', { name: 'Создать кнопки (1)' }).click();
  await expect(page.getByRole('button', { name: 'Счёт на оплату' })).toBeVisible();

  const commands = await page.evaluate(() =>
    ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map((c) => c.command));
  expect(commands).toContain('pick_folder');
  expect(commands).toContain('ensure_output_root');
  expect(commands).toContain('pick_template_files');
  expect(commands).toContain('analyze_template_file');
  expect(commands).toContain('confirm_template_setup');
});