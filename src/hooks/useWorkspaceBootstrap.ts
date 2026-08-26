import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { firstRunState } from '../lib/api';
import { defaultSelectedDocumentIds, errorMessage } from '../lib/appSupport';
import type { DocumentTemplateSpec } from '../lib/types';

interface WorkspaceBootstrapOptions {
  setDocuments: Dispatch<SetStateAction<DocumentTemplateSpec[]>>;
  setSelectedDocIds: Dispatch<SetStateAction<string[]>>;
  setStatus: Dispatch<SetStateAction<string>>;
}

export function useWorkspaceBootstrap(options: WorkspaceBootstrapOptions) {
  const [ready, setReady] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  function applyLoadedState(res: Awaited<ReturnType<typeof firstRunState>>) {
    if (res?.pack?.documents?.length) {
      options.setDocuments(res.pack.documents);
      options.setSelectedDocIds(defaultSelectedDocumentIds(res.pack.documents));
      options.setStatus(`Рабочий набор готов: ${res.pack.documents.length} документ(ов). Добавьте исходный файл.`);
    } else if (res?.has_user_buttons === false) {
      options.setDocuments([]);
      options.setSelectedDocIds([]);
      options.setStatus('Нажмите «Создать свои кнопки» и выберите ваши шаблоны Word.');
    } else if (res?.message) {
      options.setStatus(res.message);
    }
    setReady(true);
    setError('');
  }

  function recordFailure(reason: unknown) {
    const detail = errorMessage(reason);
    setReady(false);
    setError(detail);
    options.setStatus(`Не удалось загрузить сохранённый рабочий набор: ${detail}. Изменение шаблонов и запуск нового комплекта временно заблокированы.`);
  }

  async function load() {
    setLoading(true);
    setError('');
    try {
      applyLoadedState(await firstRunState());
    } catch (reason) {
      recordFailure(reason);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let alive = true;
    void firstRunState()
      .then((res) => { if (alive) applyLoadedState(res); })
      .catch((reason) => { if (alive) recordFailure(reason); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, []);
  return {
    workspaceStateReady: ready,
    workspaceStateLoading: loading,
    workspaceStateError: error,
    retryWorkspaceStateLoad: load,
  };
}
