#!/usr/bin/env python3
from __future__ import annotations

import ast
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    target = ROOT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8", newline="\n")


def replace(path: str, old: str, new: str, *, count: int = 1) -> None:
    content = read(path)
    actual = content.count(old)
    if actual != count:
        raise RuntimeError(f"{path}: expected {count} occurrence(s), found {actual}: {old[:120]!r}")
    write(path, content.replace(old, new))


# 1. Trial: one complete kit may use the full remaining monthly allowance.
replace(
    "src-tauri/src/main.rs",
    "const TRIAL_MAX_DOCUMENTS_PER_RUN: u32 = 3;",
    "const TRIAL_MAX_DOCUMENTS_PER_RUN: u32 = TRIAL_DOCUMENT_LIMIT_MONTH;",
)
replace(
    "src-tauri/src/main.rs",
    '''        return Err(format!(
            "Генерация заблокирована лицензией: {} (план {}, использовано {}/{})",
            decision.reason,
            decision.plan,
            decision.documents_used_month,
            decision.document_limit_month
        ));''',
    '''        return Err(format!(
            "Генерация заблокирована лицензией: {} (план {}, запрошено {}, лимит за запуск {}, использовано за месяц {}/{}, осталось {})",
            decision.reason,
            decision.plan,
            requested_documents,
            decision.max_documents_per_run,
            decision.documents_used_month,
            decision.document_limit_month,
            decision.documents_left_month
        ));''',
)

