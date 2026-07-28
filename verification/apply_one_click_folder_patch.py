from __future__ import annotations

import importlib.util
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text("utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, "utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    if old not in text:
        raise RuntimeError(f"pattern not found in {path}: {old[:180]!r}")
    write(path, text.replace(old, new, 1))


def regex_once(path: str, pattern: str, replacement: str, flags: int = 0) -> None:
    text = read(path)
    updated, count = re.subn(pattern, replacement, text, count=1, flags=flags)
    if count != 1:
        raise RuntimeError(f"regex matched {count} times in {path}: {pattern[:180]!r}")
    write(path, updated)


def append_once(path: str, marker: str, payload: str) -> None:
    text = read(path)
    if marker in text:
        return
    write(path, text.rstrip() + "\n\n" + payload.strip() + "\n")


# ----- Application mechanics: whole pack selected, one central creation button. -----
replace_once(
    "src/App.tsx",
    "captureWordScanner, closeWordScanner, confirmTemplateSetup, firstRunState,",
    "captureWordScanner, closeWordScanner, confirmTemplateSetup, ensureCreatedDocumentsFolder, firstRunState,",
)
replace_once(
    "src/App.tsx",
    "const PRINT_COPIES_KEY = 'dokkomplekt.print-copies.v1';\n",
    "const PRINT_COPIES_KEY = 'dokkomplekt.print-copies.v1';\nconst WORK_FOLDER_PROMPT_KEY = 'dokkomplekt.created-documents-folder-prompt.v1';\n",
)
replace_once(
    "src/App.tsx",
    "  const [utilityOpen, setUtilityOpen] = useState(false);\n",
    "  const [utilityOpen, setUtilityOpen] = useState(false);\n  const [folderSetupOpen, setFolderSetupOpen] = useState(false);\n",
)
replace_once(
    "src/App.tsx",
    "          setDocuments(res.pack.documents);\n          setSelectedDocIds([]);",
    "          setDocuments(res.pack.documents);\n          setSelectedDocIds(res.pack.documents.map((document) => document.id));",
)
replace_once(
    "src/App.tsx",
    "        } else if (res?.message) {\n          setStatus(res.message);\n        }\n      } catch { /* no backend in browser/tests — start empty */ }",
    "        } else if (res?.message) {\n          setStatus(res.message);\n        }\n        try {\n          if (localStorage.getItem(WORK_FOLDER_PROMPT_KEY) !== 'done') setFolderSetupOpen(true);\n        } catch { /* private mode */ }\n      } catch { /* no backend in browser/tests — start empty */ }",
)
replace_once(
    "src/App.tsx",
    "    setDocuments(pack.documents);\n    setSelectedDocIds([]);\n    setActiveTemplateText(templateText);",
    "    setDocuments(pack.documents);\n    setSelectedDocIds(pack.documents.map((document) => document.id));\n    setActiveTemplateText(templateText);",
)
replace_once(
    "src/App.tsx",
    "    setStatus(`Кнопки созданы: ${confirmedRows.length}. Нажмите нужные кнопки, затем добавьте исходный документ.`);",
    "    setStatus(`Кнопки созданы: ${confirmedRows.length}. Весь комплект уже выбран — добавьте исходный документ.`);",
)
replace_once(
    "src/App.tsx",
    "    if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds([]); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов). Выберите нужные кнопки.`); }",
    "    if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds(res.pack.documents.map((document) => document.id)); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов). Весь комплект выбран.`); }",
)
replace_once(
    "src/App.tsx",
    "  async function installWatcher() {\n",
    """  async function createDefaultWorkFolder() {
    const res = await run('ensure_created_documents_folder', () => ensureCreatedDocumentsFolder());
    if (!res) return;
    setWatchFolder(res.folder);
    setOutputRoot(res.folder);
    setFolderSetupOpen(false);
    try { localStorage.setItem(WORK_FOLDER_PROMPT_KEY, 'done'); } catch { /* private mode */ }
    setStatus(`Рабочая папка готова: ${res.folder}.`);
    const watcher = await run('install_background_watcher', () => installBackgroundWatcher(res.folder, DEFAULT_YEAR, sickLeave, folderParts, autoPrint, printCopies));
    if (watcher) setStatus('Папка «Созданные документы» готова и подключена. Теперь достаточно положить в неё исходный файл.');
  }

  function dismissDefaultWorkFolder() {
    setFolderSetupOpen(false);
    try { localStorage.setItem(WORK_FOLDER_PROMPT_KEY, 'done'); } catch { /* private mode */ }
    setStatus('Папку можно создать позже в разделе автоматической обработки.');
  }

  async function installWatcher() {
""",
)
replace_once(
    "src/App.tsx",
    "            onGenerate={generateDocx}\n          />",
    "            onGenerate={generateDocx}\n            onGenerateSelected={generateSelectedDocuments}\n          />",
)
replace_once(
    "src/App.tsx",
    "      {setupOpen && (\n",
    """      {folderSetupOpen && (
        <div className="backdrop" role="dialog" aria-modal="true" aria-label="Первичная настройка">
          <div className="modal firstRunFolderModal">
            <div className="firstRunFolderIcon"><i className="ti ti-folder-plus" aria-hidden="true" /></div>
            <h2>Создать рабочую папку?</h2>
            <p>На рабочем столе появится папка <strong>«Созданные документы»</strong>.</p>
            <p>Положите туда исходный файл — программа подготовит весь комплект в отдельной подпапке.</p>
            <button className="primaryBtn firstRunFolderCreate" onClick={() => void createDefaultWorkFolder()} disabled={busy}>
              {busy ? 'Создаю папку…' : 'Создать папку'}
            </button>
            <button className="textBtn" onClick={dismissDefaultWorkFolder} disabled={busy}>Не сейчас</button>
          </div>
        </div>
      )}

      {setupOpen && (
""",
)

replace_once(
    "src/components/DocumentRail.tsx",
    "              const selected = selectedCount === 0 || props.selectedDocumentIds.includes(document.id);",
    "              const selected = props.selectedDocumentIds.includes(document.id);",
)

# ----- Thin API and real Rust desktop-folder command. -----
replace_once(
    "src/lib/api.ts",
    "export async function firstRunState(): Promise<FirstRunStateResponse> {\n  return callRust('first_run_state');\n}\n",
    """export async function firstRunState(): Promise<FirstRunStateResponse> {
  return callRust('first_run_state');
}

export interface CreatedDocumentsFolderResponse {
  folder: string;
  created: boolean;
  already_existed: boolean;
}

export async function ensureCreatedDocumentsFolder(): Promise<CreatedDocumentsFolderResponse> {
  return callRust('ensure_created_documents_folder');
}
""",
)
replace_once(
    "src/lib/api.ts",
    "export const rustCommandNames = [\n  'first_run_state',",
    "export const rustCommandNames = [\n  'first_run_state',\n  'ensure_created_documents_folder',",
)

main = read("src-tauri/src/main.rs")
command = '''#[derive(Debug, Serialize)]
struct CreatedDocumentsFolderResponse {
    folder: String,
    created: bool,
    already_existed: bool,
}

#[tauri::command]
fn ensure_created_documents_folder(
    app: tauri::AppHandle,
) -> Result<CreatedDocumentsFolderResponse, String> {
    let desktop = app
        .path()
        .desktop_dir()
        .map_err(|error| format!("Не удалось определить рабочий стол: {error}"))?;
    let folder = desktop.join("Созданные документы");
    let already_existed = folder.is_dir();
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Не удалось создать папку {}: {error}", folder.display()))?;
    Ok(CreatedDocumentsFolderResponse {
        folder: folder.to_string_lossy().into_owned(),
        created: !already_existed,
        already_existed,
    })
}

'''
if "fn ensure_created_documents_folder(" not in main:
    if "fn main() {" not in main:
        raise RuntimeError("main function anchor not found")
    main = main.replace("fn main() {", command + "fn main() {", 1)
write("src-tauri/src/main.rs", main)
replace_once(
    "src-tauri/src/main.rs",
    "            first_run_state,\n            analyze_template,",
    "            first_run_state,\n            ensure_created_documents_folder,\n            analyze_template,",
)

# ----- Source archive: normal root virtualenvs must never be traversed. -----
replace_once(
    "scripts/build_source_archive.py",
    '    ".release-gate",\n',
    '    ".release-gate",\n    ".venv",\n    "venv",\n    "env",\n',
)
write(
    "tests/test_source_archive_virtualenv_exclusion.py",
    '''from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "build_source_archive.py"
SPEC = importlib.util.spec_from_file_location("build_source_archive_virtualenv_test", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_standard_in_tree_virtualenv_names_are_excluded_before_symlink_validation(tmp_path: Path) -> None:
    original_root = MODULE.ROOT
    MODULE.ROOT = tmp_path
    try:
        target = tmp_path / "outside-lib"
        target.mkdir()
        for name in (".venv", "venv", "env"):
            environment = tmp_path / name
            environment.mkdir()
            (environment / "lib64").symlink_to(target, target_is_directory=True)
        assert MODULE.source_files() == []
    finally:
        MODULE.ROOT = original_root
''',
)

# ----- RustSec evidence: publish raw JSON and proof bound to advisory DB Git HEAD. -----
replace_once(
    ".github/workflows/quality-gate.yml",
    "          path: rust-compile-gate.log\n          if-no-files-found: error",
    "          path: |\n            rust-compile-gate.log\n            .cargo-gate/RUSTSEC_AUDIT.json\n            .cargo-gate/RUSTSEC_EVIDENCE.json\n          if-no-files-found: error",
)
replace_once(
    ".github/workflows/quality-gate.yml",
    """      - name: Audit locked Rust dependencies
        shell: bash
        run: |
          set -o pipefail
          cargo audit --deny warnings 2>&1 | tee rustsec-audit.log
      - name: Upload RustSec diagnostics
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: rustsec-audit-diagnostics
          path: rustsec-audit.log
          if-no-files-found: error""",
    """      - name: Audit locked Rust dependencies and bind evidence
        shell: bash
        run: |
          set -euo pipefail
          mkdir -p .cargo-gate
          cargo audit --deny warnings --json > .cargo-gate/RUSTSEC_AUDIT.json
          python3 scripts/write_rustsec_evidence.py
          cargo audit --deny warnings 2>&1 | tee rustsec-audit.log
      - name: Upload RustSec diagnostics
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: rustsec-audit-diagnostics
          path: |
            rustsec-audit.log
            .cargo-gate/RUSTSEC_AUDIT.json
            .cargo-gate/RUSTSEC_EVIDENCE.json
          if-no-files-found: error""",
)

# ----- API contracts. -----
replace_once("src/lib/api.contract.test.ts", "  firstRunState,\n", "  firstRunState,\n  ensureCreatedDocumentsFolder,\n")
replace_once("src/lib/api.contract.test.ts", "  'first_run_state',\n", "  'first_run_state',\n  'ensure_created_documents_folder',\n")
replace_once(
    "src/lib/api.contract.test.ts",
    "      case 'first_run_state':\n      case 'load_state':",
    "      case 'ensure_created_documents_folder':\n        return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false } as never;\n      case 'first_run_state':\n      case 'load_state':",
)
replace_once(
    "src/lib/api.contract.test.ts",
    "    await firstRunState();\n    await resetCase();",
    "    await firstRunState();\n    await ensureCreatedDocumentsFolder();\n    await resetCase();",
)
replace_once(
    "src/lib/api.contract.test.ts",
    "      { command: 'first_run_state', payload: undefined },\n      { command: 'reset_case', payload: undefined },",
    "      { command: 'first_run_state', payload: undefined },\n      { command: 'ensure_created_documents_folder', payload: undefined },\n      { command: 'reset_case', payload: undefined },",
)

# ----- Focused UI tests. -----
replace_once(
    "src/App.test.tsx",
    "import { afterEach, describe, expect, it, vi } from 'vitest';",
    "import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';",
)
replace_once(
    "src/App.test.tsx",
    "    if (name === 'first_run_state') return { pack: { pack_id: 'default', name: 'Набор', documents: [] }, has_user_buttons: false } as never;",
    "    if (name === 'first_run_state') return { pack: { pack_id: 'default', name: 'Набор', documents: [] }, has_user_buttons: false } as never;\n    if (name === 'ensure_created_documents_folder') return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false } as never;\n    if (name === 'install_background_watcher') return { platform: 'windows', installed: true, watch_folder: 'C:/Users/Test/Desktop/Созданные документы', commands: [], warnings: [] } as never;",
)
replace_once(
    "src/App.test.tsx",
    "    if (name === 'get_workflow_plan') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;\n    return {} as never;",
    """    if (name === 'get_workflow_plan' || name === 'get_workflow_plan_batch') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
    if (name === 'parse_source_file') return { source_text: 'Исходный документ', source_path: 'C:/app-data/source.docx', source_kind: 'word', semantic_case: { values: {} }, report: { recognized_title: 'Исходный документ', warnings: [] } } as never;
    if (name === 'semantic_extract') return { fields: [], warnings: [], model_applied: false, prompt: '' } as never;
    if (name === 'render_docx_batch') return { output_folder: 'C:/Users/Test/Desktop/Созданные документы/Готово', created_files: ['C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx'], created_documents: [{ document_id: 'template_1', label: 'Акт выполненных работ', path: 'C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx' }] } as never;
    return {} as never;""",
)
replace_once(
    "src/App.test.tsx",
    "describe('App', () => {\n  afterEach(() => {",
    "describe('App', () => {\n  beforeEach(() => { localStorage.setItem('dokkomplekt.created-documents-folder-prompt.v1', 'done'); });\n  afterEach(() => {",
)
replace_once(
    "src/App.test.tsx",
    "  it('starts without built-in examples and shows only the clear first-run action', async () => {",
    """  it('offers and creates the desktop work folder on a clean profile', async () => {
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

  it('starts without built-in examples and shows only the clear first-run action', async () => {""",
)
replace_once(
    "src/App.test.tsx",
    "  it('keeps document buttons unselected and toggles the whole tile with one click', async () => {",
    "  it('selects newly created document buttons and toggles the whole tile with one click', async () => {",
)
replace_once(
    "src/App.test.tsx",
    "    expect(tile.getAttribute('aria-pressed')).toBe('false');\n    fireEvent.click(tile);\n    await waitFor(() => expect(tile.getAttribute('aria-pressed')).toBe('true'));",
    "    expect(tile.getAttribute('aria-pressed')).toBe('true');\n    fireEvent.click(tile);\n    await waitFor(() => expect(tile.getAttribute('aria-pressed')).toBe('false'));",
)
marker = "  it('allows an accidentally selected template to be removed before button creation', async () => {"
new_test = '''  it('creates the full selected set from a source with the one main button', async () => {
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

'''
text = read("src/App.test.tsx")
if marker not in text:
    raise RuntimeError("App test insertion anchor not found")
write("src/App.test.tsx", text.replace(marker, new_test + marker, 1))

# Broad scenario runs with first-run prompt already accepted; dedicated tests cover the prompt.
replace_once(
    "src/App.scenarios.test.tsx",
    "import { afterEach, describe, expect, it, vi } from 'vitest';",
    "import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "      case 'first_run_state':\n      case 'load_state':",
    "      case 'ensure_created_documents_folder': return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false } as never;\n      case 'first_run_state':\n      case 'load_state':",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "describe('Полный прогон пользовательских сценариев и тем', () => {\n  afterEach(() => { __resetInvokeForTests(); vi.restoreAllMocks(); });",
    "describe('Полный прогон пользовательских сценариев и тем', () => {\n  beforeEach(() => { localStorage.clear(); localStorage.setItem('dokkomplekt.created-documents-folder-prompt.v1', 'done'); });\n  afterEach(() => { localStorage.clear(); __resetInvokeForTests(); vi.restoreAllMocks(); });",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "    expect(mailMergeTile.getAttribute('aria-pressed')).toBe('false');\n    fireEvent.click(mailMergeTile);\n    await waitFor(() => expect(mailMergeTile.getAttribute('aria-pressed')).toBe('true'));",
    "    expect(mailMergeTile.getAttribute('aria-pressed')).toBe('true');",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "const internalOrProfileOnly = new Set(['icd10_suggest',",
    "const internalOrProfileOnly = new Set(['ensure_created_documents_folder', 'icd10_suggest',",
)

# Replace browser E2E with concise contracts for first interaction, buttons, and one-click render.
write(
    "tests/e2e/first_run.spec.ts",
    '''import { test, expect, type Page } from '@playwright/test';

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
  await expect(page.getByText('Исходник.docx')).toBeVisible();
  await page.getByRole('button', { name: 'Создать комплект' }).click();
  await expect(page.getByRole('heading', { name: 'Создано документов: 1' })).toBeVisible();
  const commands = await page.evaluate(() => ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map(call => call.command));
  expect(commands).toContain('get_workflow_plan_batch');
  expect(commands).toContain('render_docx_batch');
});
''',
)

append_once(
    "src/styles.css",
    "/* one-click-first-run-v1 */",
    '''/* one-click-first-run-v1 */
.simpleWorkspace { display: flex; flex-direction: column; gap: 18px; }
.simpleHero { align-items: center; }
.simpleHero h1 { max-width: 760px; font-size: clamp(30px, 4vw, 52px); line-height: 1.05; }
.simpleHero p { max-width: 760px; font-size: 17px; }
.simpleSourceStage { padding: clamp(24px, 4vw, 48px); }
.simpleDropHero { min-height: 360px; justify-content: center; }
.simpleDropHero h2 { margin: 8px 0 0; font-size: clamp(24px, 3vw, 38px); }
.simpleSourceAccepted { margin-bottom: 24px; }
.oneClickPanel { display: grid; justify-items: center; gap: 10px; padding: 24px 0 4px; border-top: 1px solid var(--line); }
.oneClickCreate { width: min(100%, 620px); min-height: 82px; font-size: clamp(22px, 3vw, 32px); font-weight: 800; border-radius: 20px; box-shadow: 0 14px 36px color-mix(in srgb, var(--accent) 26%, transparent); }
.oneClickPanel p { margin: 0; color: var(--muted); text-align: center; }
.simpleOptional { margin-top: 2px; }
.simpleOptional > summary { color: var(--muted); font-weight: 650; }
.simplePackagePanel { align-self: start; }
.simplePackageNotice { display: grid; gap: 5px; padding: 14px 16px; margin-bottom: 12px; border-radius: 14px; background: color-mix(in srgb, var(--accent) 10%, var(--panel)); }
.simplePackageNotice span { color: var(--muted); font-size: 13px; line-height: 1.4; }
.compactDocumentButtons .packageItem { min-height: 68px; }
.firstRunFolderModal { max-width: 520px; text-align: center; padding: 34px; }
.firstRunFolderIcon { width: 76px; height: 76px; display: grid; place-items: center; margin: 0 auto 16px; border-radius: 22px; background: color-mix(in srgb, var(--accent) 18%, var(--panel)); font-size: 38px; color: var(--accent); }
.firstRunFolderModal h2 { margin: 0 0 14px; font-size: 30px; }
.firstRunFolderModal p { margin: 8px 0; color: var(--muted); font-size: 16px; line-height: 1.5; }
.firstRunFolderCreate { width: 100%; min-height: 62px; margin: 22px 0 8px; font-size: 20px; font-weight: 800; }
@media (max-width: 900px) { .simpleHero { align-items: flex-start; } .oneClickCreate { min-height: 70px; } }''',
)

# Remove this excluded transport, then hash the final authored tree (temporary workflow remains until API cleanup).
Path(__file__).unlink()
module_path = ROOT / "scripts" / "build_source_archive.py"
spec = importlib.util.spec_from_file_location("build_source_archive", module_path)
if spec is None or spec.loader is None:
    raise RuntimeError("cannot load build_source_archive")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
(ROOT / module.SOURCE_MANIFEST).write_bytes(module.source_manifest_payload())
print("ONE CLICK + FIRST RUN FOLDER PATCH APPLIED")
