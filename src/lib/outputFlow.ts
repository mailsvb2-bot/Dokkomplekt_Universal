import type { FolderNamePartDto } from './types';

export type ExistingOutputPolicy = 'version' | 'replace_with_backup';

export type OpenFolderAttempt = {
  opened: boolean;
  error?: string;
};

type PlannedOutput = {
  exists: boolean;
  patient_folder: string;
};

type ConfirmOptions = {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
};

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try { return JSON.stringify(error); } catch { return 'неизвестная ошибка оболочки'; }
}

/**
 * Opening a folder is convenience, not publication. Keep a successful generation
 * successful, but return the shell failure to the caller so it can be shown instead
 * of silently claiming that the folder opened.
 */
export async function openCreatedOutputFolderSilently(
  outputFolder: string,
  openFolder: (path: string) => Promise<unknown>,
): Promise<OpenFolderAttempt> {
  const target = outputFolder.trim();
  if (!target) return { opened: false, error: 'путь готового комплекта пуст' };
  try {
    await openFolder(target);
    return { opened: true };
  } catch (error) {
    return { opened: false, error: errorText(error) };
  }
}

export async function chooseExistingOutputPolicyFlow(params: {
  outputRoot: string;
  folderParts: FolderNamePartDto[];
  labels: string[];
  getPlan: (root: string, parts: FolderNamePartDto[], labels: string[]) => Promise<PlannedOutput | null | undefined>;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  openFolder: (path: string) => Promise<unknown>;
  onStatus: (message: string) => void;
  onMissingRoot: () => void;
}): Promise<ExistingOutputPolicy | null> {
  const explicitOutputRoot = params.outputRoot.trim();
  if (!explicitOutputRoot) {
    params.onStatus('Сначала выберите папку готовых документов. Ничего не создано.');
    params.onMissingRoot();
    return null;
  }

  const planned = await params.getPlan(explicitOutputRoot, params.folderParts, params.labels);
  if (!planned) return null;
  if (!planned.exists) return 'version';

  if (await params.confirm({
    title: 'Комплект уже существует',
    message: `Папка уже есть: ${planned.patient_folder}. Открыть существующий комплект без создания новых файлов?`,
    confirmLabel: 'Открыть существующий',
    cancelLabel: 'Другие варианты',
  })) {
    const opened = await openCreatedOutputFolderSilently(planned.patient_folder, params.openFolder);
    params.onStatus(opened.opened
      ? 'Открыт существующий комплект. Новые файлы не создавались.'
      : `Существующий комплект не изменён, но папку не удалось открыть: ${opened.error}. Путь: ${planned.patient_folder}`);
    return null;
  }

  if (await params.confirm({
    title: 'Создать новую версию?',
    message: 'Текущий комплект останется без изменений, а новый будет опубликован в уникальной папке с номером версии.',
    confirmLabel: 'Создать новую версию',
    cancelLabel: 'Другие варианты',
  })) return 'version';

  if (!await params.confirm({
    title: 'Заменить комплект с резервной копией?',
    message: 'Существующая папка сначала будет целиком перенесена в резервную копию. Только после этого новый комплект займёт её место. При ошибке программа попытается восстановить старую папку.',
    confirmLabel: 'Заменить с резервной копией',
    cancelLabel: 'Отмена',
    danger: true,
  })) {
    params.onStatus('Создание комплекта отменено. Существующая папка не изменена.');
    return null;
  }
  return 'replace_with_backup';
}
