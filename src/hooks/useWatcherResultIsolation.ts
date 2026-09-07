import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { CreatedDocumentsIntakeResult, DocumentTemplateSpec, GeneratedOutput } from '../lib/types';
import { createdPrintItems, errorMessage } from '../lib/appSupport';
import { normalizeCreatedDocumentsIntakeResult } from '../lib/runtimeValidation';


export function watcherForegroundCaseActive(options: {
  sourceFileName: string | null;
  hasParsedSource: boolean;
  sourceText: string;
  intakeSource: string;
  intakeResult: CreatedDocumentsIntakeResult | null;
  lastOutput: GeneratedOutput | null;
}): boolean {
  return Boolean(options.sourceFileName || options.hasParsedSource || options.sourceText.trim() || options.intakeSource.trim() || options.intakeResult || options.lastOutput?.source === 'zero_touch');
}

type Options = {
  documents: DocumentTemplateSpec[];
  foregroundCaseActive: boolean;
  setLastOutput: Dispatch<SetStateAction<GeneratedOutput | null>>;
  setIntakeResult: Dispatch<SetStateAction<CreatedDocumentsIntakeResult | null>>;
  setStatus(message: string): void;
};

export function useWatcherResultIsolation({ documents, foregroundCaseActive, setLastOutput, setIntakeResult, setStatus }: Options) {
  const [backgroundNotice, setBackgroundNotice] = useState('');
  const current = useRef({ documents, foregroundCaseActive, setLastOutput, setIntakeResult, setStatus });
  current.current = { documents, foregroundCaseActive, setLastOutput, setIntakeResult, setStatus };

  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    listen<unknown>('document-batch-ready', (event) => {
      if (disposed) return;
      const options = current.current;
      try {
        const result = normalizeCreatedDocumentsIntakeResult(event.payload);
        if (options.foregroundCaseActive) {
          setBackgroundNotice(`Фоновый агент завершил отдельный комплект: ${result.message} Текущий открытый комплект не изменён.`);
          return;
        }
        options.setLastOutput(null);
        options.setIntakeResult(result);
        options.setStatus(result.message);
        if (result.status === 'processed' && result.created_files.length) {
          options.setLastOutput({ folder: result.patient_folder, files: result.created_files, source: 'watcher', print_items: createdPrintItems(result.created_documents, result.created_files, options.documents) });
        }
      } catch (error) {
        const message = `Фоновая обработка вернула некорректный результат: ${errorMessage(error)}`;
        if (options.foregroundCaseActive) setBackgroundNotice(message); else options.setStatus(message);
      }
    }).then((unlisten) => { if (disposed) unlisten(); else stopListening = unlisten; })
      .catch(() => { /* browser/tests: Tauri event bridge is unavailable */ });
    return () => { disposed = true; stopListening?.(); };
  }, []);
  return { backgroundNotice, dismissBackgroundNotice: () => setBackgroundNotice('') };
}