# 2. UI contract validation and a real React error boundary.
write(
    "src/lib/runtimeValidation.ts",
    r'''import type { CreatedDocumentsIntakeResult } from './types';

export class BackendContractError extends Error {
  readonly command: string;

  constructor(command: string, detail: string) {
    super(`Некорректный ответ внутреннего модуля «${command}»: ${detail}`);
    this.name = 'BackendContractError';
    this.command = command;
  }
}

function record(command: string, value: unknown, label = 'ответ'): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new BackendContractError(command, `${label} должен быть объектом`);
  }
  return value as Record<string, unknown>;
}

function array(command: string, value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new BackendContractError(command, `${label} должен быть массивом`);
  }
  return value;
}

function string(command: string, value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new BackendContractError(command, `${label} должен быть строкой`);
  }
  return value;
}

function boolean(command: string, value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') {
    throw new BackendContractError(command, `${label} должен быть логическим значением`);
  }
  return value;
}

function number(command: string, value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new BackendContractError(command, `${label} должен быть конечным числом`);
  }
  return value;
}

function stringArray(command: string, value: unknown, label: string): string[] {
  const items = array(command, value, label);
  if (!items.every((item) => typeof item === 'string')) {
    throw new BackendContractError(command, `${label} должен содержать только строки`);
  }
  return items as string[];
}

function validateDocument(command: string, value: unknown): void {
  const item = record(command, value, 'документ');
  string(command, item.id, 'document.id');
  string(command, item.button_label, 'document.button_label');
  string(command, item.template_path, 'document.template_path');
  array(command, item.required_fields, 'document.required_fields');
  array(command, item.placeholders, 'document.placeholders');
}

function validateFirstRun(command: string, value: unknown): void {
  const root = record(command, value);
  const pack = record(command, root.pack, 'pack');
  const documents = array(command, pack.documents, 'pack.documents');
  documents.forEach((item) => validateDocument(command, item));
  boolean(command, root.has_user_buttons, 'has_user_buttons');
  string(command, root.message, 'message');
}

function validateWorkflow(command: string, value: unknown): void {
  const root = record(command, value);
  array(command, root.prompts, 'prompts');
  boolean(command, root.blocked, 'blocked');
  stringArray(command, root.block_reasons, 'block_reasons');
}

function validateBatch(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.output_folder, 'output_folder');
  stringArray(command, root.created_files, 'created_files');
  if (root.created_documents !== undefined) array(command, root.created_documents, 'created_documents');
}

function validateSemantic(command: string, value: unknown): void {
  const root = record(command, value);
  array(command, root.fields, 'fields');
  stringArray(command, root.warnings, 'warnings');
  boolean(command, root.model_applied, 'model_applied');
  string(command, root.prompt, 'prompt');
}

export function normalizeCreatedDocumentsIntakeResult(
  value: unknown,
  command = 'document-batch-ready',
): CreatedDocumentsIntakeResult {
  const root = record(command, value);
  const status = string(command, root.status, 'status');
  if (!['processed', 'attention', 'setup_needed', 'ignored'].includes(status)) {
    throw new BackendContractError(command, `неизвестный status: ${status}`);
  }
  const patientFolder = root.patient_folder;
  if (patientFolder !== null && typeof patientFolder !== 'string') {
    throw new BackendContractError(command, 'patient_folder должен быть строкой или null');
  }
  const attentionFile = root.attention_file;
  if (attentionFile !== null && typeof attentionFile !== 'string') {
    throw new BackendContractError(command, 'attention_file должен быть строкой или null');
  }
  const createdFiles = stringArray(command, root.created_files, 'created_files');
  const missing = stringArray(command, root.missing, 'missing');
  const message = string(command, root.message, 'message');
  if (root.created_documents !== undefined) array(command, root.created_documents, 'created_documents');
  return {
    ...(root as unknown as CreatedDocumentsIntakeResult),
    status: status as CreatedDocumentsIntakeResult['status'],
    patient_folder: patientFolder,
    attention_file: attentionFile,
    created_files: createdFiles,
    missing,
    message,
  };
}

const ARRAY_COMMANDS = new Set([
  'get_intake_capabilities',
  'get_sidecar_status',
  'get_component_statuses',
  'refresh_component_catalog',
  'prepare_template_setup',
  'list_learned_scanner_rules',
  'get_diary_plan',
  'get_record_series_plan',
  'icd10_suggest',
  'list_organization_knowledge',
  'get_calibrated_threshold_status',
]);

const NULLABLE_COMMANDS = new Set([
  'save_state',
  'check_template_regression',
]);

export function validateRustResponse<T>(command: string, value: unknown): T {
  if (value === null || value === undefined) {
    if (NULLABLE_COMMANDS.has(command)) return value as T;
    throw new BackendContractError(command, 'получено пустое значение');
  }
  if (ARRAY_COMMANDS.has(command)) {
    array(command, value, 'ответ');
    return value as T;
  }
  switch (command) {
    case 'first_run_state':
    case 'load_state':
      validateFirstRun(command, value);
      break;
    case 'get_workflow_plan':
    case 'get_workflow_plan_batch':
      validateWorkflow(command, value);
      break;
    case 'render_docx_batch':
      validateBatch(command, value);
      break;
    case 'run_created_documents_intake':
      normalizeCreatedDocumentsIntakeResult(value, command);
      break;
    case 'semantic_extract':
      validateSemantic(command, value);
      break;
    case 'validate_product_access': {
      const root = record(command, value);
      boolean(command, root.accepted, 'accepted');
      string(command, root.plan, 'plan');
      number(command, root.document_limit_month, 'document_limit_month');
      number(command, root.max_documents_per_run, 'max_documents_per_run');
      break;
    }
    default:
      break;
  }
  return value as T;
}
''',
)
replace(
    "src/lib/api.ts",
    "import { invoke as tauriInvoke } from '@tauri-apps/api/core';",
    "import { invoke as tauriInvoke } from '@tauri-apps/api/core';\nimport { validateRustResponse } from './runtimeValidation';",
)
replace(
    "src/lib/api.ts",
    '''async function callRust<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  return invokeFn<T>(command, payload);
}''',
    '''async function callRust<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  const value = await invokeFn<unknown>(command, payload);
  return validateRustResponse<T>(command, value);
}''',
)

