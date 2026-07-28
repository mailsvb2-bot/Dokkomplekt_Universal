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
        raise RuntimeError(f"pattern not found in {path}: {old[:160]!r}")
    write(path, text.replace(old, new, 1))


def append_once(path: str, marker: str, payload: str) -> None:
    text = read(path)
    if marker in text:
        return
    write(path, text.rstrip() + "\n\n" + payload.strip() + "\n")


# App: one-click creation, all documents selected by default, first-run folder prompt.
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
    "          setDocuments(res.pack.documents);\n          setSelectedDocIds([]);\n          setStatus(`Рабочий набор готов: ${res.pack.documents.length} документ(ов). Добавьте исходный файл.`);",
    "          setDocuments(res.pack.documents);\n          setSelectedDocIds(res.pack.documents.map((document) => document.id));\n          setStatus(`Рабочий набор готов: ${res.pack.documents.length} документ(ов). Добавьте исходный файл.`);",
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
    "  async function createDefaultWorkFolder() {\n    const res = await run('ensure_created_documents_folder', () => ensureCreatedDocumentsFolder());\n    if (!res) return;\n    setWatchFolder(res.folder);\n    setOutputRoot(res.folder);\n    setFolderSetupOpen(false);\n    try { localStorage.setItem(WORK_FOLDER_PROMPT_KEY, 'done'); } catch { /* private mode */ }\n    setStatus(`Рабочая папка готова: ${res.folder}. Перетащите туда исходный документ или выберите его в программе.`);\n    const watcher = await run('install_background_watcher', () => installBackgroundWatcher(res.folder, DEFAULT_YEAR, sickLeave, folderParts, autoPrint, printCopies));\n    if (watcher) setStatus(`Папка «Созданные документы» готова и подключена. Теперь достаточно положить в неё исходный файл.`);\n  }\n\n  function dismissDefaultWorkFolder() {\n    setFolderSetupOpen(false);\n    try { localStorage.setItem(WORK_FOLDER_PROMPT_KEY, 'done'); } catch { /* private mode */ }\n    setStatus('Папку можно создать позже в настройках автоматической обработки.');\n  }\n\n  async function installWatcher() {\n",
)
replace_once(
    "src/App.tsx",
    "            onGenerate={generateDocx}\n          />",
    "            onGenerate={generateDocx}\n            onGenerateSelected={generateSelectedDocuments}\n          />",
)
replace_once(
    "src/App.tsx",
    "      {setupOpen && (\n",
    "      {folderSetupOpen && (\n        <div className=\"backdrop\" role=\"dialog\" aria-modal=\"true\" aria-label=\"Первичная настройка\">\n          <div className=\"modal firstRunFolderModal\">\n            <div className=\"firstRunFolderIcon\"><i className=\"ti ti-folder-plus\" aria-hidden=\"true\" /></div>\n            <h2>Создать рабочую папку?</h2>\n            <p>На рабочем столе появится папка <strong>«Созданные документы»</strong>.</p>\n            <p>Положите туда исходный файл — программа подготовит весь комплект в отдельной подпапке.</p>\n            <button className=\"primaryBtn firstRunFolderCreate\" onClick={() => void createDefaultWorkFolder()} disabled={busy}>\n              {busy ? 'Создаю папку…' : 'Создать папку'}\n            </button>\n            <button className=\"textBtn\" onClick={dismissDefaultWorkFolder} disabled={busy}>Не сейчас</button>\n          </div>\n        </div>\n      )}\n\n      {setupOpen && (\n",
)

# Document rail: selections are explicit; App initializes the full set.
replace_once(
    "src/components/DocumentRail.tsx",
    "              const selected = selectedCount === 0 || props.selectedDocumentIds.includes(document.id);",
    "              const selected = props.selectedDocumentIds.includes(document.id);",
)

# Thin API and command registry.
replace_once(
    "src/lib/api.ts",
    "export async function firstRunState(): Promise<FirstRunStateResponse> {\n  return callRust('first_run_state');\n}\n",
    "export async function firstRunState(): Promise<FirstRunStateResponse> {\n  return callRust('first_run_state');\n}\n\nexport interface CreatedDocumentsFolderResponse {\n  folder: string;\n  created: boolean;\n  already_existed: boolean;\n}\n\nexport async function ensureCreatedDocumentsFolder(): Promise<CreatedDocumentsFolderResponse> {\n  return callRust('ensure_created_documents_folder');\n}\n",
)
replace_once(
    "src/lib/api.ts",
    "export const rustCommandNames = [\n  'first_run_state',",
    "export const rustCommandNames = [\n  'first_run_state',\n  'ensure_created_documents_folder',",
)

