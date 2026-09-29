import { useCallback } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import type { AppDialogApi } from '../components/AppDialogProvider';
import { listTemplateVersions, rollbackTemplateVersion } from '../lib/api';
import type { DocumentPack, DocumentTemplateSpec } from '../lib/types';

type RunAction = <T>(
  label: string,
  action: () => Promise<T>,
  onError?: (detail: string) => void,
) => Promise<T | undefined>;

interface Options {
  activeDocumentId: string | null;
  documents: DocumentTemplateSpec[];
  dialogs: AppDialogApi;
  run: RunAction;
  setDocuments: Dispatch<SetStateAction<DocumentTemplateSpec[]>>;
  setStatus(message: string): void;
}

export function useTemplateVersionSelection(options: Options) {
  return useCallback(async () => {
    const activeDocumentId = options.activeDocumentId;
    if (!activeDocumentId) return;
    const current = options.documents.find((document) => document.id === activeDocumentId);
    const versions = await options.run('list_template_versions', () => listTemplateVersions(activeDocumentId));
    if (!versions?.length) {
      options.setStatus(`Для «${current?.button_label ?? activeDocumentId}» сохранённых версий шаблона пока нет.`);
      return;
    }

    const ordered = [...versions].sort((left, right) => right.version_number - left.version_number);
    const published = ordered.find((version) => version.status === 'published');
    const requested = await options.dialogs.prompt({
      title: 'Версии шаблона',
      message: ordered.map((version) => {
        const currentMark = version.status === 'published' ? ' — используется сейчас' : '';
        const note = version.note?.trim() ? ` — ${version.note.trim()}` : '';
        return `Версия ${version.version_number}${currentMark}${note}`;
      }).join('\n'),
      label: 'Номер версии для использования',
      initialValue: String(published?.version_number ?? ordered[0].version_number),
      required: true,
      confirmLabel: 'Использовать версию',
    });
    if (!requested) return;

    const versionNumber = Number.parseInt(requested.trim(), 10);
    const target = ordered.find((version) => version.version_number === versionNumber);
    if (!target || String(versionNumber) !== requested.trim()) {
      options.setStatus('Такой сохранённой версии шаблона нет. Ничего не изменено.');
      return;
    }
    if (target.status === 'published') {
      options.setStatus(`Версия ${target.version_number} уже используется для «${current?.button_label ?? activeDocumentId}».`);
      return;
    }

    const confirmed = await options.dialogs.confirm({
      title: `Использовать версию ${target.version_number}?`,
      message: 'Будет создана новая опубликованная ревизия, точно повторяющая выбранную сохранённую версию. История версий останется сохранённой.',
      confirmLabel: 'Использовать версию',
      cancelLabel: 'Отмена',
    });
    if (!confirmed) return;

    const pack: DocumentPack | undefined = await options.run(
      'rollback_template_version',
      () => rollbackTemplateVersion(target.version_id),
    );
    if (!pack) return;
    options.setDocuments(pack.documents);
    options.setStatus(`Версия ${target.version_number} выбрана для «${current?.button_label ?? activeDocumentId}». Следующее создание использует её сохранённый snapshot.`);
  }, [options]);
}