write(
    "src/components/AppErrorBoundary.tsx",
    r'''import React from 'react';

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends React.Component<React.PropsWithChildren, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    console.error('Dokkomplekt UI render failure', error, info);
  }

  render(): React.ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <main className="fatalError" role="alert">
        <h1>Интерфейс восстановлен после ошибки</h1>
        <p>Документы и настройки не удалены. Перезапустите только окно программы.</p>
        <details>
          <summary>Техническая информация</summary>
          <pre>{this.state.error.message}</pre>
        </details>
        <button type="button" onClick={() => globalThis.location.reload()}>
          Перезапустить интерфейс
        </button>
      </main>
    );
  }
}
''',
)
replace(
    "src/main.tsx",
    "import { App } from './App';",
    "import { App } from './App';\nimport { AppErrorBoundary } from './components/AppErrorBoundary';",
)
replace(
    "src/main.tsx",
    '''  <React.StrictMode>
    <App />
  </React.StrictMode>''',
    '''  <React.StrictMode>
    <AppErrorBoundary>
      <App />
    </AppErrorBoundary>
  </React.StrictMode>''',
)

write(
    "src/hooks/useActionRunner.ts",
    r'''import { useCallback, useState } from 'react';

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return 'Неизвестная ошибка';
  }
}

export function useActionRunner(onStatus: (message: string) => void) {
  const [busy, setBusy] = useState(false);

  const run = useCallback(async <T,>(_label: string, action: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    try {
      return await action();
    } catch (error) {
      onStatus(`Не удалось выполнить действие: ${message(error)}`);
      return undefined;
    } finally {
      setBusy(false);
    }
  }, [onStatus]);

  return { busy, run };
}
''',
)
replace(
    "src/App.tsx",
    "import { applyTheme, buildTheme, loadTheme, saveTheme, type ThemeState } from './theme';",
    "import { applyTheme, buildTheme, loadTheme, saveTheme, type ThemeState } from './theme';\nimport { useActionRunner } from './hooks/useActionRunner';\nimport { normalizeCreatedDocumentsIntakeResult } from './lib/runtimeValidation';",
)
replace(
    "src/App.tsx",
    "  const [busy, setBusy] = useState(false);",
    "  const { busy, run } = useActionRunner(setStatus);",
)
replace(
    "src/App.tsx",
    '''  async function run<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    try {
      return await fn();
    } catch (err) {
      setStatus(`Не удалось выполнить действие: ${errorMessage(err)}`);
      return undefined;
    } finally {
      setBusy(false);
    }
  }

''',
    "",
)
replace(
    "src/App.tsx",
    '''    listen<CreatedDocumentsIntakeResult>('document-batch-ready', (event) => {
      const result = event.payload;
      setIntakeResult(result);
      setStatus(result.message);
      if (result.status === 'processed' && result.created_files.length) {
        setLastOutput({
          folder: result.patient_folder,
          files: result.created_files,
          source: 'watcher',
          print_items: createdPrintItems(result.created_documents, result.created_files, documents),
        });
      }
    }).then((unlisten) => {''',
    '''    listen<unknown>('document-batch-ready', (event) => {
      try {
        const result = normalizeCreatedDocumentsIntakeResult(event.payload);
        setIntakeResult(result);
        setStatus(result.message);
        if (result.status === 'processed' && result.created_files.length) {
          setLastOutput({
            folder: result.patient_folder,
            files: result.created_files,
            source: 'watcher',
            print_items: createdPrintItems(result.created_documents, result.created_files, documents),
          });
        }
      } catch (error) {
        setStatus(`Фоновая обработка вернула некорректный результат: ${errorMessage(error)}`);
      }
    }).then((unlisten) => {''',
)

# 3. Archive hygiene and ASCII launcher names.
gitignore = read(".gitignore")
for entry in ["/.venv/", "/venv/", "/.tox/", "/.nox/", "/coverage/", "/.coverage"]:
    if entry not in gitignore.splitlines():
        gitignore += entry + "\n"
write(".gitignore", gitignore)

replace(
    "scripts/build_source_archive.py",
    '''    ".ruff_cache",
    "playwright-report",''',
    '''    ".ruff_cache",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".nox",
    "coverage",
    "playwright-report",''',
)