# Rust command: resolve the real desktop directory and create a stable working folder.
main = read("src-tauri/src/main.rs")
anchor = "#[tauri::command]\nfn first_run_state("
if anchor not in main:
    raise RuntimeError("first_run_state anchor not found")
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
write("src-tauri/src/main.rs", main.replace(anchor, command + anchor, 1))
replace_once(
    "src-tauri/src/main.rs",
    "            first_run_state,\n            analyze_template,",
    "            first_run_state,\n            ensure_created_documents_folder,\n            analyze_template,",
)

# Source archive must ignore standard in-tree Python environments, including symlinked lib64.
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

# RustSec artifact: preserve raw audit plus canonical evidence bound to Cargo.lock and advisory DB HEAD.
replace_once(
    ".github/workflows/quality-gate.yml",
    "          path: rust-compile-gate.log\n          if-no-files-found: error",
    "          path: |\n            rust-compile-gate.log\n            .cargo-gate/RUSTSEC_AUDIT.json\n            .cargo-gate/RUSTSEC_EVIDENCE.json\n          if-no-files-found: error",
)
replace_once(
    ".github/workflows/quality-gate.yml",
    "      - name: Audit locked Rust dependencies\n        shell: bash\n        run: |\n          set -o pipefail\n          cargo audit --deny warnings 2>&1 | tee rustsec-audit.log\n      - name: Upload RustSec diagnostics\n        if: always()\n        uses: actions/upload-artifact@v4\n        with:\n          name: rustsec-audit-diagnostics\n          path: rustsec-audit.log\n          if-no-files-found: error",
    "      - name: Audit locked Rust dependencies and bind evidence\n        shell: bash\n        run: |\n          set -euo pipefail\n          mkdir -p .cargo-gate\n          cargo audit --deny warnings --json > .cargo-gate/RUSTSEC_AUDIT.json\n          python3 scripts/write_rustsec_evidence.py\n          cargo audit --deny warnings 2>&1 | tee rustsec-audit.log\n      - name: Upload RustSec diagnostics\n        if: always()\n        uses: actions/upload-artifact@v4\n        with:\n          name: rustsec-audit-diagnostics\n          path: |\n            rustsec-audit.log\n            .cargo-gate/RUSTSEC_AUDIT.json\n            .cargo-gate/RUSTSEC_EVIDENCE.json\n          if-no-files-found: error",
)

# API contract must include the new backend command.
replace_once(
    "src/lib/api.contract.test.ts",
    "  firstRunState,\n",
    "  firstRunState,\n  ensureCreatedDocumentsFolder,\n",
)
replace_once(
    "src/lib/api.contract.test.ts",
    "  'first_run_state',\n",
    "  'first_run_state',\n  'ensure_created_documents_folder',\n",
)
replace_once(
    "src/lib/api.contract.test.ts",
    "      case 'first_run_state':\n      case 'load_state':",
    "      case 'ensure_created_documents_folder':\n        return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false } as never;\n      case 'first_run_state':\n      case 'load_state':",
)
# Ensure the imported wrapper is exercised by the contract suite.
replace_once(
    "src/lib/api.contract.test.ts",
    "    await firstRunState();\n",
    "    await firstRunState();\n    await ensureCreatedDocumentsFolder();\n",
)

