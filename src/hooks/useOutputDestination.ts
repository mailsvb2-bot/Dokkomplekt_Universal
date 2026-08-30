import { useEffect, useState } from 'react';
import {
  ensureOutputRoot,
  getBackgroundWatcherState,
  getOutputPlan,
  getOutputPreferences,
  installBackgroundWatcher,
  pickFolder,
  saveOutputPreferences,
  uninstallBackgroundWatcher,
} from '../lib/api';
import {
  currentDefaultYear,
  loadOutputFolderParts,
  loadOutputNamingConfirmed,
  loadOutputRoot,
  normalizeOutputFolderParts,
  saveOutputFolderParts,
  saveOutputRoot,
} from '../lib/appSupport';
import type { FolderNamePartDto, OutputPreferences } from '../lib/types';

type RunAction = <T>(
  label: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
) => Promise<T | undefined>;

export function useOutputDestination(
  run: RunAction,
  setStatus: (message: string) => void,
) {
  const cachedRoot = loadOutputRoot();
  const cachedParts = loadOutputFolderParts();
  const cachedConfirmed = loadOutputNamingConfirmed();
  const [watchFolder, setWatchFolder] = useState('');
  const [outputRoot, setOutputRoot] = useState(cachedRoot);
  const [outputRootDraft, setOutputRootDraft] = useState(cachedRoot);
  const [folderParts, setFolderParts] = useState<FolderNamePartDto[]>(cachedParts);
  const [folderNamingConfirmed, setFolderNamingConfirmed] = useState(cachedConfirmed && Boolean(cachedRoot));
  const [outputPreferencesReady, setOutputPreferencesReady] = useState(false);
  const [outputRootRecoveryRequired, setOutputRootRecoveryRequired] = useState(false);
  const [watcherRefreshRevision, setWatcherRefreshRevision] = useState(0);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const stored = await getOutputPreferences();
        if (!alive) return;
        if (cachedRoot.trim() && cachedConfirmed && (!stored.output_root.trim() || !stored.naming_confirmed)) {
          // One-time migration: old installations used only WebView localStorage.
          // Native startup may already have persisted an unconfirmed canonical default;
          // a previously confirmed user choice must win over that bootstrap value.
          const migrated = await saveOutputPreferences({
            output_root: cachedRoot,
            folder_parts: cachedParts,
            naming_confirmed: true,
          });
          if (!alive) return;
          setOutputRoot(migrated.output_root);
          setOutputRootDraft(migrated.output_root);
          setFolderParts(normalizeOutputFolderParts(migrated.folder_parts));
          setFolderNamingConfirmed(migrated.naming_confirmed);
          setOutputPreferencesReady(true);
        } else if (stored.output_root.trim()) {
          const normalizedParts = normalizeOutputFolderParts(stored.folder_parts);
          let rootReady = false;
          try {
            await ensureOutputRoot(stored.output_root);
            rootReady = true;
            setOutputRootRecoveryRequired(false);
          } catch {
            // Native startup intentionally keeps the window alive when the saved
            // destination cannot be prepared. Keep the exact path visible for
            // correction, but never treat it as confirmed or generation-ready.
            setOutputRootRecoveryRequired(true);
            setStatus('Не удалось подготовить папку готовых документов');
          }
          if (!alive) return;
          setOutputRoot(stored.output_root);
          setOutputRootDraft(stored.output_root);
          setFolderParts(normalizedParts);
          setFolderNamingConfirmed(rootReady && stored.naming_confirmed && normalizedParts.length > 0);
          saveOutputRoot(stored.output_root);
          saveOutputFolderParts(normalizedParts, rootReady && stored.naming_confirmed);
          setOutputPreferencesReady(true);
        } else {
          // A successful authoritative read with no configured destination is still hydrated.
          // The empty outputRoot keeps generation/watcher synchronization disabled until setup.
          setOutputPreferencesReady(true);
        }
      } catch (error) {
        if (alive) {
          setOutputPreferencesReady(false);
          setFolderNamingConfirmed(false);
          setStatus(`Не удалось восстановить проверенную папку результата: ${error instanceof Error ? error.message : String(error)}. Подтвердите папку заново.`);
        }
      }

      try {
        const watcher = await getBackgroundWatcherState();
        if (!alive) return;
        setWatchFolder(watcher.installed ? watcher.watch_folder?.trim() ?? '' : '');
        if (watcher.migration_required) {
          setStatus('Фоновый агент создан старой версией. Подтвердите папку готовых документов и включите агент заново.');
        }
      } catch (error) {
        if (alive) {
          setStatus(`Не удалось прочитать состояние фонового агента: ${error instanceof Error ? error.message : String(error)}.`);
        }
      }
    })();
    return () => { alive = false; };
  // Cached values are intentionally captured once for one-time migration.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function mirrorPreferencesLocally(preferences: OutputPreferences): boolean {
    const rootSaved = saveOutputRoot(preferences.output_root);
    const partsSaved = saveOutputFolderParts(preferences.folder_parts, preferences.naming_confirmed);
    return rootSaved && partsSaved;
  }

  async function updateFolderParts(parts: FolderNamePartDto[]): Promise<boolean> {
    if (!parts.length) {
      setFolderNamingConfirmed(false);
      setStatus('Имя подпапки не может быть пустым. Выберите хотя бы номер/дату документа, человека, организацию или период.');
      return false;
    }
    const normalized = normalizeOutputFolderParts(parts);
    if (normalized.length !== parts.length) {
      setFolderNamingConfirmed(false);
      setStatus('Правило подпапки содержит неизвестные или повторяющиеся элементы и не сохранено.');
      return false;
    }
    let saveError: string | null = null;
    const saved = await run(
      'save_output_preferences',
      () => saveOutputPreferences({
        output_root: outputRoot.trim(),
        folder_parts: normalized,
        naming_confirmed: Boolean(outputRoot.trim()),
      }),
      detail => { saveError = detail; },
    );
    if (!saved) {
      setFolderNamingConfirmed(false);
      setStatus(`Правило подпапки не сохранено: ${saveError ?? 'хранилище состояния недоступно'}.`);
      return false;
    }
    setFolderParts(saved.folder_parts);
    setFolderNamingConfirmed(saved.naming_confirmed);
    setOutputPreferencesReady(true);
    const cacheSaved = mirrorPreferencesLocally(saved);
    setStatus(cacheSaved
      ? `Правило подпапки сохранено${saved.output_root ? ` для ${saved.output_root}` : ''}.`
      : 'Правило подпапки надёжно сохранено в базе приложения; локальный UI-кэш недоступен, но это не влияет на результат.');
    return true;
  }

  async function commitOutputRoot(rawPath: string): Promise<boolean> {
    const candidate = rawPath.trim();
    if (!candidate) {
      let saveError: string | null = null;
      const saved = await run(
        'save_output_preferences',
        () => saveOutputPreferences({ output_root: '', folder_parts: folderParts, naming_confirmed: false }),
        detail => { saveError = detail; },
      );
      if (!saved) {
        setStatus(`Папка результата не очищена: ${saveError ?? 'хранилище состояния недоступно'}. Предыдущий проверенный путь сохранён.`);
        return false;
      }
      setOutputRoot('');
      setOutputRootDraft('');
      setFolderNamingConfirmed(false);
      setOutputPreferencesReady(true);
      setOutputRootRecoveryRequired(false);
      mirrorPreferencesLocally(saved);
      setStatus('Папка готовых документов очищена. Перед созданием комплекта выберите и сохраните новую папку.');
      return true;
    }

    let validationError: string | null = null;
    const validated = await run(
      'ensure_output_root',
      () => ensureOutputRoot(candidate),
      detail => { validationError = detail; },
    );
    if (!validated) {
      setStatus(`Папка не сохранена: ${validationError ?? 'проверка записи не пройдена'}. Подтверждённый путь не изменён.`);
      return false;
    }

    let saveError: string | null = null;
    const saved = await run(
      'save_output_preferences',
      () => saveOutputPreferences({
        output_root: validated,
        folder_parts: folderParts,
        naming_confirmed: folderNamingConfirmed && folderParts.length > 0,
      }),
      detail => { saveError = detail; },
    );
    if (!saved) {
      setStatus(`Папка прошла проверку записи, но настройка не сохранена: ${saveError ?? 'хранилище состояния недоступно'}. Старый путь продолжает действовать.`);
      return false;
    }

    setOutputRoot(saved.output_root);
    setOutputRootDraft(saved.output_root);
    setFolderParts(saved.folder_parts);
    setFolderNamingConfirmed(saved.naming_confirmed);
    setOutputPreferencesReady(true);
    setOutputRootRecoveryRequired(false);
    const cacheSaved = mirrorPreferencesLocally(saved);
    setStatus(cacheSaved
      ? `Папка готовых документов проверена и сохранена: ${saved.output_root}.`
      : `Папка готовых документов проверена и сохранена в базе приложения: ${saved.output_root}. Локальный UI-кэш недоступен.`);
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
    setStatus(`Рабочая папка фонового агента выбрана: ${selected}. Готовые документы будут сохраняться отдельно: ${outputRoot || 'сначала задайте папку результата'}.`);
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
    const destination = outputRoot.trim();
    if (!folder) {
      setStatus('Сначала выберите рабочую папку фонового агента на компьютере.');
      return;
    }
    if (!destination || !folderNamingConfirmed) {
      setStatus('Сначала подтвердите отдельную папку готовых документов и правило имени подпапки. Агент не включён.');
      return;
    }
    if (folder.replace(/[\\/]+$/, '').toLocaleLowerCase() === destination.replace(/[\\/]+$/, '').toLocaleLowerCase()) {
      setStatus('Рабочая папка и папка готовых документов должны быть разными. Агент не включён.');
      return;
    }
    const res = await run(
      'install_background_watcher',
      () => installBackgroundWatcher(folder, destination, currentDefaultYear(), sickLeave, folderParts, autoPrint, printCopies),
    );
    if (res) {
      setWatchFolder(res.watch_folder?.trim() || folder);
      setWatcherRefreshRevision((revision) => revision + 1);
      setStatus(`Автоматическая обработка включена: исходники «${res.watch_folder ?? folder}» → готовые документы «${res.output_root ?? destination}»${res.warnings?.length ? `; замечания: ${res.warnings.join('; ')}` : ''}.`);
    }
  }

  async function uninstallWatcher(): Promise<void> {
    let uninstallError: string | null = null;
    const result = await run('uninstall_background_watcher', () => uninstallBackgroundWatcher(), detail => { uninstallError = detail; });
    if (uninstallError || !result) {
      setStatus(`Фоновый агент не подтверждён как отключённый: ${uninstallError ?? 'backend не подтвердил удаление'}`);
      return;
    }
    setWatchFolder('');
    setWatcherRefreshRevision((revision) => revision + 1);
    setStatus('Автоматическая обработка папки отключена.');
  }

  return {
    watchFolder,
    outputRoot,
    outputRootDraft,
    folderParts,
    folderNamingConfirmed,
    outputPreferencesReady,
    outputRootRecoveryRequired,
    watcherRefreshRevision,
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
