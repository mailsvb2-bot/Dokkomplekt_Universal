import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import { exportTemplateTransfer, importTemplateTransfer, pickTemplateTransferFile } from '../lib/api';
import type { DocumentTemplateSpec } from '../lib/types';

type RunAction = <T>(
  label: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
) => Promise<T | undefined>;

interface Options {
  run: RunAction;
  setDocuments: Dispatch<SetStateAction<DocumentTemplateSpec[]>>;
  setStatus(message: string): void;
}

export function useTemplateTransfer(options: Options) {
  const exportTemplates = useCallback(async () => {
    const result = await options.run('export_template_transfer', () => exportTemplateTransfer());
    if (!result) return;
    options.setStatus(`Шаблоны экспортированы: ${result.document_count}. Пакет: ${result.package_path}`);
  }, [options]);

  const importTemplates = useCallback(async () => {
    const selected = await options.run('pick_template_transfer_file', () => pickTemplateTransferFile());
    if (!selected) return;
    const result = await options.run('import_template_transfer', () => importTemplateTransfer(selected));
    if (!result) return;
    options.setDocuments(result.pack.documents);
    options.setStatus(`Импортировано шаблонов: ${result.document_count}. Данные предыдущего дела не переносились.`);
  }, [options]);

  return { exportTemplates, importTemplates };
}