# Focused App tests: first interaction folder + real one-button render path.
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
    "    if (name === 'get_workflow_plan' || name === 'get_workflow_plan_batch') return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;\n    if (name === 'parse_source_file') return { source_text: 'Исходный документ', source_path: 'C:/app-data/source.docx', source_kind: 'word', semantic_case: { values: {} }, report: { recognized_title: 'Исходный документ', warnings: [] } } as never;\n    if (name === 'semantic_extract') return { fields: [], warnings: [], model_applied: false, prompt: '' } as never;\n    if (name === 'render_docx_batch') return { output_folder: 'C:/Users/Test/Desktop/Созданные документы/Готово', created_files: ['C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx'], created_documents: [{ document_id: 'template_1', label: 'Акт выполненных работ', path: 'C:/Users/Test/Desktop/Созданные документы/Готово/Акт выполненных работ.docx' }] } as never;\n    return {} as never;",
)
replace_once(
    "src/App.test.tsx",
    "describe('App', () => {\n  afterEach(() => {",
    "describe('App', () => {\n  beforeEach(() => { localStorage.setItem('dokkomplekt.created-documents-folder-prompt.v1', 'done'); });\n  afterEach(() => {",
)
replace_once(
    "src/App.test.tsx",
    "  it('starts without built-in examples and shows only the clear first-run action', async () => {",
    "  it('offers to create the desktop work folder on a clean profile', async () => {\n    localStorage.removeItem('dokkomplekt.created-documents-folder-prompt.v1');\n    const calls = installTemplateMock(false);\n    render(<App />);\n    const dialog = await screen.findByRole('dialog', { name: 'Первичная настройка' });\n    fireEvent.click(screen.getByRole('button', { name: 'Создать папку' }));\n    await waitFor(() => expect(calls).toContain('ensure_created_documents_folder'));\n    await waitFor(() => expect(calls).toContain('install_background_watcher'));\n    expect(dialog).not.toBeInTheDocument;\n  });\n\n  it('starts without built-in examples and shows only the clear first-run action', async () => {",
)
replace_once(
    "src/App.test.tsx",
    "  it('keeps document buttons unselected and toggles the whole tile with one click', async () => {",
    "  it('selects the whole newly created set and toggles a tile with one click', async () => {",
)
replace_once(
    "src/App.test.tsx",
    "    expect(tile.getAttribute('aria-pressed')).toBe('false');\n    fireEvent.click(tile);\n    await waitFor(() => expect(tile.getAttribute('aria-pressed')).toBe('true'));",
    "    expect(tile.getAttribute('aria-pressed')).toBe('true');\n    fireEvent.click(tile);\n    await waitFor(() => expect(tile.getAttribute('aria-pressed')).toBe('false'));",
)
insert_before = "  it('allows an accidentally selected template to be removed before button creation', async () => {"
one_click_test = '''  it('creates the selected documents from a source with the one main button', async () => {
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
if insert_before not in text:
    raise RuntimeError("App test insertion anchor not found")
write("src/App.test.tsx", text.replace(insert_before, one_click_test + insert_before, 1))

# Broad scenario: handle first-run folder and preserve full command reachability.
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
    "describe('Полный прогон пользовательских сценариев и тем', () => {\n  beforeEach(() => localStorage.clear());\n  afterEach(() => { localStorage.clear(); __resetInvokeForTests(); vi.restoreAllMocks(); });",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "    render(<App />);\n\n    // first_run_state populates a profession-neutral document set",
    "    render(<App />);\n    const folderDialog = await screen.findByRole('dialog', { name: 'Первичная настройка' });\n    fireEvent.click(within(folderDialog).getByRole('button', { name: 'Создать папку' }));\n    await waitFor(() => expect(calls.some((c) => c.command === 'ensure_created_documents_folder')).toBe(true));\n\n    // first_run_state populates a profession-neutral document set",
)
replace_once(
    "src/App.scenarios.test.tsx",
    "    expect(mailMergeTile.getAttribute('aria-pressed')).toBe('false');\n    fireEvent.click(mailMergeTile);\n    await waitFor(() => expect(mailMergeTile.getAttribute('aria-pressed')).toBe('true'));",
    "    expect(mailMergeTile.getAttribute('aria-pressed')).toBe('true');",
)

# Browser E2E: first interaction and the source -> one button -> batch result contract.
replace_once(
    "tests/e2e/first_run.spec.ts",
    "async function installTauriMock(page: Page) {\n  await page.addInitScript(() => {",
    "async function installTauriMock(page: Page, withDocument = false) {\n  await page.addInitScript((withDocument) => {",
)
replace_once(
    "tests/e2e/first_run.spec.ts",
    "          case 'first_run_state':\n            return { pack: pack([]), has_user_buttons: false, message: 'Встроенных кнопок нет.' };",
    "          case 'first_run_state':\n            return withDocument\n              ? { pack: pack([invoiceDoc]), has_user_buttons: true, message: 'Набор готов.' }\n              : { pack: pack([]), has_user_buttons: false, message: 'Встроенных кнопок нет.' };\n          case 'ensure_created_documents_folder':\n            return { folder: 'C:/Users/Test/Desktop/Созданные документы', created: true, already_existed: false };\n          case 'install_background_watcher':\n            return { platform: 'windows', installed: true, watch_folder: 'C:/Users/Test/Desktop/Созданные документы', commands: [], warnings: [] };",
)
replace_once(
    "tests/e2e/first_run.spec.ts",
    "          case 'get_workflow_plan':\n            return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] };",
    "          case 'get_workflow_plan':\n          case 'get_workflow_plan_batch':\n            return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] };\n          case 'parse_source_file':\n            return { source_text: 'Исходный документ', source_path: 'C:/app-data/source.docx', source_kind: 'word', semantic_case: { values: {} }, report: { recognized_title: 'Исходный документ', warnings: [] } };\n          case 'semantic_extract':\n            return { fields: [], warnings: [], model_applied: false, prompt: '' };\n          case 'render_docx_batch':\n            return { output_folder: 'C:/Users/Test/Desktop/Созданные документы/Готово', created_files: ['C:/Users/Test/Desktop/Созданные документы/Готово/Счёт на оплату.docx'], created_documents: [{ document_id: 'template_1', label: 'Счёт на оплату', path: 'C:/Users/Test/Desktop/Созданные документы/Готово/Счёт на оплату.docx' }] };",
)
replace_once(
    "tests/e2e/first_run.spec.ts",
    "    };\n  });\n}\n",
    "    };\n  }, withDocument);\n}\n\nasync function createWorkFolder(page: Page) {\n  const dialog = page.getByRole('dialog', { name: 'Первичная настройка' });\n  await expect(dialog).toBeVisible();\n  await dialog.getByRole('button', { name: 'Создать папку' }).click();\n  await expect(dialog).toHaveCount(0);\n}\n",
)
replace_once(
    "tests/e2e/first_run.spec.ts",
    "  await page.goto('/');\n  await expect(page.getByRole('button', { name: 'Создать свои кнопки' })).toBeVisible();",
    "  await page.goto('/');\n  await createWorkFolder(page);\n  await expect(page.getByRole('button', { name: 'Создать свои кнопки' })).toBeVisible();",
)
# Second existing test has the same goto but not the create-buttons expectation sequence.
replace_once(
    "tests/e2e/first_run.spec.ts",
    "  await page.goto('/');\n  await page.getByRole('button', { name: 'Создать свои кнопки' }).click();",
    "  await page.goto('/');\n  await createWorkFolder(page);\n  await page.getByRole('button', { name: 'Создать свои кнопки' }).click();",
)
append_once(
    "tests/e2e/first_run.spec.ts",
    "one button creates the full selected batch",
    '''test('one button creates the full selected batch', async ({ page }) => {
  await installTauriMock(page, true);
  await page.goto('/');
  await createWorkFolder(page);
  await page.getByTestId('source-file-input').setInputFiles({
    name: 'Исходник.docx',
    mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    buffer: Buffer.from([0x50, 0x4b, 0x03, 0x04]),
  });
  await expect(page.getByText('Исходник.docx')).toBeVisible();
  await page.getByRole('button', { name: 'Создать комплект' }).click();
  await expect(page.getByRole('heading', { name: 'Создано документов: 1' })).toBeVisible();
  const commands = await page.evaluate(() =>
    ((window as unknown as Record<string, unknown>).__E2E_CALLS__ as Array<{ command: string }>).map((call) => call.command));
  expect(commands).toContain('get_workflow_plan_batch');
  expect(commands).toContain('render_docx_batch');
});''',
)

# Simple visual hierarchy: source + one large action; everything else stays collapsed.
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

# Remove the temporary workflow transport and this script before generating provenance.
workflow_path = ROOT / ".github" / "workflows" / "source-provenance.yml"
workflow = workflow_path.read_text("utf-8")
workflow = re.sub(
    r"\n      # BEGIN ONE CLICK PATCH\n.*?      # END ONE CLICK PATCH\n",
    "\n",
    workflow,
    flags=re.S,
)
workflow_path.write_text(workflow, "utf-8")
Path(__file__).unlink()

# Regenerate the checked-in source manifest from the final clean tree.
module_path = ROOT / "scripts" / "build_source_archive.py"
spec = importlib.util.spec_from_file_location("build_source_archive", module_path)
if spec is None or spec.loader is None:
    raise RuntimeError("cannot load build_source_archive")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
(ROOT / module.SOURCE_MANIFEST).write_bytes(module.source_manifest_payload())

print("ONE CLICK + FIRST RUN FOLDER PATCH APPLIED")
