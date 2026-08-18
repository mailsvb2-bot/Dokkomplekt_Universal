import type { FolderNamePartDto } from './types';

export type ExistingOutputPolicy = 'version' | 'replace_with_backup';

export type PreparedGenerationOutput = {
  outputRoot: string;
  existingOutputPolicy: ExistingOutputPolicy;
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

type ExistingOutputFlowParams = {
  outputRoot: string;
  folderParts: FolderNamePartDto[];
  labels: string[];
  getPlan: (root: string, parts: FolderNamePartDto[], labels: string[]) => Promise<PlannedOutput | null | undefined>;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  openFolder: (path: string) => Promise<unknown>;
  onStatus: (message: string) => void;
  onMissingRoot: () => void;
};

export async function chooseExistingOutputPolicyFlow(params: ExistingOutputFlowParams): Promise<ExistingOutputPolicy | null> {
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
    await params.openFolder(planned.patient_folder);
    params.onStatus('Открыт существующий комплект. Новые файлы не создавались.');
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

export async function prepareGenerationOutputFlow(params: ExistingOutputFlowParams & {
  getDefaultRoot: () => Promise<string>;
  onResolvedRoot: (root: string) => void;
}): Promise<PreparedGenerationOutput | null> {
  let outputRoot = params.outputRoot.trim();
  if (!outputRoot) {
    outputRoot = (await params.getDefaultRoot()).trim();
    if (!outputRoot) {
      params.onStatus('Не удалось создать стандартную папку «Выписанные пациенты». Выберите папку готовых документов вручную.');
      params.onMissingRoot();
      return null;
    }
    params.onResolvedRoot(outputRoot);
  }

  const existingOutputPolicy = await chooseExistingOutputPolicyFlow({ ...params, outputRoot });
  return existingOutputPolicy ? { outputRoot, existingOutputPolicy } : null;
}