renames = {
    "ПРОВЕРИТЬ_ПРОЕКТ.bat": "CHECK_PROJECT.bat",
    "СОБРАТЬ_EXE.bat": "BUILD_EXE.bat",
}
for source, destination in renames.items():
    source_path = ROOT / source
    if source_path.exists():
        source_path.rename(ROOT / destination)

for path in ROOT.rglob("*"):
    if not path.is_file() or ".git" in path.parts:
        continue
    if path.name == "SOURCE_MANIFEST_SHA256.txt":
        continue
    try:
        content = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        continue
    updated = content
    for source, destination in renames.items():
        updated = updated.replace(source, destination)
    if updated != content:
        path.write_text(updated, encoding="utf-8", newline="\n")

# 4. Thin/offline packaging are now intentionally different.
tauri = json.loads(read("src-tauri/tauri.conf.json"))
tauri["bundle"]["windows"]["webviewInstallMode"] = {"type": "downloadBootstrapper", "silent": True}
write("src-tauri/tauri.conf.json", json.dumps(tauri, ensure_ascii=False, indent=2) + "\n")
write(
    "src-tauri/tauri.thin.conf.json",
    json.dumps(
        {
            "bundle": {
                "resources": [],
                "windows": {"webviewInstallMode": {"type": "downloadBootstrapper", "silent": True}},
            }
        },
        ensure_ascii=False,
        indent=2,
    )
    + "\n",
)
write(
    "src-tauri/tauri.offline.conf.json",
    json.dumps(
        {
            "bundle": {
                "resources": ["resources/tools/**"],
                "windows": {"webviewInstallMode": {"type": "offlineInstaller"}},
            }
        },
        ensure_ascii=False,
        indent=2,
    )
    + "\n",
)

write(
    "BUILD_WINDOWS_INSTALLER.bat",
    r'''@echo off
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

if "%DOKKOMPLEKT_SIDECAR_MANIFEST%"=="" (
  echo ERROR: DOKKOMPLEKT_SIDECAR_MANIFEST is not set.
  echo The offline installer requires a reviewed manifest for OCR and Office sidecars.
  exit /b 1
)
if "%DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64 is required.
  exit /b 1
)
if "%DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD%"=="" (
  echo ERROR: DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD is required.
  exit /b 1
)

where node >nul 2>nul || (echo ERROR: Node.js is missing. & exit /b 2)
where npm >nul 2>nul || (echo ERROR: npm is missing. & exit /b 2)
where cargo >nul 2>nul || (echo ERROR: Rust/Cargo is missing. & exit /b 2)
where powershell >nul 2>nul || (echo ERROR: PowerShell is missing. & exit /b 2)

call scripts\ensure_python_env.bat || exit /b 1
call npm ci || exit /b 1
.venv\Scripts\python.exe scripts\prepare_sidecars.py "%DOKKOMPLEKT_SIDECAR_MANIFEST%" --clean || exit /b 1
.venv\Scripts\python.exe scripts\assert_offline_runtime_ready.py --target windows-x86_64 --require-semantic-model --require-supply-chain --production || exit /b 1
.venv\Scripts\python.exe scripts\probe_offline_runtime.py --target windows-x86_64 || exit /b 1
.venv\Scripts\python.exe scripts\run_python_contracts_sharded.py --report verification\installer\python-contracts.json || exit /b 1
call scripts\prepackage_rust_gate.bat || exit /b 1
.venv\Scripts\python.exe scripts\assert_release_ready.py || exit /b 1
call npm run typecheck || exit /b 1
call npm run test || exit /b 1
call npm run build || exit /b 1

call npx tauri build --no-bundle || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign_windows_release.ps1 -ArtifactRoot target\release\dokkomplekt-tauri.exe || exit /b 1
call npx tauri bundle --bundles nsis --config src-tauri\tauri.offline.conf.json || exit /b 1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign_windows_release.ps1 -ArtifactRoot target\release\bundle\nsis || exit /b 1
set "DOKKOMPLEKT_REQUIRE_AUTHENTICODE=1"
powershell -NoProfile -ExecutionPolicy Bypass -File tests\installer\windows_installer_contract.ps1 || exit /b 1

echo SIGNED OFFLINE INSTALLER CREATED AND VERIFIED.
''',
)

