import { test, expect, type Page } from '@playwright/test';

/**
 * Browser E2E прогон против Vite dev-сервера. Настоящего Rust-бэкенда в браузере нет,
 * поэтому перед загрузкой страницы устанавливается мок Tauri-моста
 * (window.__TAURI_INTERNALS__.invoke) с каноническими DTO-ответами — теми же
 * формами, что проверяет контрактный тест TS↔Rust. Это не desktop/Tauri E2E и не проверка IPC/Rust. UI остаётся тонким:
 * тест проверяет провод «кнопка → команда → отрисовка ответа», а бизнес-логика
 * покрыта юнит-тестами Rust.
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
      placeholders: ['document.number', 'org.inn'],
      is_static_copy: false,
    };
    const calls: Array<{ command: string; payload?: unknown }> = [];
    (window as unknown as Record<string, unknown>).__E2E_CALLS__ = calls;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      invoke: async (command: string, payload?: unknown) => {
        calls.push({ command, payload });
        switch (command) {
          case 'first_run_state':
            return { pack: pack([]), has_user_buttons: false, message: 'Встроенных кнопок нет.' };
          case 'import_template_file':
            return { template_path: '/app-data/user-templates/template_1.docx', extracted_text: 'Счёт на оплату № {{document.number}}' };
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
              analysis: {},
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

test('first run does not show built-in medical buttons', async ({ page }) => {
  await installTauriMock(page);
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Создать свои кнопки' })).toBeVisible();
  await expect(page.getByText('Дневники наблюдения')).toHaveCount(0);
});

test('created document button appears after template setup confirmation', async ({ page }) => {
  await installTauriMock(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Создать свои кнопки' }).click();
  await expect(page.getByRole('dialog', { name: 'Настройка шаблона' })).toBeVisible();
  await expect(page.getByText('Вы выбрали документ')).toBeVisible();
  await page.getByRole('button', { name: 'Создать кнопку из шаблона' }).click();
  // Демо-шаблон в диалоге — «Счёт на оплату…»; именно такая кнопка и должна появиться.
  await expect(page.getByRole('button', { name: 'Счёт на оплату' })).toBeVisible();
  // Вставленный текст превратился в реальный DOCX через import_template_file,
  // и кнопка ссылается на файл, который сможет отрендерить render_docx.
  const commands = await page.evaluate(() =>
    ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map((c) => c.command));
  expect(commands).toContain('import_template_file');
  expect(commands).toContain('confirm_template_setup');
});
