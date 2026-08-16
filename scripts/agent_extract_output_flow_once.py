from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, found {count}: {old[:160]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")

replace_once(
    "src/App.tsx",
    "import { createPendingTemplateIntelligenceHandlers } from './lib/pendingTemplateIntelligence';\n",
    "import { createPendingTemplateIntelligenceHandlers } from './lib/pendingTemplateIntelligence';\nimport { chooseExistingOutputPolicyFlow } from './lib/outputFlow';\n",
)

replace_once(
    "src/App.tsx",
    '''  async function exportLastOutputKedo() {
    if (!lastOutput?.files.length) return;
    const result = await run('create_kedo_package', () => createKedoPackage(
      lastOutput.files,
      lastOutput.folder || outputRoot.trim() || 'output/Готовые документы',
    ));
    if (!result) return;
    setStatus(`Пакет обмена создан: ${result.package_folder}.`);
  }

  async function chooseExistingOutputPolicy(documentIds: string[]): Promise<'version' | 'replace_with_backup' | null> {
    const explicitOutputRoot = outputRoot.trim();
    if (!explicitOutputRoot) {
      setStatus('Сначала выберите папку готовых документов. Ничего не создано.');
      setFolderNamingConfirmed(false);
      return null;
    }
    const labels = documentIds.map(id => documents.find(document => document.id === id)?.button_label).filter((value): value is string => Boolean(value));
    const planned = await run('get_output_plan', () => getOutputPlan(explicitOutputRoot, folderParts, labels));
    if (!planned) return null;
    if (!planned.exists) return 'version';
    if (await dialogs.confirm({ title: 'Комплект уже существует', message: `Папка уже есть: ${planned.patient_folder}. Открыть существующий комплект без создания новых файлов?`, confirmLabel: 'Открыть существующий', cancelLabel: 'Другие варианты' })) {
      await run('open_in_file_manager', () => openInFileManager(planned.patient_folder));
      setStatus('Открыт существующий комплект. Новые файлы не создавались.');
      return null;
    }
    if (await dialogs.confirm({ title: 'Создать новую версию?', message: 'Текущий комплект останется без изменений, а новый будет опубликован в уникальной папке с номером версии.', confirmLabel: 'Создать новую версию', cancelLabel: 'Другие варианты' })) return 'version';
    if (!await dialogs.confirm({ title: 'Заменить комплект с резервной копией?', message: 'Существующая папка сначала будет целиком перенесена в резервную копию. Только после этого новый комплект займёт её место. При ошибке программа попытается восстановить старую папку.', confirmLabel: 'Заменить с резервной копией', cancelLabel: 'Отмена', danger: true })) {
      setStatus('Создание комплекта отменено. Существующая папка не изменена.');
      return null;
    }
    return 'replace_with_backup';
  }''',
    '''  async function exportLastOutputKedo() {
    if (!lastOutput?.files.length) return;
    const packageRoot = lastOutput.folder || outputRoot.trim();
    if (!packageRoot) { setStatus('Папка готовых документов не определена. Пакет обмена не создан.'); return; }
    const result = await run('create_kedo_package', () => createKedoPackage(lastOutput.files, packageRoot));
    if (!result) return;
    setStatus(`Пакет обмена создан: ${result.package_folder}.`);
  }

  async function chooseExistingOutputPolicy(documentIds: string[]) {
    const labels = documentIds.map(id => documents.find(document => document.id === id)?.button_label).filter((value): value is string => Boolean(value));
    return chooseExistingOutputPolicyFlow({
      outputRoot, folderParts, labels,
      getPlan: (root, parts, names) => run('get_output_plan', () => getOutputPlan(root, parts, names)),
      confirm: (options) => dialogs.confirm(options),
      openFolder: (path) => run('open_in_file_manager', () => openInFileManager(path)),
      onStatus: setStatus,
      onMissingRoot: () => setFolderNamingConfirmed(false),
    });
  }''',
)

replace_once(
    "src/App.tsx",
    '''    const explicitOutputRoot = outputRoot.trim();
    if (!explicitOutputRoot) {
      setStatus('Сначала выберите папку готовых документов. Ничего не создано.');
      setFolderNamingConfirmed(false);
      return;
    }
    const res = await run('render_docx_batch', () => renderDocxBatch(documentIds, explicitOutputRoot, folderParts, true, existingOutputPolicy));''',
    '''    const explicitOutputRoot = outputRoot.trim();
    if (!explicitOutputRoot) return;
    const res = await run('render_docx_batch', () => renderDocxBatch(documentIds, explicitOutputRoot, folderParts, true, existingOutputPolicy));''',
)

replace_once(
    "tests/test_donor_diary_output_parity.py",
    '''    app = read("src/App.tsx")
    workspace = read("src/components/Workspace.tsx")''',
    '''    app = read("src/App.tsx")
    output_flow = read("src/lib/outputFlow.ts")
    workspace = read("src/components/Workspace.tsx")''',
)
replace_once(
    "tests/test_donor_diary_output_parity.py",
    '''    assert "outputRoot.trim() || 'output/Готовые документы'" not in app
    assert "Сначала выберите папку готовых документов. Ничего не создано." in app
    assert "Создано документов:" in workspace''',
    '''    assert "outputRoot.trim() || 'output/Готовые документы'" not in app
    assert "Сначала выберите папку готовых документов. Ничего не создано." in output_flow
    assert "Создано документов:" in workspace''',
)

print("output flow extracted from App")