workflow_path = ".github/workflows/build-installers.yml"
workflow = read(workflow_path)
workflow = workflow.replace("    types: [created]", "    types: [published]")
workflow = workflow.replace("permissions:\n  contents: read", "permissions:\n  contents: write")
if "publish-release-assets:" not in workflow:
    workflow += r'''

  publish-release-assets:
    name: Publish only verified signed release assets
    if: github.event_name == 'release' && github.event.action == 'published'
    needs: [windows-hardware-e2e, linux-bundles]
    runs-on: ubuntu-24.04
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: Dokkomplekt-Windows-NSIS-Signed-Offline
          path: release/windows
      - uses: actions/download-artifact@v4
        with:
          name: Dokkomplekt-Windows-Hardware-E2E-Evidence
          path: release/evidence
      - uses: actions/download-artifact@v4
        with:
          name: Dokkomplekt-Linux-AppImage-deb-rpm
          path: release/linux
      - name: Create complete release checksum register
        shell: bash
        run: |
          cd release
          find . -type f ! -name SHA256SUMS.txt -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS.txt
          test -s SHA256SUMS.txt
      - name: Attach artifacts only after signing and hardware E2E
        env:
          GH_TOKEN: ${{ github.token }}
          TAG: ${{ github.event.release.tag_name }}
        shell: bash
        run: |
          mapfile -d '' files < <(find release -type f -print0)
          gh release upload "$TAG" "${files[@]}" --clobber
'''
write(workflow_path, workflow)

