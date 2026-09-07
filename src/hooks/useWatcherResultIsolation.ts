import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { CreatedDocumentsIntakeResult, DocumentTemplateSpec, GeneratedOutput } from '../lib/types';
import { createdPrintItems, errorMessage } from '../lib/appSupport';
import { normalizeCreatedDocumentsIntakeResult } from '../lib/runtimeValidation';

type Options = {
  documents: DocumentTemplateSpec[];
  foregroundCaseActive: boolean;
  setLastOutput: Dispatch<SetStateAction<GeneratedOutput | null>>;
  setIntakeResult: Dispatch<SetStateAction<CreatedDocumentsIntakeResult | null>>;
  setStatus(message: string): void;
};

export function useWatcherResultIsolation({ documents, foregroundCaseActive, setLastOutput, setIntakeResult, setStatus }: Options) {
  const [backgroundNotice, setBackgroundNotice] = useState('');
  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    listen<unknown>('document-batch-ready', (event) => {
      try {
        const result = normalizeCreatedDocumentsIntakeResult(event.payload);
        if (foregroundCaseActive) {
          setBackgroundNotice(`Фоновый агент завершил отдельный комплект: ${result.message} Текущий открытый комплект не изменён.`);
          return;
        }
        setLastOutput(null);
        setIntakeResult(result);
        setStatus(result.message);
        if (result.status === 'processed' && result.created_files.length) {
          setLastOutput({ folder: result.patient_folder, files: result.created_files, source: 'watcher', print_items: createdPrintItems(result.created_documents, result.created_files, documents) });
        }
      } catch (error) {
        const message = `Фоновая обработка вернула некорректный результат: ${errorMessage(error)}`;
        if (foregroundCaseActive) setBackgroundNotice(message); else setStatus(message);
      }
    }).then((unlisten) => { if (disposed) unlisten(); else stopListening = unlisten; })
      .catch(() => { /* browser/tests: Tauri event bridge is unavailable */ });
    return () => { disposed = true; stopListening?.(); };
  }, [documents, foregroundCaseActive, setIntakeResult, setLastOutput, setStatus]);
  return { backgroundNotice, dismissBackgroundNotice: () => setBackgroundNotice('') };
}
