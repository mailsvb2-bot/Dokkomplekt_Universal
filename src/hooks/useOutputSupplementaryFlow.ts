import { useEffect, useState } from 'react';
import type { DocumentTemplateSpec, FolderNamePartDto, OutputConflictPolicy, SupplementarySourceDto } from '../lib/types';
import {
  attachSupplementaryFile,
  attachSupplementaryFolder,
  getOutputPlan,
  listSupplementarySources,
  openInFileManager,
  pickFolder,
  removeSupplementarySource,
} from '../lib/api';
import {
  OUTPUT_NAMING_PRESETS,
  arrayBufferToBase64,
  loadOutputFolderParts,
  loadOutputNamingConfirmed,
  outputNamingPreset,
  readFileBytes,
  saveOutputFolderParts,
} from '../lib/appSupport';

type RunAction = <T>(name: string, action: () => Promise<T>) => Promise<T | undefined>;

interface ChoiceOption {
  value: string;
  label: string;
  description?: string;
  danger?: boolean;
}

interface DialogApi {
  choose(options: {
    title: string;
    message?: string;
    options: ChoiceOption[];
    cancelLabel?: string;
  }): Promise<string | null>;
}

interface OutputSupplementaryFlowOptions {
  dialogs: DialogApi;
  run: RunAction;
  documents: DocumentTemplateSpec[];
  outputRoot: string;
  ensureComponentForSource(fileName: string): Promise<boolean>;
  setUtilityOpen(value: boolean): void;
  setStatus(message: string): void;
}

export function useOutputSupplementaryFlow(options: OutputSupplementaryFlowOptions) {
  const {
    dialogs,
    run,
    documents,
    outputRoot,
    ensureComponentForSource,
    setUtilityOpen,
    setStatus,
  } = options;
  const [folderParts, setFolderParts] = useState<FolderNamePartDto[]>(loadOutputFolderParts);
  const [outputNamingConfirmed, setOutputNamingConfirmed] = useState(loadOutputNamingConfirmed);
  const [supplementarySources, setSupplementarySources] = useState<SupplementarySourceDto[]>([]);

  useEffect(() => {
    let alive = true;
    void listSupplementarySources()
      .then((response) => { if (alive) setSupplementarySources(response.sources); })
      .catch(() => { /* browser/tests */ });
    return () => { alive = false; };
  }, []);

  function updateFolderParts(parts: FolderNamePartDto[]) {
    const next = saveOutputFolderParts(parts);
    setFolderParts(next);
    setOutputNamingConfirmed(next.length > 0);
  }

  async function ensureOutputNamingConfirmed(): Promise<FolderNamePartDto[] | null> {
    if (outputNamingConfirmed && folderParts.length) return folderParts;
    const choice = await dialogs.choose({
      title: 'Как называть папку комплекта?',
      message: 'Выберите принцип один раз. Он подходит для любой профессии и будет сохранён; позже его можно изменить в настройках.',
      options: [
        ...OUTPUT_NAMING_PRESETS.map((preset) => ({
          value: preset.value,
          label: preset.label,
          description: preset.description,
        })),
        {
          value: 'manual',
          label: 'Настроить вручную',
          description: 'Открыть полный список частей имени: организация, имя, даты, период, месяцы и другие.',
        },
      ],
      cancelLabel: 'Отмена',
    });
    if (!choice) return null;
    if (choice === 'manual') {
      setUtilityOpen(true);
      setStatus('В настройках выберите состав имени папки комплекта и повторите создание.');
      return null;
    }
    const preset = outputNamingPreset(choice);
    if (!preset) return null;
    const next = saveOutputFolderParts(preset.parts);
    setFolderParts(next);
    setOutputNamingConfirmed(true);
    return next;
  }

  async function attachSupplementaryFiles(files: File[], role: string) {
    for (const file of files.slice(0, 200)) {
      if (!(await ensureComponentForSource(file.name))) continue;
      const bytes = await readFileBytes(file);
      const result = await run('attach_supplementary_file', () => attachSupplementaryFile(
        role,
        file.name,
        arrayBufferToBase64(bytes),
        (file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name,
      ));
      if (result) {
        setSupplementarySources(result.sources);
        if (result.warnings.length) setStatus(result.warnings.join(' '));
      }
    }
  }

  async function attachSupplementaryFolderByRole(role: string) {
    const folder = await run('pick_folder', () => pickFolder(null));
    if (!folder) return;
    const result = await run(
      'attach_supplementary_folder',
      () => attachSupplementaryFolder(role, folder),
    );
    if (!result) return;
    setSupplementarySources(result.sources);
    setStatus(result.warnings.length
      ? `Дополнительные материалы добавлены; замечания: ${result.warnings.join(' ')}`
      : `Дополнительные материалы добавлены из папки: ${folder}`);
  }

  async function removeSupplementary(sourceId: string) {
    const result = await run(
      'remove_supplementary_source',
      () => removeSupplementarySource(sourceId),
    );
    if (result) setSupplementarySources(result.sources);
  }

  async function outputConflictPolicy(
    documentIds: string[],
    namingParts: FolderNamePartDto[],
  ): Promise<OutputConflictPolicy | 'open_existing' | null> {
    const labels = documents
      .filter((document) => documentIds.includes(document.id))
      .map((document) => document.button_label);
    const target = await run('get_output_plan', () => getOutputPlan(
      outputRoot.trim() || 'output/Готовые документы',
      namingParts,
      labels,
    ));
    if (!target || !target.target_exists) return 'create_new_version';
    const choice = await dialogs.choose({
      title: 'Комплект уже существует. Что сделать?',
      message: target.patient_folder,
      options: [
        {
          value: 'open_existing',
          label: 'Открыть существующий',
          description: 'Ничего не создавать и открыть уже готовую папку.',
        },
        {
          value: 'create_new_version',
          label: 'Создать новую версию',
          description: 'Сохранить существующий комплект и создать отдельную папку (2), (3) и т. д.',
        },
        {
          value: 'replace_with_backup',
          label: 'Заменить с резервной копией',
          description: 'Старый комплект сначала будет сохранён в отдельную резервную папку.',
          danger: true,
        },
      ],
      cancelLabel: 'Отмена',
    });
    if (choice === 'open_existing') {
      await run('open_in_file_manager', () => openInFileManager(target.patient_folder));
      setStatus('Открыт существующий комплект; новые документы не создавались.');
      return 'open_existing';
    }
    if (choice === 'replace_with_backup') return 'replace_with_backup';
    if (choice === 'create_new_version') return 'create_new_version';
    return null;
  }

  return {
    folderParts,
    supplementarySources,
    setSupplementarySources,
    updateFolderParts,
    ensureOutputNamingConfirmed,
    attachSupplementaryFiles,
    attachSupplementaryFolderByRole,
    removeSupplementary,
    outputConflictPolicy,
  };
}