# 5. Tests for the exact regressions and blind spots.
write(
    "src/lib/runtimeValidation.test.ts",
    r'''import { describe, expect, it } from 'vitest';
import { BackendContractError, normalizeCreatedDocumentsIntakeResult, validateRustResponse } from './runtimeValidation';

describe('runtime backend contracts', () => {
  it('rejects null where the UI expects an array', () => {
    expect(() => validateRustResponse('get_intake_capabilities', null)).toThrow(BackendContractError);
  });

  it('rejects a malformed workflow before React reads .length', () => {
    expect(() => validateRustResponse('get_workflow_plan', {
      document_id: 'x',
      prompts: null,
      blocked: false,
      block_reasons: [],
    })).toThrow(/prompts/);
  });

  it('normalizes a valid watcher payload and rejects malformed created_files', () => {
    expect(normalizeCreatedDocumentsIntakeResult({
      status: 'processed',
      patient_folder: 'out',
      created_files: ['a.docx'],
      missing: [],
      attention_file: null,
      message: 'ok',
    }).created_files).toEqual(['a.docx']);
    expect(() => normalizeCreatedDocumentsIntakeResult({
      status: 'processed',
      patient_folder: 'out',
      created_files: null,
      missing: [],
      attention_file: null,
      message: 'bad',
    })).toThrow(/created_files/);
  });
});
''',
)
write(
    "src/components/AppErrorBoundary.test.tsx",
    r'''import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AppErrorBoundary } from './AppErrorBoundary';

function Broken(): never {
  throw new Error('render exploded');
}

describe('AppErrorBoundary', () => {
  it('shows a recoverable screen instead of a white window', () => {
    vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<AppErrorBoundary><Broken /></AppErrorBoundary>);
    expect(screen.getByRole('alert')).toHaveTextContent('Интерфейс восстановлен после ошибки');
    expect(screen.getByText('render exploded')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Перезапустить интерфейс' })).toBeInTheDocument();
  });
});
''',
)
write(
    "src/components/PopupFieldEditor.test.tsx",
    r'''import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PopupFieldEditor } from './PopupFieldEditor';

describe('PopupFieldEditor', () => {
  it('adds a safe question and reports edits', () => {
    const onChange = vi.fn();
    const { rerender } = render(<PopupFieldEditor fields={[]} onChange={onChange} />);
    fireEvent.click(screen.getByRole('button', { name: '+ Добавить вопрос' }));
    expect(onChange).toHaveBeenCalledOnce();
    const fields = onChange.mock.calls[0][0];
    rerender(<PopupFieldEditor fields={fields} onChange={onChange} />);
    fireEvent.change(screen.getByLabelText('Текст вопроса 1'), { target: { value: 'Номер дела' } });
    expect(onChange).toHaveBeenCalledTimes(2);
  });
});
''',
)
write(
    "src/components/TemplateSetupModal.test.tsx",
    r'''import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TemplateSetupModal } from './TemplateSetupModal';

const base = {
  templateText: '',
  buttonLabel: '',
  previewTitle: 'Документ',
  pendingTemplates: [],
  draftPopupFields: [],
  onTemplateTextChange: vi.fn(),
  onButtonLabelChange: vi.fn(),
  onDraftPopupFieldsChange: vi.fn(),
  onPendingTemplateLabelChange: vi.fn(),
  onPendingPopupFieldsChange: vi.fn(),
  onMarkupPendingTemplate: vi.fn(async () => undefined),
  onStartGuidedPendingScanner: vi.fn(),
  onAnalyze: vi.fn(),
  onPickFile: vi.fn(),
  onDropFiles: vi.fn(),
  onCancel: vi.fn(),
  onConfirm: vi.fn(),
};

describe('TemplateSetupModal', () => {
  it('keeps the first step simple and disables confirmation without input', () => {
    render(<TemplateSetupModal {...base} />);
    expect(screen.getByText('Выберите шаблоны')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Создать кнопку' })).toBeDisabled();
  });

  it('creates every prepared template as a button', () => {
    const onConfirm = vi.fn();
    render(<TemplateSetupModal {...base} onConfirm={onConfirm} pendingTemplates={[{
      document_id: 'd1',
      file_name: 'Акт.docx',
      button_label: 'Акт',
      extracted_text: 'Акт',
      popup_fields: [],
    }]} />);
    fireEvent.click(screen.getByRole('button', { name: 'Создать кнопки (1)' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
''',
)
write(
    "src/components/OrganizationKnowledgePanel.test.tsx",
    r'''import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { __resetInvokeForTests, __setInvokeForTests } from '../lib/api';
import { OrganizationKnowledgePanel } from './OrganizationKnowledgePanel';

describe('OrganizationKnowledgePanel', () => {
  afterEach(() => __resetInvokeForTests());

  it('survives malformed list responses and validates field lines before saving', async () => {
    __setInvokeForTests(async (command) => {
      if (command === 'list_organization_knowledge') return null as never;
      return [] as never;
    });
    const onStatus = vi.fn();
    render(<OrganizationKnowledgePanel onStatus={onStatus} />);
    await screen.findByText('В этой категории записей пока нет.');
    fireEvent.change(screen.getByPlaceholderText('org.main'), { target: { value: 'org.main' } });
    fireEvent.change(screen.getByPlaceholderText('Основная организация'), { target: { value: 'Организация' } });
    fireEvent.change(screen.getByPlaceholderText('org.name=ООО Ромашка\norg.inn=7707083893'), { target: { value: 'сломанная строка' } });
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить запись' }));
    await waitFor(() => expect(onStatus).toHaveBeenCalledWith(expect.stringContaining('field.id=значение')));
  });
});
''',
)

