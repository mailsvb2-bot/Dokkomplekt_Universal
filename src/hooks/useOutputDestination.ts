import { useState } from 'react';
import {
  ensureOutputRoot,
  getOutputPlan,
  installBackgroundWatcher,
  pickFolder,
  uninstallBackgroundWatcher,
} from '../lib/api';
import {
  DEFAULT_YEAR,
  loadOutputFolderParts,
  loadOutputNamingConfirmed,
  loadOutputRoot,
  saveOutputFolderParts,
  saveOutputRoot,
} from '../lib/appSupport';
import type { FolderNamePartDto } from '../lib/types';

type RunAction = <T>(
  label: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
) => Promise<T | undefined>;

export function useOutputDestination(
  run: RunAction,
  setStatus: (message: string) => void,
) {
  const [watchFolder, setWatchFolder] = useState('');
  const [outputRoot, setOutputRoot] = useState(loadOutputRoot);
  const [outputRootDraft, setOutputRootDraft] = useState(loadOutputRoot);
  const [folderParts, setFolderParts] = useState<FolderNamePartDto[]>(loadOutputFolderParts);
  const [folderNamingConfirmed, setFolderNamingConfirmed] = useState(loadOutputNamingConfirmed);

  function updateFolderParts(parts: FolderNamePartDto[]) {
    setFolderParts(parts);
    saveOutputFolderParts(parts, true);
    setFolderNamingConfirmed(true);
  }

  async function commitOutputRoot(rawPath: string): Promise<boolean> {
    const candidate = rawPath.trim();
    if (!candidate) {
      setOutputRoot('');
      setOutputRootDraft('');
      saveOutputRoot('');
      setFolderNamingConfirmed(false);
      setStatus('Папка готовых документов очищена. Перед созданием комплекта выберите и сохраните новую папку.');
      return true;
    }
    let validationError: string | null = null;
    const validated = await run(
      'ensure_output_root',
      () => ensureOutputRoot(candidate),
      (detail) => { validationError = detail; },
    );
    if (!validated) {
      setStatus(`Папка не сохранена: ${validationError ?? 'проверка записи не пройдена'}. Подтверждённый путь не изменён.`);
      return false;
    }
    setOutputRoot(validated);
    setOutputRootDraft(validated);
    saveOutputRoot(validated);
    setStatus(`Папка готовых документов проверена и сохранена: ${validated}.`);
    return true;
  }

  async function chooseAndCommitOutputFolder(): Promise<void> {
    const selected = await run('pick_folder', () => pickFolder(outputRootDraft || outputRoot));
    if (!selected) return;
    setOutputRootDraft(selected);
    await commitOutputRoot(selected);
  }

  async function chooseWatchFolder(): Promise<void> {
    const selected = await run('pick_folder', () => pickFolder(watchFolder));
    if (!selected) return;
    setWatchFolder(selected);
    setStatus(`Рабочая папка: ${selected}`);
  }

  async function outputPlan(labels: string[]): Promise<void> {
    if (!outputRoot.trim()) {
      setStatus('Укажите корневую папку вывода.');
      return;
    }
    const res = await run('get_output_plan', () => getOutputPlan(outputRoot.trim(), folderParts, labels));
    if (res) setStatus(`Папка комплекта: ${res.patient_folder}`);
  }

  async function installWatcher(
    sickLeave: boolean,
    autoPrint: boolean,
    printCopies: Record<string, number>,
  ): Promise<void> {
    const folder = watchFolder.trim();
    if (!folder) {
      setStatus('Сначала выберите рабочую папку фонового агента на компьютере. Относительный путь по умолчанию больше не используется.');
      return;
    }
    const res = await run(
      'install_background_watcher',
      () => installBackgroundWatcher(folder, DEFAULT_YEAR, sickLeave, folderParts, autoPrint, printCopies),
    );
    if (res) {
      setStatus(`Автоматическая обработка включена для папки «${res.watch_folder ?? ''}»${res.warnings?.length ? `; замечания: ${res.warnings.join('; ')}` : ''}.`);
    }
  }

  async function uninstallWatcher(): Promise<void> {
    let uninstallError: string | null = null;
    await run('uninstall_background_watcher', () => uninstallBackgroundWatcher(), (detail) => { uninstallError = detail; });
    if (uninstallError) {
      setStatus(`Фоновый агент не подтверждён как отключённый: ${uninstallError}`);
      return;
    }
    setStatus('Автоматическая обработка папки отключена.');
  }

  return {
    watchFolder,
    outputRoot,
    outputRootDraft,
    folderParts,
    folderNamingConfirmed,
    setOutputRootDraft,
    setFolderNamingConfirmed,
    updateFolderParts,
    commitOutputRoot,
    chooseAndCommitOutputFolder,
    chooseWatchFolder,
    outputPlan,
    installWatcher,
    uninstallWatcher,
  };
}
