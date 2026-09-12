import { useEffect, useRef, useState } from 'react';
import { getBackgroundWatcherState, updateBackgroundWatcherPreferences } from '../lib/api';
import { errorMessage } from '../lib/appSupport';
import type { FolderNamePartDto } from '../lib/types';
function updateWatcherPreferencesSnapshot(
  outputRoot: string,
  folderParts: FolderNamePartDto[],
  autoPrint: boolean,
  printCopies: Record<string, number>,
) {
  return updateBackgroundWatcherPreferences(outputRoot, folderParts, autoPrint, printCopies);
}


type WatcherPreferenceSyncOptions = {
  outputPreferencesReady: boolean;
  watcherRefreshRevision: number;
  folderNamingConfirmed: boolean;
  outputRoot: string;
  folderParts: FolderNamePartDto[];
  autoPrint: boolean;
  printCopies: Record<string, number>;
  setAutoPrint: (value: boolean) => void;
  setPrintCopies: (value: Record<string, number>) => void;
  setStatus: (message: string) => void;
};

export function useWatcherPreferenceSync({
  outputPreferencesReady,
  watcherRefreshRevision,
  folderNamingConfirmed,
  outputRoot,
  folderParts,
  autoPrint,
  printCopies,
  setAutoPrint,
  setPrintCopies,
  setStatus,
}: WatcherPreferenceSyncOptions): void {
  const [watcherPreferencesReady, setWatcherPreferencesReady] = useState(false);
  const syncQueue = useRef<Promise<void>>(Promise.resolve());
  const syncRevision = useRef(0);

  useEffect(() => {
    let alive = true;
    void getBackgroundWatcherState()
      .then((watcher) => {
        if (!alive) return;
        if (watcher.installed) {
          if (typeof watcher.auto_print === 'boolean') setAutoPrint(watcher.auto_print);
          if (watcher.print_copies_by_document) setPrintCopies(watcher.print_copies_by_document);
        }
        setWatcherPreferencesReady(!watcher.migration_required);
      })
      .catch((error) => {
        if (!alive) return;
        setWatcherPreferencesReady(false);
        setStatus(`Не удалось восстановить настройки фонового агента: ${errorMessage(error)}.`);
      });
    return () => { alive = false; };
  }, [watcherRefreshRevision, setAutoPrint, setPrintCopies, setStatus]);

  useEffect(() => {
    if (!watcherPreferencesReady || !outputPreferencesReady || !folderNamingConfirmed || !outputRoot.trim() || !folderParts.length) return;
    const revision = ++syncRevision.current;
    const rootSnapshot = outputRoot;
    const partsSnapshot = [...folderParts];
    const autoPrintSnapshot = autoPrint;
    const copiesSnapshot = { ...printCopies };
    syncQueue.current = syncQueue.current
      .catch(() => undefined)
      .then(async () => {
        try {
          await updateWatcherPreferencesSnapshot(rootSnapshot, partsSnapshot, autoPrintSnapshot, copiesSnapshot);
        } catch (error) {
          if (revision === syncRevision.current) {
            setStatus(`Не удалось синхронизировать настройки фонового агента: ${errorMessage(error)}. Агент продолжает использовать последнюю подтверждённую конфигурацию.`);
          }
        }
      });
  }, [watcherPreferencesReady, outputPreferencesReady, folderNamingConfirmed, outputRoot, folderParts, autoPrint, printCopies, setStatus]);
}