write(
    "tests/test_agent_hardening_contracts.py",
    r'''from __future__ import annotations

import ast
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_trial_allows_one_complete_monthly_kit() -> None:
    source = (ROOT / "src-tauri/src/main.rs").read_text("utf-8")
    assert "const TRIAL_MAX_DOCUMENTS_PER_RUN: u32 = TRIAL_DOCUMENT_LIMIT_MONTH;" in source
    assert "запрошено {}" in source
    assert "лимит за запуск {}" in source


def test_thin_and_offline_installers_have_distinct_payloads() -> None:
    thin = json.loads((ROOT / "src-tauri/tauri.thin.conf.json").read_text("utf-8"))
    offline = json.loads((ROOT / "src-tauri/tauri.offline.conf.json").read_text("utf-8"))
    assert thin["bundle"]["windows"]["webviewInstallMode"]["type"] == "downloadBootstrapper"
    assert thin["bundle"]["resources"] == []
    assert offline["bundle"]["windows"]["webviewInstallMode"]["type"] == "offlineInstaller"
    assert offline["bundle"]["resources"] == ["resources/tools/**"]


def test_local_windows_release_signs_binary_and_installer() -> None:
    script = (ROOT / "BUILD_WINDOWS_INSTALLER.bat").read_text("utf-8")
    assert "sign_windows_release.ps1 -ArtifactRoot target\\release\\dokkomplekt-tauri.exe" in script
    assert "sign_windows_release.ps1 -ArtifactRoot target\\release\\bundle\\nsis" in script
    assert "DOKKOMPLEKT_REQUIRE_AUTHENTICODE=1" in script
    assert "tauri.offline.conf.json" in script


def test_release_assets_wait_for_hardware_e2e() -> None:
    workflow = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")
    assert "types: [published]" in workflow
    assert "needs: [windows-hardware-e2e, linux-bundles]" in workflow
    assert "Publish only verified signed release assets" in workflow


def test_source_archive_excludes_virtual_environments_and_ascii_launchers_exist() -> None:
    module_path = ROOT / "scripts/build_source_archive.py"
    spec = importlib.util.spec_from_file_location("source_archive", module_path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assert {".venv", "venv", ".tox", ".nox"} <= module.EXCLUDED_DIRS
    assert (ROOT / "CHECK_PROJECT.bat").is_file()
    assert (ROOT / "BUILD_EXE.bat").is_file()
    assert not (ROOT / "ПРОВЕРИТЬ_ПРОЕКТ.bat").exists()
    assert not (ROOT / "СОБРАТЬ_EXE.bat").exists()


def test_rustsec_evidence_requires_a_real_database_commit() -> None:
    source = (ROOT / "scripts/write_rustsec_evidence.py").read_text("utf-8")
    tree = ast.parse(source)
    assert "advisory_database_commit" in source
    assert "len(head) != 40" in source
    assert any(isinstance(node, ast.Raise) for node in ast.walk(tree))
''',
)

# 6. Release source history separately, without mutating the deterministic source ZIP.
source_workflow = ".github/workflows/source-provenance.yml"
if (ROOT / source_workflow).is_file():
    content = read(source_workflow)
    if "Create portable Git history bundle" not in content:
        marker = "      - name: Upload source-manifest evidence\n"
        insertion = r'''      - name: Create portable Git history bundle
        run: |
          git bundle create verification/ci/Dokkomplekt_Universal-history.bundle --all
          git bundle verify verification/ci/Dokkomplekt_Universal-history.bundle
'''
        if marker not in content:
            raise RuntimeError("source provenance upload marker not found")
        content = content.replace(marker, insertion + marker)
        content = content.replace(
            "            verification/ci/source-manifest-report.json",
            "            verification/ci/source-manifest-report.json\n            verification/ci/Dokkomplekt_Universal-history.bundle",
        )
        write(source_workflow, content)

# Temporary patch machinery must not remain in the product.
for temporary in [
    ROOT / "scripts/agent_autofix.py",
    ROOT / ".github/workflows/agent-autofix.yml",
]:
    temporary.unlink(missing_ok=True)

# Regenerate deterministic source manifest last.
module_path = ROOT / "scripts/build_source_archive.py"
spec = importlib.util.spec_from_file_location("build_source_archive", module_path)
if spec is None or spec.loader is None:
    raise RuntimeError("cannot load source archive module")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
(ROOT / module.SOURCE_MANIFEST).write_bytes(module.source_manifest_payload())

print("Agent autofix applied successfully.")
