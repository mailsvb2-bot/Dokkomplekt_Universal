from pathlib import Path

path = Path('src/App.scenarios.test.tsx')
text = path.read_text(encoding='utf-8')

def replace_once(old: str, new: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'expected one match, found {count}: {old[:80]!r}')
    text = text.replace(old, new, 1)

replace_once(
    "import { afterEach, describe, expect, it, vi } from 'vitest';",
    "import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';",
)
replace_once(
    "import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';",
    "import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';\nimport { OUTPUT_NAMING_CONFIRMED_KEY, OUTPUT_PREFS_KEY } from './lib/appSupport';",
)
replace_once(
    """describe('Полный прогон пользовательских сценариев и тем', () => {
  afterEach(() => { __resetInvokeForTests(); vi.restoreAllMocks(); });

  it('каждый пользовательский сценарий вызывает соответствующую Rust-команду', async () => {""",
    """describe('Полный прогон пользовательских сценариев и тем', () => {
  beforeEach(() => {
    localStorage.clear();
    // These broad scenarios model an established user. A persisted naming
    // preference is itself a donor-compatible confirmation during upgrade.
    localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(['DocumentNumber', 'DocumentDate']));
  });

  afterEach(() => {
    localStorage.clear();
    __resetInvokeForTests();
    vi.restoreAllMocks();
  });

  it('первое создание явно подтверждает нейтральный принцип имени папки результата', async () => {
    localStorage.clear();
    const calls: Call[] = [];
    installMock(calls);
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Счёт № 148' } });
    await click(/Использовать текст/);
    await screen.findByDisplayValue('7701234567');

    await click(/Проверить и создать \\(2\\)/);
    const preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));

    const namingDialog = await screen.findByRole('dialog', { name: 'Подтвердите имя папки результата' });
    expect(calls.some((call) => call.command === 'render_docx_batch')).toBe(false);
    expect(localStorage.getItem(OUTPUT_NAMING_CONFIRMED_KEY)).toBeNull();
    fireEvent.click(within(namingDialog).getByRole('button', { name: 'Использовать этот принцип' }));

    await waitFor(() => expect(calls.some((call) => call.command === 'render_docx_batch')).toBe(true));
    expect(localStorage.getItem(OUTPUT_NAMING_CONFIRMED_KEY)).toBe('true');
    expect(JSON.parse(localStorage.getItem(OUTPUT_PREFS_KEY) || 'null')).toEqual(['DocumentNumber', 'DocumentDate']);
  });

  it('каждый пользовательский сценарий вызывает соответствующую Rust-команду', async () => {""",
)

path.write_text(text, encoding='utf-8')
