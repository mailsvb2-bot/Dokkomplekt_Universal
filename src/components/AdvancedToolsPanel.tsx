import { useEffect, useMemo, useState } from 'react';
import type {
  ClauseBlockRecord,
  DocumentTemplateSpec,
  MailMergeTable,
  ProcessBlueprintState,
  TemplateCandidateDto,
  TemplateLearningReport,
  TemplateMarkupCandidate,
  TemplateRegressionReport,
  TemplateVersionRecord,
} from '../lib/types';
import {
  analyzeTemplateFile,
  applyTemplateLearningMap,
  applyTemplateMarkup,
  checkTemplateRegression,
  confirmTemplateSetup,
  getProcessBlueprints,
  deleteClauseBlock,
  importLearningExampleFile,
  importTemplateFile,
  learnTemplateFromExamples,
  listClauseBlocks,
  listTemplateVersions,
  prepareMailMergeFile,
  prepareTemplateSetup,
  previewMailMerge,
  renderMailMerge,
  replaceClauseBlocks,
  registerLearnedTemplate,
  rollbackTemplateVersion,
  saveClauseBlock,
  selectProcessBlueprint,
  suggestTemplateMarkup,
  updateDocumentTemplate,
} from '../lib/api';
import { useActionRunner, labelledActionError } from '../hooks/useActionRunner';
import { STARTER_PACKS, type StarterPackAsset } from '../data/starterPacks';
import {
  MEDICAL_DIARY_FINAL_PREFIX,
  MEDICAL_DIARY_REGULAR_PREFIX,
  isFinalMedicalDiaryText,
  medicalDiagnosisKey,
  medicalDiaryFileKey,
  uniqueMedicalDiaryTexts,
} from '../lib/medicalDiarySources';

interface Props {
  documents: DocumentTemplateSpec[];
  selectedDocumentIds: string[];
  outputRoot: string;
  onStatus(message: string): void;
  onDocumentsChanged(documents: DocumentTemplateSpec[]): void;
}

const YEAR = new Date().getFullYear();

