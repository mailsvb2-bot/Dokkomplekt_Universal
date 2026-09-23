import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react';
import { setDocumentSelection } from '../lib/api';
import { errorMessage } from '../lib/appSupport';

interface DocumentSelectionPersistenceOptions {
  ready: boolean;
  selectedDocumentIds: string[];
  setSelectedDocumentIds: Dispatch<SetStateAction<string[]>>;
  setStatus: Dispatch<SetStateAction<string>>;
}

export function useDocumentSelectionPersistence(options: DocumentSelectionPersistenceOptions) {
  const hydrated = useRef(false);
  const persistenceChain = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!options.ready) return;
    if (!hydrated.current) {
      hydrated.current = true;
      return;
    }
    const requested = [...options.selectedDocumentIds];
    persistenceChain.current = persistenceChain.current
      .then(async () => {
        const persisted = await setDocumentSelection(requested);
        if (persisted.length !== requested.length || persisted.some((id, index) => id !== requested[index])) {
          options.setSelectedDocumentIds(persisted);
        }
      })
      .catch((reason) => {
        options.setStatus(`Не удалось сохранить выбор документов: ${errorMessage(reason)}.`);
      });
  }, [options.ready, options.selectedDocumentIds]);
}