export function AdvancedToolsPanel({
  documents,
  selectedDocumentIds,
  outputRoot,
  onStatus,
  onDocumentsChanged,
}: Props) {
  const { busy, run: execute } = useActionRunner(onStatus, labelledActionError);
  const [blocks, setBlocks] = useState<ClauseBlockRecord[]>([]);
  const [templateVersions, setTemplateVersions] = useState<TemplateVersionRecord[]>([]);
  const [blockId, setBlockId] = useState('');
  const [blockTitle, setBlockTitle] = useState('');
  const [blockContent, setBlockContent] = useState('');
  const [diaryFinalDiagnosis, setDiaryFinalDiagnosis] = useState('');
  const [diaryFinalText, setDiaryFinalText] = useState('');
  const [tableText, setTableText] = useState('');
  const [table, setTable] = useState<MailMergeTable | null>(null);
  const [markupPath, setMarkupPath] = useState('');
  const [markupCandidates, setMarkupCandidates] = useState<TemplateMarkupCandidate[]>([]);
  const [selectedCandidates, setSelectedCandidates] = useState<string[]>([]);
  const [onboardingStep, setOnboardingStep] = useState<1 | 2 | 3>(1);
  const [dryReport, setDryReport] = useState('');
  const [installingPackId, setInstallingPackId] = useState('');
  const [processState, setProcessState] = useState<ProcessBlueprintState | null>(null);
  const [blankLearningFile, setBlankLearningFile] = useState<File | null>(null);
  const [completedLearningFiles, setCompletedLearningFiles] = useState<File[]>([]);
  const [sourceLearningFiles, setSourceLearningFiles] = useState<File[]>([]);
  const [learningLocale, setLearningLocale] = useState('ru-RU');
  const [learningBlankPath, setLearningBlankPath] = useState('');
  const [learningReport, setLearningReport] = useState<TemplateLearningReport | null>(null);
  const [selectedLearningFields, setSelectedLearningFields] = useState<string[]>([]);
  const [learnedTemplatePath, setLearnedTemplatePath] = useState('');
  const [learnedDocumentId, setLearnedDocumentId] = useState('');
  const [learnedButtonLabel, setLearnedButtonLabel] = useState('');
  const [replacementPath, setReplacementPath] = useState('');
  const [replacementRegression, setReplacementRegression] = useState<TemplateRegressionReport | null>(null);

  useEffect(() => {
    listClauseBlocks().then(setBlocks).catch(() => undefined);
    getProcessBlueprints().then(setProcessState).catch(() => undefined);
  }, []);

  const selectedDocuments = useMemo(
    () => documents.filter((document) => selectedDocumentIds.includes(document.id)),
    [documents, selectedDocumentIds],
  );
  const versionedDocument = selectedDocuments.length === 1 ? selectedDocuments[0] : null;
  const medicalAvailable = documents.some((document) => document.category === 'Medical');
  const medicalDiarySources = blocks.filter((block) =>
    block.block_id.startsWith(MEDICAL_DIARY_REGULAR_PREFIX) || block.block_id.startsWith(MEDICAL_DIARY_FINAL_PREFIX));

  useEffect(() => {
    if (!versionedDocument) {
      setTemplateVersions([]);
      return;
    }
    listTemplateVersions(versionedDocument.id)
      .then(setTemplateVersions)
      .catch(() => setTemplateVersions([]));
  }, [versionedDocument?.id]);

  async function chooseProcess(processId: string) {
    const result = await execute('выбор рабочего процесса', () => selectProcessBlueprint(processId));
    if (!result) return;
    setProcessState(result);
    const selected = result.processes.find((process) => process.process_id === result.selected_process_id);
    onStatus(selected
      ? `Выбран процесс «${selected.title}». Теперь загрузите свои формы и примеры.`
      : 'Рабочий процесс выбран.');
  }

  async function runTemplateLearning() {
    if (!blankLearningFile) {
      onStatus('Для обучения выберите пустой DOCX/DOCM-шаблон.');
      return;
    }
    if (completedLearningFiles.length < 3 || completedLearningFiles.length > 10) {
      onStatus('Для обучения нужны от 3 до 10 ранее заполненных DOCX/DOCM-примеров.');
      return;
    }
    const result = await execute('обучение пользовательского шаблона', async () => {
      const blankBytes = await readBytes(blankLearningFile);
      const blank = await importTemplateFile(`learning_blank_${Date.now()}`, {
        fileName: blankLearningFile.name,
        bytesBase64: toBase64(blankBytes),
      });
      const completedPaths: string[] = [];
      for (const [index, file] of completedLearningFiles.entries()) {
        const bytes = await readBytes(file);
        const imported = await importTemplateFile(`learning_completed_${Date.now()}_${index}`, {
          fileName: file.name,
          bytesBase64: toBase64(bytes),
        });
        completedPaths.push(imported.template_path);
      }
      const sourcePaths: string[] = [];
      for (const file of sourceLearningFiles.slice(0, 10)) {
        const bytes = await readBytes(file);
        const imported = await importLearningExampleFile(file.name, toBase64(bytes));
        sourcePaths.push(imported.source_path);
      }
      const report = await learnTemplateFromExamples({
        blankTemplatePath: blank.template_path,
        completedExamplePaths: completedPaths,
        sourceExamplePaths: sourcePaths,
        defaultYear: YEAR,
        locale: learningLocale,
      });
      return { blankPath: blank.template_path, report };
    });
    if (!result) return;
    setLearningBlankPath(result.blankPath);
    setLearningReport(result.report);
    setSelectedLearningFields(result.report.fields
      .filter((field) => field.confidence >= 0.6)
      .map((field) => field.field_id));
    setLearnedTemplatePath('');
    onStatus(`Сравнено ${completedLearningFiles.length} примеров. Найдено ${result.report.fields.length} переменных полей; карту нужно подтвердить.`);
  }

  function toggleLearningField(fieldId: string, checked: boolean) {
    setSelectedLearningFields((current) => checked
      ? [...new Set([...current, fieldId])]
      : current.filter((item) => item !== fieldId));
  }

  async function applyLearningMap() {
    if (!learningReport || !learningBlankPath) return;
    const confirmed = learningReport.fields
      .filter((field) => selectedLearningFields.includes(field.field_id))
      .map((field) => ({
        field_id: field.field_id,
        line_index: field.line_index,
        blank_line: field.blank_line,
        common_prefix: field.common_prefix,
        common_suffix: field.common_suffix,
      }));
    if (!confirmed.length) {
      onStatus('Подтвердите хотя бы одно найденное поле.');
      return;
    }
    const result = await execute('применение подтверждённой карты', () =>
      applyTemplateLearningMap(learningBlankPath, learnedOutputPath(learningBlankPath), confirmed));
    if (!result) return;
    setLearnedTemplatePath(result.output_path);
    onStatus(`Создана безопасная обученная копия. Вставлено полей: ${result.applied_field_ids.length}; вручную проверить: ${result.skipped_field_ids.length}.`);
  }

  async function publishLearnedTemplate() {
    if (!learnedTemplatePath || !learnedDocumentId.trim() || !learnedButtonLabel.trim()) {
      onStatus('Укажите идентификатор, название документа и сначала создайте обученную копию.');
      return;
    }
    const result = await execute('публикация обученного шаблона', () =>
      registerLearnedTemplate(learnedDocumentId.trim(), learnedButtonLabel.trim(), learnedTemplatePath));
    if (!result) return;
    onDocumentsChanged(result.documents);
    onStatus(`Кнопка «${learnedButtonLabel.trim()}» создана из подтверждённой карты. Тексты примеров не используются как источник смыслов.`);
  }

  async function inspectReplacement(file: File) {
    if (!versionedDocument) {
      onStatus('Сначала выберите ровно один документ слева.');
      return;
    }
    const result = await execute('проверка новой версии шаблона', async () => {
      const bytes = await readBytes(file);
      const imported = await importTemplateFile(`candidate_${versionedDocument.id}_${Date.now()}`, {
        fileName: file.name,
        bytesBase64: toBase64(bytes),
      });
      const regression = await checkTemplateRegression(versionedDocument.id, imported.template_path);
      return { path: imported.template_path, regression };
    });
    if (!result) return;
    setReplacementPath(result.path);
    setReplacementRegression(result.regression);
    onStatus(result.regression?.critical
      ? 'Новая версия содержит критические структурные изменения. Автопубликация заблокирована.'
      : 'Новая версия проверена. Критических структурных регрессий не найдено.');
  }

  async function publishReplacement(acknowledge: boolean) {
    if (!versionedDocument || !replacementPath) return;
    if (replacementRegression?.critical && !acknowledge) {
      onStatus('Критические изменения требуют отдельного явного подтверждения.');
      return;
    }
    const pack = await execute('публикация версии шаблона', () =>
      updateDocumentTemplate(versionedDocument.id, replacementPath, acknowledge));
    if (!pack) return;
    onDocumentsChanged(pack.documents);
    setReplacementPath('');
    setReplacementRegression(null);
    onStatus(`Новая версия «${versionedDocument.button_label}» опубликована после структурной проверки.`);
    const versions = await execute('история шаблона', () => listTemplateVersions(versionedDocument.id));
    if (versions) setTemplateVersions(versions);
  }

  async function saveBlock() {
    const result = await execute('библиотека блоков', () => saveClauseBlock(blockId, blockTitle, blockContent));
    if (result) {
      setBlocks(result);
      onStatus(`Блок {{block ${blockId.trim()}}} сохранён локально.`);
    }
  }

  async function removeBlock(id: string) {
    const result = await execute('удаление блока', () => deleteClauseBlock(id));
    if (result) setBlocks(result);
  }

  async function importMedicalDiaryTexts(files: File[]) {
    if (!files.length) return;
    const result = await execute('импорт текстов дневников', async () => {
      const buckets = new Map<string, { regular: string[]; final: string[] }>();
      let imported = 0;
      for (const file of files) {
        const supported = /\.(txt|docx|docm)$/i.test(file.name);
        if (!supported) continue;
        const key = medicalDiaryFileKey(file.name);
        const content = /\.txt$/i.test(file.name)
          ? (await file.text()).trim()
          : (await importLearningExampleFile(file.name, toBase64(await readBytes(file)))).extracted_text.trim();
        if (!key) continue;
        if (!content) {
          throw new Error(`Файл «${file.name}» прочитан, но не содержит текста; набор дневников не изменён.`);
        }
        const bucket = buckets.get(key) ?? { regular: [], final: [] };
        const role = isFinalMedicalDiaryText(file.name) ? bucket.final : bucket.regular;
        role.push(content);
        buckets.set(key, bucket);
        imported += 1;
      }
      if (!buckets.size) return { current: blocks, imported };

      const deleteBlockIds: string[] = [];
      const replacements: Array<{ blockId: string; title: string; content: string }> = [];
      for (const [key, bucket] of buckets) {
        deleteBlockIds.push(
          `${MEDICAL_DIARY_REGULAR_PREFIX}${key}`,
          `${MEDICAL_DIARY_FINAL_PREFIX}${key}`,
        );
        replacements.push(
          {
            blockId: `${MEDICAL_DIARY_REGULAR_PREFIX}${key}`,
            title: `Тексты дневников: ${key}`,
            content: uniqueMedicalDiaryTexts(bucket.regular).join('\n\n'),
          },
          {
            blockId: `${MEDICAL_DIARY_FINAL_PREFIX}${key}`,
            title: `Итоговый дневник: ${key}`,
            content: uniqueMedicalDiaryTexts(bucket.final).join('\n\n'),
          },
        );
      }
      await replaceClauseBlocks(deleteBlockIds, replacements);
      return { current: await listClauseBlocks(), imported };
    });
    if (!result) return;
    setBlocks(result.current);
    onStatus(`Импортировано источников текстов дневников: ${result.imported}. Имя файла или код МКБ-10 в имени используется для привязки к диагнозу; данные сохранены локально атомарным набором.`);
  }

  async function saveMedicalFinalDiary() {
    const diagnosis = diaryFinalDiagnosis.trim();
    const key = medicalDiagnosisKey(diagnosis);
    if (!key || !diaryFinalText.trim()) {
      onStatus('Для итогового дневника укажите диагноз и подтверждённый специалистом текст.');
      return;
    }
    const result = await execute('сохранение итогового дневника', () => saveClauseBlock(
      `${MEDICAL_DIARY_FINAL_PREFIX}${key}`,
      `Итоговый дневник: ${diagnosis}`,
      diaryFinalText.trim(),
    ));
    if (!result) return;
    setBlocks(result);
    onStatus(`Итоговый дневник для ${diagnosis} сохранён локально и будет использоваться только в медицинском профиле.`);
  }

  async function inspectTable() {
    const result = await execute('проверка таблицы', () => previewMailMerge(tableText));
    if (result) setTable(result);
  }

  async function loadDataFile(file: File) {
    const result = await execute('проверка таблицы', async () => {
      const bytes = await readBytes(file);
      return prepareMailMergeFile(file.name, toBase64(bytes));
    });
    if (result) {
      setTableText(result.delimited_text);
      setTable(result.table);
      onStatus(`Таблица ${file.name} прочитана: ${result.table.rows.length} строк.`);
    }
  }

  async function generateTable() {
    if (!selectedDocumentIds.length) {
      onStatus('Для пакетной генерации отметьте документы в левой панели.');
      return;
    }
    const result = await execute('пакетная генерация', () =>
      renderMailMerge(selectedDocumentIds, tableText, outputRoot || 'output/Готовые документы', true));
    if (result) {
      onStatus(result.warnings?.length
        ? `Пакетная генерация: ${result.row_count} комплектов, ${result.created_files.length} файлов. Требует внимания: ${result.warnings.join(' ')}`
        : `Пакетная генерация: ${result.row_count} комплектов, ${result.created_files.length} файлов.`);
    }
  }

  async function inspectTemplate(file: File) {
    if (!/\.doc[xm]$/i.test(file.name)) {
      onStatus('Мастер принимает DOCX и DOCM.');
      return;
    }
    const result = await execute('загрузка и авторазметка шаблона', async () => {
      const bytes = await readBytes(file);
      const encoded = toBase64(bytes);
      const imported = await importTemplateFile(`wizard_${Date.now()}`, { fileName: file.name, bytesBase64: encoded });
      const candidates = await suggestTemplateMarkup(file.name, encoded, YEAR);
      return { imported, candidates };
    });
    if (!result) return;
    setMarkupPath(result.imported.template_path);
    setMarkupCandidates(result.candidates);
    setSelectedCandidates(result.candidates.filter((candidate) => candidate.selected_by_default).map(candidateKey));
    setOnboardingStep(2);
    setDryReport('');
    onStatus(`Найдено ${result.candidates.length} кандидатов. Замена выполняется только после подтверждения.`);
  }

  function toggle(candidate: TemplateMarkupCandidate, checked: boolean) {
    const key = candidateKey(candidate);
    setSelectedCandidates((current) => checked
      ? [...new Set([...current, key])]
      : current.filter((item) => item !== key));
  }

  async function rollbackVersion(item: TemplateVersionRecord) {
    const pack = await execute('rollback шаблона', () => rollbackTemplateVersion(item.version_id));
    if (!pack) return;
    onDocumentsChanged(pack.documents);
    onStatus(`Шаблон «${versionedDocument?.button_label || item.document_id}» возвращён к версии ${item.version_number}. Создана новая опубликованная версия с контрольным SHA-256.`);
    const versions = await execute('история шаблона', () => listTemplateVersions(item.document_id));
    if (versions) setTemplateVersions(versions);
  }

  async function applyMarkup() {
    const replacements = markupCandidates
      .filter((candidate) => selectedCandidates.includes(candidateKey(candidate)))
      .map((candidate) => ({ field_id: candidate.field_id, value: candidate.value }));
    if (!markupPath || !replacements.length) {
      onStatus('Выберите хотя бы одно значение.');
      return;
    }
    const result = await execute('применение авторазметки', () =>
      applyTemplateMarkup(markupPath, markedOutputPath(markupPath), replacements));
    if (result) {
      setMarkupPath(result.output_path);
      setOnboardingStep(3);
      setDryReport('');
      onStatus(`Размеченный файл: ${result.output_path}. Заменено: ${result.replaced_occurrences}.`);
    }
  }

  async function runDryCheck() {
    if (!markupPath) {
      onStatus('Сначала создайте размеченную копию.');
      return;
    }
    const result = await execute('сухой прогон шаблона', () =>
      analyzeTemplateFile(markupPath, `onboarding_${Date.now()}`));
    if (!result) return;
    const document = result.document;
    const role = document.role_id && document.role_id !== 'unknown' ? document.role_id : 'не определена';
    const fields = [...new Set([...document.placeholders, ...document.required_fields])];
    const report = `Роль: ${role}. Полей: ${fields.length}. Уточняющих вопросов: ${document.popup_fields?.length ?? 0}. Режим: ${document.is_static_copy ? 'статическая копия' : 'динамический DOCX'}.`;
    setDryReport(report);
    onStatus(`Сухой прогон завершён. ${report}`);
  }

  async function installStarterPack(pack: StarterPackAsset) {
    if (pack.usageMode !== 'draft_only') {
      onStatus('Установка заблокирована: starter-пак должен быть явно помечен draft_only.');
      return;
    }
    setInstallingPackId(pack.id);
    const documentPack = await execute(`установка starter-пака ${pack.name}`, async () => {
      const verifiedAssets: Array<{ template: StarterPackAsset['templates'][number]; bytes: ArrayBuffer }> = [];
      for (const template of pack.templates) {
        const response = await fetch(template.url, { cache: 'no-store' });
        if (!response.ok) throw new Error(`не удалось прочитать ${template.fileName}: HTTP ${response.status}`);
        const bytes = await response.arrayBuffer();
        const actualSha256 = await sha256Hex(bytes);
        if (actualSha256 !== template.sha256.toLowerCase()) {
          throw new Error(`SHA-256 не совпал для ${template.fileName}`);
        }
        verifiedAssets.push({ template, bytes });
      }
      const candidates: TemplateCandidateDto[] = [];
      for (const { template, bytes } of verifiedAssets) {
        const imported = await importTemplateFile(template.documentId, {
          fileName: template.fileName,
          bytesBase64: toBase64(bytes),
        });
        candidates.push({
          document_id: template.documentId,
          template_path: imported.template_path,
          extracted_text: imported.extracted_text,
          preferred_button_label: template.label,
        });
      }
      const rows = await prepareTemplateSetup(candidates);
      if (rows.some((row) => row.is_static_copy)) {
        throw new Error('один из starter-шаблонов не содержит канонических полей');
      }
      const labels = new Map(pack.templates.map((item) => [item.documentId, item.label]));
      return confirmTemplateSetup(rows.map((row) => ({
        ...row,
        editable_button_label: labels.get(row.document_id) ?? row.editable_button_label,
      })));
    });
    setInstallingPackId('');
    if (!documentPack) return;
    onDocumentsChanged(documentPack.documents);
    onStatus(`Starter-пак «${pack.name}» установлен: ${pack.templates.length} шаблонов. Формы работают только как черновики до проверки организацией.`);
  }


  return (
    <div className="advancedTools">
      <section className="utilityCard advancedCard processBlueprintCard">
        <strong>1. Выберите профессию или рабочий процесс</strong>
        <small>{processState?.notice ?? 'Выбор процесса настраивает ожидаемый комплект и проверки, но не подменяет ваши утверждённые формы.'}</small>
        <select
          value={processState?.selected_process_id ?? ''}
          disabled={busy || !processState}
          onChange={(event) => void chooseProcess(event.target.value)}
        >
          <option value="" disabled>Выберите процесс…</option>
          {processState?.processes?.map((process) => (
            <option key={process.process_id} value={process.process_id}>{process.title} · {process.locale}</option>
          ))}
        </select>
        {processState?.selected_process_id && (() => {
          const process = processState.processes?.find((item) => item.process_id === processState.selected_process_id);
          return process ? <div className="processBlueprintSummary"><b>{process.title}</b><small>{process.description}</small><small>Ожидаемые формы: {process.template_slots.join(', ')}</small></div> : null;
        })()}
      </section>

      <section className="utilityCard advancedCard templateLearningCard">
        <strong>2. Научить программу вашим шаблонам</strong>
        <small>Загрузите пустой шаблон, 3–10 ранее заполненных копий и, при наличии, исходные документы. Программа сравнит их, отделит постоянный текст от переменных значений и покажет карту до публикации.</small>
        <div className="learningUploadGrid">
          <label className="fileBtn">Пустой DOCX/DOCM
            <input hidden type="file" accept=".docx,.docm" onChange={(event) => { setBlankLearningFile(event.target.files?.[0] ?? null); event.currentTarget.value = ''; }} />
          </label>
          <span>{blankLearningFile?.name ?? 'не выбран'}</span>
          <label className="fileBtn">3–10 заполненных примеров
            <input hidden multiple type="file" accept=".docx,.docm" onChange={(event) => { setCompletedLearningFiles(Array.from(event.target.files ?? []).slice(0, 10)); event.currentTarget.value = ''; }} />
          </label>
          <span>{completedLearningFiles.length ? completedLearningFiles.map((file) => file.name).join(', ') : 'не выбраны'}</span>
          <label className="fileBtn">Исходные документы, необязательно
            <input hidden multiple type="file" accept=".docx,.docm,.doc,.ppt,.pptx,.pdf,.jpg,.jpeg,.png,.tif,.tiff,.bmp,.webp,.xlsx,.xls,.ods,.odt,.rtf,.txt,.md,.csv,.tsv,.json,.xml,.html,.htm,.eml,.msg,.zip,.7z,.rar" onChange={(event) => { setSourceLearningFiles(Array.from(event.target.files ?? []).slice(0, 10)); event.currentTarget.value = ''; }} />
          </label>
          <span>{sourceLearningFiles.length ? `${sourceLearningFiles.length} файл(ов)` : 'не выбраны'}</span>
        </div>
        <label>Язык примеров
          <select value={learningLocale} onChange={(event) => setLearningLocale(event.target.value)}>
            <option value="ru-RU">Русский</option>
            <option value="en-US">English</option>
            <option value="de-DE">Deutsch</option>
            <option value="fr-FR">Français</option>
            <option value="es-ES">Español</option>
            <option value="kk-KZ">Қазақша</option>
            <option value="uk-UA">Українська</option>
            <option value="auto">Автоопределение</option>
          </select>
        </label>
        <button className="utilBtn" disabled={busy || !blankLearningFile || completedLearningFiles.length < 3} onClick={() => void runTemplateLearning()}>Сравнить примеры и предложить карту</button>
        {learningReport && (
          <div className="learningReport">
            <div className="rowBetween"><b>Предложенная карта · уверенность {Math.round(learningReport.confidence * 100)}%</b><small>Публикация без подтверждения запрещена</small></div>
            {learningReport.fields.map((field) => (
              <label key={field.field_id} className="learningField">
                <input type="checkbox" checked={selectedLearningFields.includes(field.field_id)} onChange={(event) => toggleLearningField(field.field_id, event.target.checked)} />
                <span><b>{field.title}</b> <code>{field.placeholder}</code><small>строка {field.line_index + 1} · {Math.round(field.confidence * 100)}% · примеры: {field.example_values.slice(0, 3).join(' / ') || 'нет'}{field.condition ? ` · условие: ${field.condition}` : ''}</small></span>
              </label>
            ))}
            {!!learningReport.diff.length && <details><summary>Визуальный diff переменных строк</summary>{learningReport.diff.map((hunk) => <div key={hunk.line_index} className="learningDiff"><b>Строка {hunk.line_index + 1}</b><del>{hunk.blank_line || 'пусто'}</del><ins>{hunk.common_prefix}…{hunk.common_suffix}</ins><small>{hunk.example_lines.slice(0, 4).join(' | ')}</small></div>)}</details>}
            {!!learningReport.warnings.length && <small className="badgeWarn">{learningReport.warnings.join('; ')}</small>}
            <button className="utilBtn" disabled={busy || !selectedLearningFields.length} onClick={() => void applyLearningMap()}>Подтвердить карту и создать копию</button>
          </div>
        )}
        {learnedTemplatePath && (
          <div className="learnedPublish">
            <input value={learnedDocumentId} onChange={(event) => setLearnedDocumentId(event.target.value)} placeholder="идентификатор: document.custom" />
            <input value={learnedButtonLabel} onChange={(event) => setLearnedButtonLabel(event.target.value)} placeholder="название документа" />
            <button className="primaryBtn" disabled={busy || !learnedDocumentId.trim() || !learnedButtonLabel.trim()} onClick={() => void publishLearnedTemplate()}>Добавить документ в набор</button>
          </div>
        )}
      </section>

      <section className="utilityCard advancedCard starterPacksCard">
        <strong>Starter-паки для пилота</strong>
        <small>Работающие DOCX-каркасы для проверки полного контура. Они не являются утверждёнными юридическими или кадровыми формами и устанавливаются только в режиме draft-only.</small>
        <div className="starterPackGrid">
          {STARTER_PACKS.map((pack) => (
            <article key={pack.id}>
              <b>{pack.name}</b>
              <small>{pack.templates.length} шаблонов · обязательная проверка организацией</small>
              <button
                className="utilBtn"
                disabled={busy}
                onClick={() => void installStarterPack(pack)}
              >
                {installingPackId === pack.id ? 'Проверка и импорт…' : 'Установить starter-пак'}
              </button>
            </article>
          ))}
        </div>
      </section>

      {medicalAvailable && (
        <section className="utilityCard advancedCard">
          <strong>Медицина · источники дневников</strong>
          <small>Совместимость с diary-filler: выберите TXT, DOCX или DOCM с пользовательскими текстами дневников. Имя файла или код МКБ-10 в имени используется для привязки к диагнозу; чужой диагноз не подмешивается.</small>
          <label className="utilBtn fileButton">
            Импортировать «Тексты» (TXT/DOCX/DOCM)
            <input
              type="file"
              accept=".txt,.docx,.docm,text/plain"
              multiple
              hidden
              onChange={(event) => { void importMedicalDiaryTexts(Array.from(event.currentTarget.files ?? [])); event.currentTarget.value = ''; }}
            />
          </label>
          <input value={diaryFinalDiagnosis} onChange={(event) => setDiaryFinalDiagnosis(event.target.value)} placeholder="диагноз для итогового дневника, например F20.0" />
          <textarea value={diaryFinalText} onChange={(event) => setDiaryFinalText(event.target.value)} placeholder="подтверждённый специалистом итоговый дневник" />
          <button disabled={busy || !diaryFinalDiagnosis.trim() || !diaryFinalText.trim()} className="utilBtn" onClick={saveMedicalFinalDiary}>Сохранить итоговый дневник</button>
          {medicalDiarySources.length > 0 && (
            <div className="advancedList">
              {medicalDiarySources.map((block) => (
                <div key={block.block_id} className="advancedListRow">
                  <span>{block.title || block.block_id}</span>
                  <button disabled={busy} className="utilBtn danger" onClick={() => void removeBlock(block.block_id)}>Удалить</button>
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <section className="utilityCard advancedCard">
        <strong>Библиотека блоков</strong>
        <small>{'Вставка: {{block requisites_ooo}}'}</small>
        <input value={blockId} onChange={(event) => setBlockId(event.target.value)} placeholder="идентификатор блока" />
        <input value={blockTitle} onChange={(event) => setBlockTitle(event.target.value)} placeholder="название" />
        <textarea value={blockContent} onChange={(event) => setBlockContent(event.target.value)} placeholder="текст блока с условиями и полями" />
        <button disabled={busy || !blockId.trim() || !blockContent.trim()} className="utilBtn" onClick={saveBlock}>Сохранить блок</button>
        <div className="compactList">
          {blocks.map((block) => (
            <div key={block.block_id}>
              <button className="linkBtn" onClick={() => { setBlockId(block.block_id); setBlockTitle(block.title); setBlockContent(block.content); }}>{block.title || block.block_id}</button>
              <button className="iconBtn" aria-label={`Удалить ${block.block_id}`} onClick={() => void removeBlock(block.block_id)}>×</button>
            </div>
          ))}
        </div>
      </section>

      <section className="utilityCard advancedCard onboardingWizard">
        <strong>Мастер первого запуска: 3 шага</strong>
        <small>Импорт → подтверждаемая авторазметка → сухой прогон без печати. Исходный DOCX/DOCM не изменяется.</small>
        <div className="wizardSteps">
          <b className={onboardingStep === 1 ? 'active' : ''}>1. Выбрать шаблон</b>
          <b className={onboardingStep === 2 ? 'active' : ''}>2. Подтвердить поля</b>
          <b className={onboardingStep === 3 ? 'active' : ''}>3. Проверить сценарий</b>
        </div>
        <label className="fileBtn">Выбрать DOCX/DOCM
          <input hidden type="file" accept=".docx,.docm" onChange={(event) => { const file = event.target.files?.[0]; if (file) void inspectTemplate(file); event.currentTarget.value = ''; }} />
        </label>
        <div className="candidateList">
          {markupCandidates.map((candidate) => (
            <label key={candidateKey(candidate)}>
              <input type="checkbox" checked={selectedCandidates.includes(candidateKey(candidate))} onChange={(event) => toggle(candidate, event.target.checked)} />
              <span><b>{candidate.title}</b>: {candidate.value} · {Math.round(candidate.confidence * 100)}%</span>
            </label>
          ))}
        </div>
        <div className="inlineButtons">
          <button disabled={busy || !markupCandidates.length} className="utilBtn" onClick={() => void applyMarkup()}>Создать размеченную копию</button>
          <button disabled={busy || onboardingStep !== 3 || !markupPath} className="softBtn" onClick={() => void runDryCheck()}>Сухой прогон без печати</button>
        </div>
        {dryReport && <small className="dryRunReport">✓ {dryReport}</small>}
      </section>

      <section className="utilityCard advancedCard">
        <strong>Версии пользовательского шаблона</strong>
        <small>Выберите ровно один документ слева. Каждая публикация получает SHA-256 и номер; предыдущая версия остаётся доступной для безопасного rollback.</small>
        {!versionedDocument ? <small>Для просмотра истории отметьте один документ.</small> : (
          <>
            <label className="fileBtn">Проверить новую DOCX/DOCM-версию
              <input hidden type="file" accept=".docx,.docm" onChange={(event) => { const file = event.target.files?.[0]; if (file) void inspectReplacement(file); event.currentTarget.value = ''; }} />
            </label>
            {replacementRegression && (
              <div className={`regressionReport ${replacementRegression.critical ? 'critical' : 'safe'}`}>
                <b>{replacementRegression.critical ? 'Критические изменения' : 'Структурная проверка пройдена'}</b>
                {replacementRegression.issues.length ? <ul>{replacementRegression.issues.map((issue) => <li key={`${issue.code}:${issue.message}`}><b>{issue.severity}</b> · {issue.message}</li>)}</ul> : <small>Изменений, влияющих на workflow, placeholders, таблицы, секции, headers/footers и page breaks, не найдено.</small>}
                <button className={replacementRegression.critical ? 'dangerBtn' : 'utilBtn'} disabled={busy} onClick={() => void publishReplacement(replacementRegression.critical)}>
                  {replacementRegression.critical ? 'Я проверил изменения — опубликовать' : 'Опубликовать новую версию'}
                </button>
              </div>
            )}
            <div className="compactList templateVersionList">
            {templateVersions.length === 0 && <small>История появится после первой проверенной публикации шаблона.</small>}
            {templateVersions.map((item) => (
              <div key={item.version_id}>
                <span><b>v{item.version_number}</b> · {item.status}<small>{item.note} · {item.template_sha256.slice(0, 12)}…</small></span>
                {item.status !== 'published' && <button className="softBtn" disabled={busy} onClick={() => void rollbackVersion(item)}>Вернуть</button>}
              </div>
            ))}
            </div>
          </>
        )}
      </section>

      <section className="utilityCard advancedCard">
        <strong>Пакетная генерация XLSX/CSV/TSV</strong>
        <small>Строка = отдельный комплект. Для XLSX безопасно читается первый лист; формулы берутся по сохранённым значениям. Заголовки: field.id или привычные названия.</small>
        <label className="fileBtn">Загрузить XLSX/CSV/TSV
          <input hidden type="file" accept=".xlsx,.csv,.tsv,.txt" onChange={(event) => { const file = event.target.files?.[0]; if (file) void loadDataFile(file); event.currentTarget.value = ''; }} />
        </label>
        <textarea value={tableText} onChange={(event) => setTableText(event.target.value)} placeholder={'Наименование;document.number\nПример;Д-001'} />
        {table && <small>Распознано: {table.rows.length} строк · {table.canonical_headers.join(', ')}</small>}
        <small>Выбрано документов: {selectedDocuments.map((document) => document.button_label).join(', ') || 'нет'}</small>
        <div className="inlineButtons">
          <button disabled={busy || !tableText.trim()} className="softBtn" onClick={() => void inspectTable()}>Проверить</button>
          <button disabled={busy || !tableText.trim() || !selectedDocumentIds.length} className="utilBtn" onClick={() => void generateTable()}>Создать комплекты</button>
        </div>
      </section>
    </div>
  );
}

function candidateKey(candidate: TemplateMarkupCandidate) {
  return `${candidate.field_id}\u0000${candidate.value}`;
}

function learnedOutputPath(path: string) {
  const match = path.match(/\.(docx|docm)$/i);
  return match ? `${path.slice(0, -match[0].length)}.learned${match[0]}` : `${path}.learned.docx`;
}

function markedOutputPath(path: string) {
  const match = path.match(/\.(docx|docm)$/i);
  return match ? `${path.slice(0, -match[0].length)}.marked${match[0]}` : `${path}.marked.docx`;
}


function readBytes(file: File): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer();
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as ArrayBuffer);
    reader.onerror = () => reject(reader.error ?? new Error('file read failed'));
    reader.readAsArrayBuffer(file);
  });
}

function toBase64(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  }
  return btoa(binary);
}

async function sha256Hex(buffer: ArrayBuffer) {
  if (!globalThis.crypto?.subtle) throw new Error('WebCrypto SHA-256 недоступен');
  const digest = await globalThis.crypto.subtle.digest('SHA-256', buffer);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
