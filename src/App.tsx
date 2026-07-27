import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { CreatedDocumentOutput, CreatedDocumentsIntakeResult, GeneratedOutput, GeneratedPrintItem, IntakeCapability, PrintJobDto, PrintTriageReport, SemanticExtractResult, DocumentRoutingRecommendation, DocumentTemplateSpec, DomainKind, FolderNamePartDto, GuidedScannerMarkupAction, Icd10Suggestion, LearnedScannerRule, PopupFieldConfig, PromptSpec, WordScannerCapture, WordScannerSession, WorkflowPlan } from './lib/types';
import {
  activateWordScanner, analyzeTemplate, analyzeTemplateFile, applyPopup, applyPopupBatch, applyScanner, applyTemplateMarkup, applyWordScannerSelection, captureWordScanner, closeWordScanner, confirmTemplateSetup, firstRunState,
  getRecordSeriesPlan, getDocumentTemplateText, getIntakeCapabilities, getComponentStatuses, installComponent, getOutputPlan, getWorkflowPlan, getWorkflowPlanBatch, icd10Suggest, installBackgroundWatcher, loadState, parseSource, parseSourceFile, parseWebSource,
  approveDocumentTemplate, createKedoPackage, exportFilesToPdf, getPrintTriage, importTemplateFile, listLearnedScannerRules, openInFileManager, prepareTemplateSetup, printFiles, removeDocumentButton, renameDocumentButton, renderDocx, renderDocxBatch, renderPreview, resetCase, runCreatedDocumentsIntake, saveLearnedScannerRule, semanticExtract, saveState, setField, startWordScanner, uninstallBackgroundWatcher, updateBackgroundWatcherPreferences, updateDocumentPopupFields, updateDocumentTemplate,
  checkForUpdates, validateProductAccess, verifyRustLicenseText,
} from './lib/api';
import { ThemeSwitcher } from './components/ThemeSwitcher';
import { UtilityPanel } from './components/UtilityPanel';
import { TemplateSetupModal } from './components/TemplateSetupModal';
import { DocumentRail } from './components/DocumentRail';
import { Workspace } from './components/Workspace';
import { PopupDesignerModal } from './components/PopupDesignerModal';
import { RuntimePromptModal } from './components/RuntimePromptModal';
import { GuidedScannerModal } from './components/GuidedScannerModal';
import { ensurePopupField, newPopupField } from './components/PopupFieldEditor';
import { bestScannerSuggestion, suggestScannerFields, type ScannerFieldSuggestion } from './lib/scannerSuggestions';
import { applyTheme, buildTheme, loadTheme, saveTheme, type ThemeState } from './theme';

const DEFAULT_YEAR = new Date().getFullYear();
const STATE_DB = 'dokkomplekt-user-state.sqlite';
const OUTPUT_PREFS_KEY = 'dokkomplekt.output-folder-parts.v1';
const AUTO_PRINT_KEY = 'dokkomplekt.auto-print.v1';
const PRINT_COPIES_KEY = 'dokkomplekt.print-copies.v1';

type PendingTemplate = {
  document_id: string;
  template_path: string;
  extracted_text: string;
  file_name: string;
  button_label: string;
  popup_fields: PopupFieldConfig[];
};

type PendingGeneration = {
  kind: 'single' | 'batch';
  documentIds: string[];
};

type GuidedScannerTarget =
  | { mode: 'source'; documentId: string | null; label: string | null; domain: DomainKind | null }
  | { mode: 'template'; kind: 'pending' | 'existing'; documentId: string; label: string; domain: DomainKind | null };

type GuidedScannerState = {
  session: WordScannerSession;
  target: GuidedScannerTarget;
  capture: WordScannerCapture | null;
  suggestions: ScannerFieldSuggestion[];
  selectedFieldId: string;
  rememberRule: boolean;
  addQuestion: boolean;
  markupAction: GuidedScannerMarkupAction;
};


export function App() {
  const [theme, setTheme] = useState<ThemeState>(() => loadTheme());
  useEffect(() => { applyTheme(buildTheme(theme)); saveTheme(theme); }, [theme]);

  const [documents, setDocuments] = useState<DocumentTemplateSpec[]>([]);
  const [activeDoc, setActiveDoc] = useState<string | null>(null);
  const [selectedDocIds, setSelectedDocIds] = useState<string[]>([]);
  const [status, setStatus] = useState('Добавьте исходный файл — остальное программа подготовит сама.');
  const [busy, setBusy] = useState(false);

  const [sourceText, setSourceText] = useState('');
  const [sourceFileName, setSourceFileName] = useState<string | null>(null);
  const [sourceFilePath, setSourceFilePath] = useState<string | null>(null);
  const [webSourceUrl, setWebSourceUrl] = useState('');
  const [intakeCapabilities, setIntakeCapabilities] = useState<IntakeCapability[]>([]);
  const [parsed, setParsed] = useState<{
    title: string;
    count: number;
    warnings: string[];
    sourceKind?: string;
    layoutRows?: number;
    tableRows?: number;
  } | null>(null);

  const [plan, setPlan] = useState<WorkflowPlan | null>(null);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [sickLeave, setSickLeave] = useState(false);

  const [activeTemplateText, setActiveTemplateText] = useState('');
  const [preview, setPreview] = useState<{ text: string; missing: number } | null>(null);

  const [setupOpen, setSetupOpen] = useState(false);
  const [templateText, setTemplateText] = useState('');
  const [buttonLabel, setButtonLabel] = useState('');
  // Абсолютный путь DOCX в app_data/user-templates, куда Rust сохранил шаблон
  // (выбранный файл или сгенерированный из вставленного текста).
  const [importedTemplatePath, setImportedTemplatePath] = useState<string | null>(null);
  const [pendingTemplates, setPendingTemplates] = useState<PendingTemplate[]>([]);
  const [draftPopupFields, setDraftPopupFields] = useState<PopupFieldConfig[]>([]);
  const [popupDesignerDocument, setPopupDesignerDocument] = useState<DocumentTemplateSpec | null>(null);
  const [popupDesignerFields, setPopupDesignerFields] = useState<PopupFieldConfig[]>([]);
  const [runtimePrompt, setRuntimePrompt] = useState<{ plan: WorkflowPlan; generation: PendingGeneration; title: string } | null>(null);
  const [runtimeAnswers, setRuntimeAnswers] = useState<Record<string, string>>({});
  const [runtimeMessage, setRuntimeMessage] = useState('');

  const [icdQuery, setIcdQuery] = useState('');
  const [icdHits, setIcdHits] = useState<Icd10Suggestion[]>([]);

  const [licenseText, setLicenseText] = useState('');
  const [utilityOpen, setUtilityOpen] = useState(false);

  const [watchFolder, setWatchFolder] = useState('Созданные документы');
  const [intakeSource, setIntakeSource] = useState('');
  const [intakeResult, setIntakeResult] = useState<CreatedDocumentsIntakeResult | null>(null);
  const [semantic, setSemantic] = useState<SemanticExtractResult | null>(null);
  const [modelOutput, setModelOutput] = useState('');
  const [seriesStart, setSeriesStart] = useState('');
  const [seriesEnd, setSeriesEnd] = useState('');
  const [seriesSkipWeekends, setSeriesSkipWeekends] = useState(false);
  const [scannerField, setScannerField] = useState('');
  const [scannerText, setScannerText] = useState('');
  const [outputRoot, setOutputRoot] = useState('output/Готовые документы');
  const [folderParts, setFolderParts] = useState<FolderNamePartDto[]>(loadOutputFolderParts);
  const [autoPrint, setAutoPrint] = useState(loadAutoPrintPreference);
  const [printCopies, setPrintCopies] = useState<Record<string, number>>(loadPrintCopyPreferences);
  const [lastOutput, setLastOutput] = useState<GeneratedOutput | null>(null);
  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const res = await firstRunState();
        if (!alive) return;
        if (res?.pack?.documents?.length) {
          setDocuments(res.pack.documents);
          setSelectedDocIds(res.pack.documents.map((document) => document.id));
          setStatus(`Рабочий набор готов: ${res.pack.documents.length} документ(ов). Добавьте исходный файл.`);
        } else if (res?.has_user_buttons === false) {
          setStatus('Нажмите «Создать свои кнопки» и выберите ваши шаблоны Word.');
        } else if (res?.message) {
          setStatus(res.message);
        }
      } catch { /* no backend in browser/tests — start empty */ }
    })();
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    let alive = true;
    void getIntakeCapabilities()
      .then((items) => { if (alive) setIntakeCapabilities(items); })
      .catch(() => { /* browser/tests */ });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    void updateBackgroundWatcherPreferences(autoPrint, printCopies).catch(() => {
      // No persisted watcher plan yet: local preferences remain valid and will be
      // written when the watcher is installed.
    });
  }, [autoPrint, printCopies]);

  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    listen<CreatedDocumentsIntakeResult>('document-batch-ready', (event) => {
      const result = event.payload;
      setIntakeResult(result);
      setStatus(result.message);
      if (result.status === 'processed' && result.created_files.length) {
        setLastOutput({
          folder: result.patient_folder,
          files: result.created_files,
          source: 'watcher',
          print_items: createdPrintItems(result.created_documents, result.created_files, documents),
        });
      }
    }).then((unlisten) => {
      if (disposed) unlisten(); else stopListening = unlisten;
    }).catch(() => { /* browser/tests: Tauri event bridge is unavailable */ });
    return () => {
      disposed = true;
      stopListening?.();
    };
  }, [documents]);

  useEffect(() => {
    if (!setupOpen) return;
    function onKey(e: KeyboardEvent) { if (e.key === 'Escape') setSetupOpen(false); }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [setupOpen]);

  const visibleDocs = documents;

  useEffect(() => {
    const existing = new Set(documents.map((document) => document.id));
    setSelectedDocIds((previous) => previous.filter((id) => existing.has(id)));
  }, [documents]);

  const previewTitle = detectTitle(templateText) || 'Документ';
  const previewLabel = buttonLabel.trim() || previewTitle;

  async function run<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    try {
      return await fn();
    } catch (err) {
      setStatus(`Не удалось выполнить действие: ${errorMessage(err)}`);
      return undefined;
    } finally {
      setBusy(false);
    }
  }

  async function ensureOptionalComponent(id: string, fallbackLabel: string): Promise<boolean> {
    const statuses = await run('get_component_statuses', () => getComponentStatuses());
    if (!statuses) return false;
    const component = statuses.find(item => item.id === id);
    if (component?.available || component?.installed) return true;
    const label = component?.label || fallbackLabel;
    const size = component?.size_label || 'размер будет показан после проверки подписанного каталога';
    const accepted = globalThis.confirm?.(`${label} отсутствует. Скачать ${size}?\n\nРазовая загрузка; после установки компонент работает офлайн.`) ?? false;
    if (!accepted) {
      setStatus(`${label}: пользователь отказался от загрузки; функция не запущена.`);
      return false;
    }
    const installed = await run('install_component', () => installComponent(id));
    if (!installed?.installed) return false;
    setStatus(`${installed.label}: компонент установлен и доступен офлайн.`);
    return true;
  }

  async function ensureComponentForSource(fileName: string): Promise<boolean> {
    const extension = fileName.split('.').at(-1)?.toLowerCase() || '';
    if (['pdf', 'jpg', 'jpeg', 'png', 'tif', 'tiff', 'bmp', 'webp'].includes(extension)) {
      return ensureOptionalComponent('ocr', 'Распознавание сканов (OCR)');
    }
    if (['xls', 'ods'].includes(extension)) {
      return ensureOptionalComponent('office', 'Конвертация Office-файлов');
    }
    return true;
  }

  async function resetCurrentCase() {
    const cleared = await run('reset_case', () => resetCase());
    if (!cleared) return;
    setSourceText('');
    setSourceFileName(null);
    setSourceFilePath(null);
    setWebSourceUrl('');
    setParsed(null);
    setSemantic(null);
    setAnswers({});
    setPlan(null);
    setPreview(null);
    setScannerField('');
    setScannerText('');
    setRuntimePrompt(null);
    setRuntimeMessage('');
    setStatus('Новый комплект начат. Данные предыдущего комплекта очищены.');
  }

  function applyRoutingRecommendation(routing?: DocumentRoutingRecommendation): string {
    if (!routing) return '';
    if (routing.auto_select && routing.recommended_document_ids.length) {
      setSelectedDocIds(routing.recommended_document_ids);
      const labels = routing.recommended_document_ids
        .map((id) => documents.find((document) => document.id === id)?.button_label || id)
        .join(', ');
      return ` Автоподбор комплекта: ${labels}.`;
    }
    if (routing.matches.length) {
      return ` Предложен тип «${routing.matches[0].button_label}», но перед созданием требуется подтверждение.`;
    }
    return ' Новый тип не сопоставлен с шаблонами; откройте мастер разметки.';
  }

  async function parseSourceNow() {
    const res = await run('parse_source', () => parseSource(sourceText, DEFAULT_YEAR));
    if (!res) return;
    const semanticResult = await run('semantic_extract', () => semanticExtract(sourceText, DEFAULT_YEAR, modelOutput.trim() || undefined));
    setSemantic(semanticResult ?? null);
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    setParsed({
      title: res.report?.recognized_title ?? 'Документ распознан',
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: 'manual_text',
      layoutRows: 0,
      tableRows: 0,
    });
    setAnswers({});
    setPlan(null);
    setPreview(null);
    const routingSummary = applyRoutingRecommendation(res.routing);
    setStatus(`Источник прочитан. Найдено значений: ${count}.${routingSummary}`);
  }

  async function pickSourceFile(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (file) await processSourceFile(file);
  }

  async function processSourceFile(file: File) {
    if (!(await ensureComponentForSource(file.name))) return;
    const buffer = await readFileBytes(file);
    const res = await run('parse_source_file', () =>
      parseSourceFile(file.name, arrayBufferToBase64(buffer), DEFAULT_YEAR));
    if (!res) return;
    setSourceFileName(file.name);
    setSourceFilePath(res.source_path);
    setSourceText(res.source_text);
    const semanticResult = await run('semantic_extract', () => semanticExtract(res.source_text, DEFAULT_YEAR, modelOutput.trim() || undefined));
    setSemantic(semanticResult ?? null);
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    const layoutItems = res.layout_items ?? [];
    setParsed({
      title: res.report?.recognized_title ?? file.name,
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: res.source_kind ?? 'file',
      layoutRows: layoutItems.length,
      tableRows: layoutItems.filter((item) => item.item_kind === 'table_row').length,
    });
    setAnswers({});
    setPlan(null);
    setPreview(null);
    const routingSummary = applyRoutingRecommendation(res.routing);
    setStatus(`Файл «${file.name}» прочитан. Найдено значений: ${count}.${routingSummary}`);
  }


  async function loadWebSource() {
    const url = webSourceUrl.trim();
    if (!url) {
      setStatus('Укажите HTTPS-адрес сайта или API.');
      return;
    }
    const res = await run('parse_web_source', () => parseWebSource(url, DEFAULT_YEAR));
    if (!res) return;
    setSourceFileName(res.final_url);
    setSourceFilePath(null);
    setSourceText(res.source_text);
    const semanticResult = await run('semantic_extract', () => semanticExtract(res.source_text, DEFAULT_YEAR, modelOutput.trim() || undefined));
    setSemantic(semanticResult ?? null);
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    setParsed({
      title: res.report?.recognized_title ?? res.final_url,
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: 'https',
      layoutRows: 0,
      tableRows: 0,
    });
    setAnswers({});
    setPlan(null);
    setPreview(null);
    const routingSummary = applyRoutingRecommendation(res.routing);
    setStatus(`Источник загружен. Найдено значений: ${count}.${routingSummary}`);
  }

  async function selectDocument(doc: DocumentTemplateSpec) {
    setActiveDoc(doc.id);
    setPreview(null);
    const [workflow, template] = await Promise.all([
      run('get_workflow_plan', () => getWorkflowPlan(doc.id, sickLeave)),
      run('get_document_template_text', () => getDocumentTemplateText(doc.id)),
    ]);
    if (template) setActiveTemplateText(template.template_text);
    if (!workflow) return;
    setPlan(workflow);
    setAnswers(Object.fromEntries(workflow.prompts.map((p) => [p.field_id, p.current_value ?? ''])));
    setStatus(workflow.prompts.length ? `Требуется уточнить полей: ${workflow.prompts.length}.` : 'Все поля распознаны — документ готов.');
  }

  function toggleDocumentSelected(documentId: string) {
    setSelectedDocIds((previous) => previous.includes(documentId)
      ? previous.filter((id) => id !== documentId)
      : [...previous, documentId]);
  }

  function selectAllVisibleDocuments() {
    const visibleIds = visibleDocs.map((document) => document.id);
    setSelectedDocIds((previous) => Array.from(new Set([...previous, ...visibleIds])));
  }

  function clearSelectedDocuments() {
    setSelectedDocIds([]);
  }

  function updateFolderParts(parts: FolderNamePartDto[]) {
    const next: FolderNamePartDto[] = parts.length ? parts : ['DocumentNumber', 'DocumentDate'];
    setFolderParts(next);
    try { localStorage.setItem(OUTPUT_PREFS_KEY, JSON.stringify(next)); } catch { /* private mode */ }
  }

  function updateAutoPrint(value: boolean) {
    setAutoPrint(value);
    try { localStorage.setItem(AUTO_PRINT_KEY, value ? 'true' : 'false'); } catch { /* private mode */ }
  }

  function updatePrintCopies(documentId: string, copies: number) {
    const normalized = normalizeCopyCount(copies);
    setPrintCopies((previous) => {
      const next = { ...previous, [documentId]: normalized };
      try { localStorage.setItem(PRINT_COPIES_KEY, JSON.stringify(next)); } catch { /* private mode */ }
      return next;
    });
  }

  function jobsForItems(items: GeneratedPrintItem[]): PrintJobDto[] {
    return items
      .map((item) => ({ path: item.path, copies: printCopies[item.document_id] ?? 1 }))
      .filter((job) => job.copies > 0);
  }

  async function queuePrint(
    jobs: PrintJobDto[],
    automatic = false,
    documentIds: string[] = [],
    preparedTriage?: PrintTriageReport | null,
    reviewFolder?: string | null,
  ) {
    const selectedJobs = jobs.filter((job) => normalizeCopyCount(job.copies) > 0);
    if (!selectedJobs.length) {
      setStatus('Для печати у всех документов указано 0 экземпляров.');
      return;
    }
    if (automatic) {
      const triage = preparedTriage ?? (documentIds.length
        ? await run('get_print_triage', () => getPrintTriage(documentIds, reviewFolder))
        : null);
      if (!triage) {
        setStatus('Автоматическая печать остановлена: комплект нужно проверить вручную.');
        return;
      }
      if (!triage.auto_print_allowed) {
        const blocker = triage.blockers[0];
        const detail = blocker
          ? `${blocker.field_id}: ${blocker.reason}`
          : triage.reasons[0] ?? 'Требуется проверка специалиста.';
        setStatus(`Комплект создан, но автоматическая печать остановлена: ${detail}`);
        return;
      }
    }
    const result = await run('print_files', () => printFiles(selectedJobs));
    if (!result) return;
    if (result.failed_files.length) {
      const first = result.failed_files[0];
      setStatus(`На печать отправлено экземпляров: ${result.queued_copies}; ошибок: ${result.failed_files.length}. ${first.error}`);
    } else {
      setStatus(`${automatic ? 'Автоматическая печать' : 'Печать'}: отправлено ${result.queued_copies} экземпляр(ов).`);
    }
  }

  async function openLastOutput() {
    const target = lastOutput?.folder || lastOutput?.files[0];
    if (!target) return;
    await run('open_in_file_manager', () => openInFileManager(target));
    setStatus('Папка готового комплекта открыта.');
  }

  async function printLastOutput() {
    if (!lastOutput) return;
    const items = lastOutput.print_items?.length
      ? lastOutput.print_items
      : lastOutput.files.map((path, index) => ({ document_id: `generated:${index}`, label: fileLabel(path), path }));
    await queuePrint(jobsForItems(items));
  }

  async function exportLastOutput(pdfa1: boolean) {
    if (!lastOutput?.files.length) return;
    if (!(await ensureOptionalComponent('office', 'Конвертация и печать (LibreOffice)'))) return;
    const result = await run('export_files_to_pdf', () => exportFilesToPdf(lastOutput.files, pdfa1, lastOutput.folder));
    if (!result) return;
    if (result.failed_files.length) {
      setStatus(`PDF-экспорт: создано ${result.created_files.length}, ошибок ${result.failed_files.length}. ${result.failed_files[0].error}`);
    } else {
      setStatus(`${pdfa1 ? 'Архивный PDF/A-1' : 'PDF'}: создано ${result.created_files.length}. ${result.conformance_note}`);
    }
  }

  async function exportLastOutputKedo() {
    if (!lastOutput?.files.length) return;
    const result = await run('create_kedo_package', () => createKedoPackage(
      lastOutput.files,
      lastOutput.folder || outputRoot.trim() || 'output/Готовые документы',
    ));
    if (!result) return;
    setStatus(`Пакет обмена создан: ${result.package_folder}.`);
  }

  async function generateSelectedDocuments() {
    if (!selectedDocIds.length) {
      setStatus('Отметьте хотя бы один документ для комплекта.');
      return;
    }
    await requestGeneration({ kind: 'batch', documentIds: selectedDocIds });
  }

  async function performGenerateSelectedDocuments(documentIds: string[]) {
    const res = await run('render_docx_batch', () => renderDocxBatch(
      documentIds,
      outputRoot.trim() || 'output/Готовые документы',
      folderParts,
      true,
    ));
    if (!res) return;
    const printItems = createdPrintItems(res.created_documents, res.created_files, documents, documentIds);
    setLastOutput({ folder: res.output_folder, files: res.created_files, source: 'batch', print_items: printItems });
    setStatus(`Комплект создан: ${res.created_files.length} документ(ов) в ${res.output_folder}.`);
    if (autoPrint) await queuePrint(jobsForItems(printItems), true, documentIds, null, res.output_folder);
  }

  async function requestGeneration(generation: PendingGeneration) {
    const workflow = generation.kind === 'single'
      ? await run('get_workflow_plan', () => getWorkflowPlan(generation.documentIds[0], sickLeave))
      : await run('get_workflow_plan_batch', () => getWorkflowPlanBatch(generation.documentIds, sickLeave));
    if (!workflow) return;
    if (workflow.blocked) {
      setStatus(`Создание заблокировано: ${workflow.block_reasons.join('; ')}`);
      return;
    }
    if (!workflow.prompts.length) {
      await completeGeneration(generation);
      return;
    }
    setRuntimeAnswers(Object.fromEntries(workflow.prompts.map((prompt) => [prompt.field_id, prompt.current_value ?? ''])));
    setRuntimeMessage('');
    setRuntimePrompt({
      plan: workflow,
      generation,
      title: generation.kind === 'batch' ? `Уточнить данные комплекта (${generation.documentIds.length})` : 'Уточнить данные документа',
    });
    setStatus(`Перед созданием нужно проверить или заполнить полей: ${workflow.prompts.length}.`);
  }

  async function submitRuntimePrompt() {
    if (!runtimePrompt) return;
    const payload = runtimePrompt.plan.prompts.map((prompt) => ({
      field_id: prompt.field_id,
      value: runtimeAnswers[prompt.field_id] ?? '',
      continue_without_value: false,
    }));
    const result = runtimePrompt.generation.kind === 'single'
      ? await run('apply_popup', () => applyPopup(runtimePrompt.generation.documentIds[0], sickLeave, payload))
      : await run('apply_popup_batch', () => applyPopupBatch(runtimePrompt.generation.documentIds, sickLeave, payload));
    if (!result) return;
    if (!result.accepted) {
      setRuntimeMessage(result.message || 'Проверьте введённые значения. Окно оставлено открытым.');
      setStatus(result.message || 'Проверьте обязательные поля.');
      return;
    }
    const generation = runtimePrompt.generation;
    setRuntimePrompt(null);
    setRuntimeMessage('');
    setStatus('Данные подтверждены. Формируется документ…');
    await completeGeneration(generation);
  }

  async function completeGeneration(generation: PendingGeneration) {
    if (generation.kind === 'single') await performGenerateDocx(generation.documentIds[0]);
    else await performGenerateSelectedDocuments(generation.documentIds);
  }

  function openPopupDesigner() {
    if (!activeDoc) return;
    const document = documents.find((item) => item.id === activeDoc);
    if (!document) return;
    const fields = document.popup_configured
      ? (document.popup_fields ?? [])
      : (document.popup_fields?.length ? document.popup_fields : (plan?.prompts ?? []).map(promptToPopupField));
    setPopupDesignerDocument(document);
    setPopupDesignerFields(fields);
  }

  async function savePopupDesigner() {
    if (!popupDesignerDocument) return;
    const invalid = popupDesignerFields.find((field) => !field.field_id.trim() || !field.title.trim());
    if (invalid) {
      setStatus('У каждого вопроса должны быть смысловое поле и понятный текст.');
      return;
    }
    const pack = await run('update_document_popup_fields', () => updateDocumentPopupFields(popupDesignerDocument.id, popupDesignerFields));
    if (!pack) return;
    setDocuments(pack.documents);
    setPopupDesignerDocument(null);
    setStatus(`Вопросы для «${popupDesignerDocument.button_label}» сохранены: ${popupDesignerFields.length}.`);
  }

  async function renameActiveDocument() {
    if (!activeDoc) return;
    const current = documents.find((document) => document.id === activeDoc);
    const requested = globalThis.prompt?.('Новое название документа', current?.button_label ?? '')?.trim();
    if (!requested || requested === current?.button_label) return;
    const pack = await run('rename_document_button', () => renameDocumentButton(activeDoc, requested));
    if (!pack) return;
    setDocuments(pack.documents);
    setStatus(`Документ переименован: ${requested}. Исходный шаблон не изменён.`);
  }

  async function approveActiveTemplate() {
    if (!activeDoc) return;
    const current = documents.find((document) => document.id === activeDoc);
    const jurisdiction = globalThis.prompt?.('Юрисдикция утверждённой формы', 'Российская Федерация')?.trim() ?? '';
    if (!jurisdiction) return;
    const approvedBy = globalThis.prompt?.('Кто утвердил форму (ФИО или роль)', '')?.trim() ?? '';
    if (!approvedBy) return;
    const acknowledgement = globalThis.confirm?.(
      `Подтвердите: организация проверила точную ревизию шаблона «${current?.button_label ?? activeDoc}» и принимает ответственность за её применение в указанной юрисдикции.`,
    ) ?? false;
    if (!acknowledgement) return;
    const approval = await run('approve_document_template', () => approveDocumentTemplate({
      documentId: activeDoc,
      jurisdiction,
      approvedBy,
      acknowledgement,
    }));
    if (!approval) return;
    setStatus('Версия шаблона подтверждена и готова к использованию.');
  }

  async function removeActiveDocument() {
    if (!activeDoc) return;
    const current = documents.find((document) => document.id === activeDoc);
    const confirmed = globalThis.confirm?.(`Убрать документ «${current?.button_label ?? activeDoc}» из набора? Файл шаблона останется на диске.`) ?? false;
    if (!confirmed) return;
    const pack = await run('remove_document_button', () => removeDocumentButton(activeDoc));
    if (!pack) return;
    setDocuments(pack.documents);
    setActiveDoc(null);
    setPlan(null);
    setPreview(null);
    setStatus('Документ убран из набора. Исходный шаблон сохранён.');
  }

  async function pinField(fieldId: string) {
    const value = answers[fieldId] ?? '';
    await run('set_field', () => setField(fieldId, value));
    setStatus('Значение сохранено и будет использовано в других документах комплекта.');
  }

  async function saveFields() {
    if (!activeDoc || !plan) return;
    const missing = plan.prompts.filter((p) => p.required && !answers[p.field_id]?.trim());
    if (missing.length) { setStatus(`Не заполнено обязательное поле: ${missing[0].title}.`); return; }
    const payload = Object.entries(answers).map(([field_id, value]) => ({ field_id, value }));
    const res = await run('apply_popup', () => applyPopup(activeDoc, sickLeave, payload));
    if (!res) return;
    setStatus(res.accepted ? 'Поля сохранены. Значения общие для всех документов.' : res.message || `Не заполнено полей: ${res.still_missing?.length ?? 0}`);
  }

  async function previewNow() {
    const res = await run('render_preview', () => renderPreview(activeTemplateText, false));
    if (!res) return;
    setPreview({ text: res.output_text ?? '', missing: res.missing_fields?.length ?? 0 });
    setStatus(res.missing_fields?.length ? `Предпросмотр: не заполнено полей — ${res.missing_fields.length}.` : 'Предпросмотр готов — незаполненных полей нет.');
  }

  async function generateDocx() {
    if (!activeDoc) { setStatus('Выберите документ, который нужно создать.'); return; }
    await requestGeneration({ kind: 'single', documentIds: [activeDoc] });
  }

  async function performGenerateDocx(documentId: string) {
    const res = await run('render_docx', () => renderDocx(documentId, `output/${documentId}.docx`, true));
    if (!res) return;
    if (res.output_path) {
      const current = documents.find((document) => document.id === documentId);
      const printItems: GeneratedPrintItem[] = [{ document_id: documentId, label: current?.button_label ?? fileLabel(res.output_path), path: res.output_path }];
      setLastOutput({ folder: res.output_path, files: [res.output_path], source: 'single', print_items: printItems });
      setStatus(`Готово: ${res.output_path}`);
      if (autoPrint) await queuePrint(jobsForItems(printItems), true, [documentId], null, res.output_path);
    } else {
      setStatus(`Не заполнено полей: ${res.missing_fields?.length ?? 0}.`);
    }
  }

  async function analyzeInDialog() {
    const res = await run('analyze_template', () => analyzeTemplate(templateText, newDocumentId(), importedTemplatePath ?? '', previewLabel));
    if (!res) return;
    setStatus(`Шаблон прочитан. Найдено мест для заполнения: ${res.document.placeholders.length}.`);
  }

  /** Пользователь выбрал .docx в диалоге: байты уходят в Rust, обратно приходят
   *  настоящий путь в app_data/user-templates и извлечённый текст. */
  async function pickTemplateFile(event: React.ChangeEvent<HTMLInputElement>) {
    const files = Array.from(event.target.files ?? []);
    event.target.value = '';
    if (files.length) await processTemplateFiles(files);
  }

  function openTemplateSetup() {
    setTemplateText('');
    setButtonLabel('');
    setImportedTemplatePath(null);
    setPendingTemplates([]);
    setDraftPopupFields([]);
    setSetupOpen(true);
  }

  async function processTemplateFiles(files: File[]) {
    const accepted = files.filter((file) => /\.doc[xm]$/i.test(file.name));
    if (!accepted.length) {
      setStatus('Шаблоны должны быть в формате DOCX или DOCM.');
      return;
    }
    const importedRows: PendingTemplate[] = [];
    for (const file of accepted) {
      const id = newDocumentId();
      const buffer = await readFileBytes(file);
      const imported = await run('import_template_file', () =>
        importTemplateFile(id, { fileName: file.name, bytesBase64: arrayBufferToBase64(buffer) }));
      if (!imported) continue;
      const detectedLabel = detectTitle(imported.extracted_text) || file.name.replace(/\.doc[xm]$/i, '');
      const analyzed = await run('analyze_template_file', () => analyzeTemplateFile(imported.template_path, id, detectedLabel));
      if (!analyzed) continue;
      importedRows.push({
        document_id: id,
        template_path: imported.template_path,
        extracted_text: imported.extracted_text,
        file_name: file.name,
        button_label: detectedLabel,
        popup_fields: analyzed.document.popup_fields ?? [],
      });
    }
    if (!importedRows.length) return;
    setPendingTemplates((previous) => [...previous, ...importedRows]);
    const last = importedRows.at(-1)!;
    setImportedTemplatePath(last.template_path);
    setTemplateText(last.extracted_text);
    setButtonLabel(last.button_label);
    setStatus(`Шаблоны выбраны: ${importedRows.length}. Проверьте названия и нажмите «Создать кнопки».`);
  }

  async function processTemplateFile(file: File) {
    await processTemplateFiles([file]);
  }

  function updatePendingTemplateLabel(documentId: string, value: string) {
    setPendingTemplates((previous) => previous.map((item) => (
      item.document_id === documentId ? { ...item, button_label: value } : item
    )));
  }

  function updatePendingPopupFields(documentId: string, fields: PopupFieldConfig[]) {
    setPendingTemplates((previous) => previous.map((item) => (
      item.document_id === documentId ? { ...item, popup_fields: fields } : item
    )));
  }

  async function markupPendingTemplate(
    documentId: string,
    selectedText: string,
    fieldId: string,
    action: 'replace' | 'insert_after',
  ) {
    const current = pendingTemplates.find((item) => item.document_id === documentId);
    const value = selectedText.trim();
    const normalizedField = fieldId.trim();
    if (!current || !value || !normalizedField) {
      setStatus('Выделите значение в шаблоне и укажите смысловое поле.');
      return;
    }
    const outputPath = cursorMarkedTemplatePath(current.template_path, documentId);
    const report = await run('apply_template_markup_command', () => applyTemplateMarkup(
      current.template_path,
      outputPath,
      [{ field_id: normalizedField, value, action }],
    ));
    if (!report) return;
    if (!report.replaced_occurrences) {
      setStatus('Выделенный фрагмент не найден в видимом тексте DOCX/DOCM. Исходный шаблон не изменён.');
      return;
    }
    const placeholder = `{{${normalizedField}}}`;
    const visibleReplacement = action === 'replace' ? placeholder : `${value}${placeholder}`;
    setPendingTemplates((previous) => previous.map((item) => item.document_id === documentId
      ? {
          ...item,
          template_path: report.output_path,
          extracted_text: replaceAllLiteral(item.extracted_text, value, visibleReplacement),
        }
      : item));
    if (importedTemplatePath === current.template_path) setImportedTemplatePath(report.output_path);
    if (templateText === current.extracted_text) {
      setTemplateText(replaceAllLiteral(templateText, value, visibleReplacement));
    }
    setStatus(`Шаблон размечен. Обновлено мест: ${report.replaced_occurrences}. Исходный файл сохранён.`);
  }

  async function startGuidedSourceScanner(preselectedFieldId = '') {
    if (!sourceFilePath) {
      setStatus('Сначала выберите исходный DOCX/DOCM. Программа откроет именно этот документ в Word.');
      return;
    }
    const active = documents.find((document) => document.id === activeDoc) ?? null;
    const session = await run('start_word_scanner', () => startWordScanner(sourceFilePath, 'source', false));
    if (!session) return;
    setGuidedScanner({
      session,
      target: { mode: 'source', documentId: active?.id ?? null, label: active?.button_label ?? null, domain: active?.category ?? null },
      capture: null,
      suggestions: [],
      selectedFieldId: preselectedFieldId,
      rememberRule: true,
      addQuestion: Boolean(active),
      markupAction: 'replace',
    });
    setStatus('Word открыт автоматически. Выделите значение мышью или просто поставьте курсор внутрь слова.');
  }

  async function reportSemanticFieldError(fieldId: string, value: string) {
    setScannerField(fieldId);
    setScannerText(value);
    setStatus('Покажите правильное значение один раз — программа исправит текущий комплект и запомнит расположение.');
    await startGuidedSourceScanner(fieldId);
  }

  async function startGuidedPendingTemplateScanner(documentId: string) {
    const pending = pendingTemplates.find((item) => item.document_id === documentId);
    if (!pending) return;
    const session = await run('start_word_scanner', () => startWordScanner(pending.template_path, 'template', true));
    if (!session) return;
    setGuidedScanner({
      session,
      target: { mode: 'template', kind: 'pending', documentId, label: pending.button_label || pending.file_name, domain: null },
      capture: null,
      suggestions: [],
      selectedFieldId: '',
      rememberRule: false,
      addQuestion: true,
      markupAction: 'replace',
    });
    setStatus('Безопасная копия шаблона открыта в Word. Покажите программе место мышью.');
  }

  async function startGuidedExistingTemplateScanner() {
    const document = documents.find((item) => item.id === activeDoc);
    if (!document) {
      setStatus('Сначала выберите документ слева.');
      return;
    }
    const session = await run('start_word_scanner', () => startWordScanner(document.template_path, 'template', true));
    if (!session) return;
    setGuidedScanner({
      session,
      target: { mode: 'template', kind: 'existing', documentId: document.id, label: document.button_label, domain: document.category },
      capture: null,
      suggestions: [],
      selectedFieldId: '',
      rememberRule: false,
      addQuestion: true,
      markupAction: 'replace',
    });
    setStatus('Безопасная копия шаблона открыта в Word. Выделите примерное значение или подпись поля.');
  }

  async function captureGuidedScannerValue() {
    if (!guidedScanner) return;
    const capture = await run('capture_word_scanner', () => captureWordScanner(guidedScanner.session.session_id, false));
    if (!capture) return;
    let learnedRules: LearnedScannerRule[] = [];
    try {
      learnedRules = await listLearnedScannerRules();
    } catch {
      // The scanner remains usable even when the optional learned-rules store is unavailable.
    }
    const suggestions = suggestScannerFields({
      selectedText: capture.selected_text,
      contextText: capture.context_text,
      documents,
      activeDocumentId: guidedScanner.target.documentId,
      domainHint: guidedScanner.target.domain,
      learnedRules,
    });
    const recommended = bestScannerSuggestion(suggestions);
    setGuidedScanner((current) => current ? {
      ...current,
      capture,
      suggestions,
      selectedFieldId: recommended?.field_id ?? '',
      markupAction: inferGuidedMarkupAction(capture),
    } : current);
    setStatus(recommended
      ? `Программа предлагает: «${recommended.title}». Проверьте вариант и подтвердите.`
      : 'Программа не уверена. Выберите один из вариантов или укажите своё поле.');
  }

  async function returnToGuidedWord() {
    const current = guidedScanner;
    if (!current) return;
    const activated = await run('activate_word_scanner', () => activateWordScanner(current.session.session_id));
    if (!activated) return;
    setStatus('Word снова открыт поверх окон. Покажите значение мышью, затем вернитесь в Доккомплект.');
  }

  async function retryGuidedScannerSelection() {
    const current = guidedScanner;
    if (!current) return;
    const activated = await run('activate_word_scanner', () => activateWordScanner(current.session.session_id));
    if (!activated) return;
    setGuidedScanner({
      ...current,
      capture: null,
      suggestions: [],
      selectedFieldId: '',
    });
    setStatus('Выделите другое значение в Word. Когда закончите, вернитесь и нажмите «Я показал значение».');
  }

  async function confirmGuidedScanner() {
    if (!guidedScanner) return;
    const capture = guidedScanner.capture;
    if (!capture || !guidedScanner.selectedFieldId.trim()) return;
    const current = guidedScanner;
    const fieldId = current.selectedFieldId.trim();
    const suggestion = current.suggestions.find((item) => item.field_id === fieldId) ?? null;
    const title = suggestion?.title ?? newPopupField(fieldId).title;
    const inputKind = suggestion?.input_kind ?? newPopupField(fieldId).input_kind;

    if (current.target.mode === 'source') {
      const sourceTarget = current.target;
      const applied = await run('apply_scanner', () => applyScanner([{
        field_id: fieldId,
        selected_text: capture.selected_text,
        page_index: 0,
        confidence: 1,
      }]));
      if (!applied || !applied.applied_fields.includes(fieldId)) {
        setStatus('Значение не принято валидатором. Выберите другой смысл поля или исправьте значение в документе.');
        return;
      }
      const saved = await run('save_learned_scanner_rule', () => saveLearnedScannerRule({
        fieldId,
        title,
        selectedText: capture.selected_text,
        contextText: capture.context_text,
        beforeText: capture.before_text,
        afterText: capture.after_text,
        inputKind,
        sourceText,
      }));
      if (!saved) return;
      if (current.addQuestion && sourceTarget.documentId) {
        const document = documents.find((item) => item.id === sourceTarget.documentId);
        if (document) {
          const popupFields = ensureSuggestedPopupField(document.popup_fields ?? [], fieldId, title, inputKind);
          const pack = await run('update_document_popup_fields', () => updateDocumentPopupFields(document.id, popupFields));
          if (!pack) return;
          setDocuments(pack.documents);
        }
      }
      setScannerField(fieldId);
      setScannerText(capture.selected_text);
      const closed = await run('close_word_scanner', () => closeWordScanner(current.session.session_id, false));
      if (!closed) return;
      setGuidedScanner(null);
      setStatus(`Готово. «${capture.selected_text}» назначено полю «${title}». Word закрыт автоматически.`);
      return;
    }

    const templateTarget = current.target;
    const marked = await run('apply_word_scanner_selection', () => applyWordScannerSelection(
      current.session.session_id,
      fieldId,
      current.markupAction,
    ));
    if (!marked) return;

    if (templateTarget.kind === 'pending') {
      setPendingTemplates((previous) => previous.map((item) => item.document_id === templateTarget.documentId
        ? {
            ...item,
            template_path: marked.output_path,
            extracted_text: marked.extracted_text,
            popup_fields: current.addQuestion
              ? ensureSuggestedPopupField(item.popup_fields, fieldId, title, inputKind)
              : item.popup_fields,
          }
        : item));
      setImportedTemplatePath((previous) => previous === current.session.original_path ? marked.output_path : previous);
      setTemplateText((previous) => previous === pendingTemplates.find((item) => item.document_id === templateTarget.documentId)?.extracted_text
        ? marked.extracted_text
        : previous);
    } else {
      let pack = await run('update_document_template', () => updateDocumentTemplate(templateTarget.documentId, marked.output_path));
      if (!pack) return;
      if (current.addQuestion) {
        const document = pack.documents.find((item) => item.id === templateTarget.documentId);
        if (document) {
          const popupFields = ensureSuggestedPopupField(document.popup_fields ?? [], fieldId, title, inputKind);
          pack = await run('update_document_popup_fields', () => updateDocumentPopupFields(document.id, popupFields));
          if (!pack) return;
        }
      }
      setDocuments(pack.documents);
      setActiveTemplateText(marked.extracted_text);
    }
    setGuidedScanner(null);
    setStatus(`Готово. Поле «${title}» вставлено в безопасную копию шаблона. Word закрыт автоматически.`);
  }

  async function cancelGuidedScanner() {
    const current = guidedScanner;
    if (!current) return;
    if (!current.capture?.document_closed) {
      await run('close_word_scanner', () => closeWordScanner(current.session.session_id, current.target.mode === 'template'));
    }
    setGuidedScanner(null);
    setStatus(current.target.mode === 'template'
      ? 'Разметка отменена. Безопасная копия удалена, исходный шаблон не изменён.'
      : 'Сканер закрыт. Исходный документ не изменён.');
  }

  async function createButtonFromTemplate() {
    const candidates = pendingTemplates.length
      ? pendingTemplates.map((item) => ({
          document_id: item.document_id,
          template_path: item.template_path,
          extracted_text: item.extracted_text,
          preferred_button_label: item.button_label.trim() || item.file_name.replace(/\.doc[xm]$/i, ''),
        }))
      : [];

    if (!candidates.length) {
      const id = newDocumentId();
      let templatePath = importedTemplatePath;
      if (!templatePath) {
        const imported = await run('import_template_file', () => importTemplateFile(id, { templateText }));
        if (!imported) return;
        templatePath = imported.template_path;
      }
      candidates.push({
        document_id: id,
        template_path: templatePath,
        extracted_text: templateText,
        preferred_button_label: previewLabel,
      });
    }

    const rows = await run('prepare_template_setup', () => prepareTemplateSetup(candidates));
    if (!rows) return;
    const staticRows = rows.filter((row) => row.is_static_copy);
    if (staticRows.length) {
      setStatus(`Кнопки будут созданы. Шаблоны без полей будут копироваться без изменений: ${staticRows.map((row) => row.detected_title).join(', ')}.`);
    }
    const labels = new Map(pendingTemplates.map((item) => [item.document_id, item.button_label.trim()]));
    const popupById = new Map(pendingTemplates.map((item) => [item.document_id, item.popup_fields]));
    const confirmedRows = rows.map((row) => ({
      ...row,
      editable_button_label: labels.get(row.document_id) || (rows.length === 1 ? buttonLabel.trim() : '') || row.editable_button_label,
      popup_fields: popupById.get(row.document_id) ?? (rows.length === 1 ? draftPopupFields : row.popup_fields ?? []),
    }));
    const pack = await run('confirm_template_setup', () => confirmTemplateSetup(confirmedRows));
    if (!pack) return;
    setDocuments(pack.documents);
    setSelectedDocIds(pack.documents.map((document) => document.id));
    setActiveTemplateText(templateText);
    setImportedTemplatePath(null);
    setPendingTemplates([]);
    setDraftPopupFields([]);
    setSetupOpen(false);
    setStatus(`Кнопки созданы: ${confirmedRows.length}. Теперь добавьте исходный документ.`);
  }

  async function chooseIcd(hit: Icd10Suggestion) {
    await run('set_field', async () => {
      await setField('medical.icd10', hit.code);
      return setField('medical.diagnosis', hit.title);
    });
    setIcdQuery(`${hit.code} ${hit.title}`);
    setStatus(`Значение выбрано: ${hit.code} — ${hit.title}. Оно будет использовано во всех выбранных документах.`);
  }

  async function searchIcd() {
    const res = await run('icd10_suggest', () => icd10Suggest(icdQuery));
    if (!res) return;
    setIcdHits(res.slice(0, 6));
    setStatus(`Найдено вариантов: ${res.length} по запросу «${icdQuery}».`);
  }

  async function seriesPlan() {
    if (!seriesStart.trim() || !seriesEnd.trim()) {
      setStatus('Укажите даты начала и окончания серии записей.');
      return;
    }
    const res = await run('get_record_series_plan', () => getRecordSeriesPlan({
      start_date: seriesStart.trim(),
      end_date: seriesEnd.trim(),
      default_year: DEFAULT_YEAR,
      start_offset_days: 0,
      cadence: { kind: 'daily' },
      day_start_time: null,
      day_end_time: null,
      skip_weekdays: seriesSkipWeekends ? [6, 7] : [],
      excluded_dates: [],
    }));
    if (res) setStatus(`Серия записей рассчитана: ${res.length} дат${seriesSkipWeekends ? ', выходные пропущены' : ''}.`);
  }
  async function scanMarks(addQuestion = false) {
    if (!scannerField.trim() || !scannerText.trim()) {
      setStatus('Укажите смысл значения и выделенный фрагмент текста.');
      return;
    }
    const fieldId = scannerField.trim();
    const res = await run('apply_scanner', () => applyScanner([{
      field_id: fieldId,
      selected_text: scannerText.trim(),
      page_index: 0,
      confidence: 1,
    }]));
    if (!res) return;
    if (addQuestion) {
      const document = documents.find((item) => item.id === activeDoc);
      if (!document) {
        setStatus('Значение распознано, но для создания вопроса сначала выберите документ.');
        return;
      }
      const startingFields = document.popup_configured
        ? (document.popup_fields ?? [])
        : (document.popup_fields?.length ? document.popup_fields : (plan?.prompts ?? []).map(promptToPopupField));
      const updatedFields = ensurePopupField(startingFields, fieldId);
      const pack = await run('update_document_popup_fields', () => updateDocumentPopupFields(document.id, updatedFields));
      if (!pack) return;
      setDocuments(pack.documents);
      setStatus(`Значение добавлено в уточнения документа «${document.button_label}».`);
      return;
    }
    setStatus(`Разметка сохранена: принято ${res.applied_fields.length}, пропущено ${res.rejected_fields.length}.`);
  }
  async function outputPlan() {
    if (!outputRoot.trim()) {
      setStatus('Укажите корневую папку вывода.');
      return;
    }
    const labels = visibleDocs.map((d) => d.button_label);
    const res = await run('get_output_plan', () => getOutputPlan(
      outputRoot.trim(),
      folderParts,
      labels,
    ));
    if (res) setStatus(`Папка комплекта: ${res.patient_folder}`);
  }
  async function saveSession() {
    await run('save_state', () => saveState(STATE_DB));
    setStatus('Настройки и текущий набор сохранены.');
  }
  async function loadSession() {
    const res = await run('load_state', () => loadState(STATE_DB));
    if (res?.pack?.documents) { setDocuments(res.pack.documents); setSelectedDocIds(res.pack.documents.map((document) => document.id)); setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов).`); }
  }
  async function checkAccess() {
    const res = await run('validate_product_access', () => validateProductAccess(null));
    if (res) setStatus(`Доступ: ${res.plan}; использовано ${res.documents_used_month}/${res.document_limit_month}, осталось ${res.documents_left_month}.`);
  }
  async function verifyLicense() {
    // Доверенный публичный ключ вшит в Rust-бинарник; UI его не передаёт.
    const res = await run('verify_rust_license_text', () => verifyRustLicenseText(licenseText));
    setStatus(res ? 'Лицензия подтверждена.' : 'Не удалось подтвердить лицензию.');
  }
  async function checkUpdates() {
    const result = await run('check_for_updates', () => checkForUpdates());
    if (!result) return;
    if (result.available) {
      setStatus(`Доступна версия ${result.latest_version}. Проверенный пакет сохранён: ${result.verified_package_path ?? 'путь не указан'}.`);
    } else {
      setStatus(`${result.message}: ${result.current_version}.`);
    }
  }

  async function installWatcher() {
    const res = await run('install_background_watcher', () => installBackgroundWatcher(watchFolder.trim() || 'Созданные документы', DEFAULT_YEAR, sickLeave, folderParts, autoPrint, printCopies));
    if (res) setStatus(`Автоматическая обработка включена для папки «${res.watch_folder ?? ''}»${res.warnings?.length ? `; замечания: ${res.warnings.join('; ')}` : ''}.`);
  }
  async function uninstallWatcher() {
    await run('uninstall_background_watcher', () => uninstallBackgroundWatcher());
    setStatus('Автоматическая обработка папки отключена.');
  }

  async function runZeroTouch() {
    if (!intakeSource.trim()) {
      setStatus('Укажите путь к исходному файлу поддерживаемого формата.');
      return;
    }
    const res = await run('run_created_documents_intake', () =>
      runCreatedDocumentsIntake(intakeSource.trim(), watchFolder.trim() || 'Созданные документы', folderParts, DEFAULT_YEAR, sickLeave));
    if (!res) return;
    setIntakeResult(res);
    setStatus(res.message);
    if (res.status === 'processed' && res.created_files.length) {
      const printItems = createdPrintItems(res.created_documents, res.created_files, documents);
      setLastOutput({ folder: res.patient_folder, files: res.created_files, source: 'zero_touch', print_items: printItems });
      if (autoPrint) await queuePrint(jobsForItems(printItems), true, printItems.map((item) => item.document_id), res.print_triage ?? null, res.patient_folder);
    }
  }

  async function understand() {
    const res = await run('semantic_extract', () => semanticExtract(sourceText, DEFAULT_YEAR, modelOutput.trim() || undefined));
    if (!res) return;
    setSemantic(res);
    setStatus(res.model_applied
      ? `Проверка завершена: найдено значений — ${res.fields.length}.`
      : `Проверка завершена: уверенно найдено значений — ${res.fields.length}.`);
  }


  return (
    <div className="appRoot">
      <div className="window">
        <header className="hdr clientHeader">
          <div className="brand">
            <span className="logo" aria-hidden="true"><i className="ti ti-files" /></span>
            <div className="brandText">
              <span className="bname">Доккомплект</span>
              <span className="bnote">Из исходника — готовый комплект документов</span>
            </div>
          </div>
          <div className="hdrRight">
            <button className="headerSettings" onClick={() => setUtilityOpen((value) => !value)} aria-expanded={utilityOpen}>
              <i className="ti ti-settings" aria-hidden="true" /> Настройки
            </button>
            <ThemeSwitcher theme={theme} onChange={setTheme} />
          </div>
        </header>

        <div className="grid clientGrid">
          <Workspace
            busy={busy}
            watchFolder={watchFolder}
            intakeSource={intakeSource}
            intakeResult={intakeResult}
            lastOutput={lastOutput}
            autoPrint={autoPrint}
            printCopies={printCopies}
            sourceText={sourceText}
            sourceFileName={sourceFileName}
            sourceFilePath={sourceFilePath}
            webSourceUrl={webSourceUrl}
            intakeCapabilities={intakeCapabilities}
            scannerField={scannerField}
            scannerText={scannerText}
            parsed={parsed}
            modelOutput={modelOutput}
            semantic={semantic}
            plan={plan}
            answers={answers}
            preview={preview}
            setWatchFolder={setWatchFolder}
            setIntakeSource={setIntakeSource}
            setAutoPrint={updateAutoPrint}
            setSourceText={setSourceText}
            setSourceFileName={(value) => { setSourceFileName(value); if (value === null) setSourceFilePath(null); }}
            setWebSourceUrl={setWebSourceUrl}
            setScannerField={setScannerField}
            setScannerText={setScannerText}
            setModelOutput={setModelOutput}
            setAnswers={setAnswers}
            onRunZeroTouch={runZeroTouch}
            onOpenLastOutput={openLastOutput}
            onPrintLastOutput={printLastOutput}
            onExportLastOutputPdf={() => void exportLastOutput(false)}
            onExportLastOutputPdfa={() => void exportLastOutput(true)}
            onExportLastOutputKedo={() => void exportLastOutputKedo()}
            onPickSourceFile={pickSourceFile}
            onDropSourceFile={processSourceFile}
            onLoadWebSource={loadWebSource}
            onResetCase={resetCurrentCase}
            onParseSource={parseSourceNow}
            onStartGuidedSourceScanner={() => { void startGuidedSourceScanner(); }}
            onReportSemanticError={(fieldId, value) => { void reportSemanticFieldError(fieldId, value); }}
            onApplyScannerSelection={() => void scanMarks(false)}
            onApplyScannerAndQuestion={() => void scanMarks(true)}
            onPrintCopyChange={updatePrintCopies}
            onUnderstand={understand}
            onPinField={pinField}
            onPreview={previewNow}
            onSaveFields={saveFields}
            onGenerate={generateDocx}
          />
          <DocumentRail
            documents={visibleDocs}
            activeDocumentId={activeDoc}
            selectedDocumentIds={selectedDocIds}
            busy={busy}
            printCopies={printCopies}
            extraRulesEnabled={sickLeave}
            onExtraRulesChange={setSickLeave}
            onSelect={selectDocument}
            onToggleSelected={toggleDocumentSelected}
            onPrintCopiesChange={updatePrintCopies}
            onSelectAll={selectAllVisibleDocuments}
            onClearSelected={clearSelectedDocuments}
            onGenerateSelected={generateSelectedDocuments}
            onRename={renameActiveDocument}
            onConfigurePopups={openPopupDesigner}
            onScanTemplate={startGuidedExistingTemplateScanner}
            onRemove={removeActiveDocument}
            onApprove={approveActiveTemplate}
            onAdd={openTemplateSetup}
            onToggleUtilities={() => setUtilityOpen((value) => !value)}
          />
        </div>

        {utilityOpen && (
          <UtilityPanel
            documents={documents}
            selectedDocumentIds={selectedDocIds}
            onStatus={setStatus}
            onDocumentsChanged={setDocuments}
            seriesStart={seriesStart}
            seriesEnd={seriesEnd}
            seriesSkipWeekends={seriesSkipWeekends}
            scannerField={scannerField}
            scannerText={scannerText}
            outputRoot={outputRoot}
            folderParts={folderParts}
            licenseText={licenseText}
            onSeriesStartChange={setSeriesStart}
            onSeriesEndChange={setSeriesEnd}
            onSeriesSkipWeekendsChange={setSeriesSkipWeekends}
            onScannerFieldChange={setScannerField}
            onScannerTextChange={setScannerText}
            onOutputRootChange={setOutputRoot}
            onFolderPartsChange={updateFolderParts}
            onLicenseTextChange={setLicenseText}
            onSeriesPlan={seriesPlan}
            onScanMarks={() => void scanMarks(false)}
            onOutputPlan={outputPlan}
            onSaveSession={saveSession}
            onLoadSession={loadSession}
            onCheckAccess={checkAccess}
            onCheckUpdates={checkUpdates}
            onInstallWatcher={installWatcher}
            onUninstallWatcher={uninstallWatcher}
            onVerifyLicense={verifyLicense}
            onSemanticCaseChanged={(semanticCase) => setSemantic({
              fields: Object.values(semanticCase.values).map((value) => ({
                field_id: value.field_id,
                value: value.value,
                confidence: value.confidence,
                method: value.source,
                source: value.source,
                evidence: value.evidence?.map((item) => item.excerpt) ?? [],
              })),
              warnings: [],
              model_applied: false,
              prompt: 'business_registry_confirmed',
            })}
          />
        )}

        <footer className="statusBar">
          <span className={busy ? 'dot busy' : 'dot'} aria-hidden="true" />
          {status}
        </footer>
      </div>

      {setupOpen && (
        <TemplateSetupModal
          templateText={templateText}
          buttonLabel={buttonLabel}
          previewTitle={previewTitle}
          pendingTemplates={pendingTemplates}
          draftPopupFields={draftPopupFields}
          onTemplateTextChange={setTemplateText}
          onButtonLabelChange={setButtonLabel}
          onDraftPopupFieldsChange={setDraftPopupFields}
          onPendingTemplateLabelChange={updatePendingTemplateLabel}
          onPendingPopupFieldsChange={updatePendingPopupFields}
          onMarkupPendingTemplate={markupPendingTemplate}
          onStartGuidedPendingScanner={startGuidedPendingTemplateScanner}
          onAnalyze={analyzeInDialog}
          onPickFile={pickTemplateFile}
          onDropFiles={processTemplateFiles}
          onCancel={() => setSetupOpen(false)}
          onConfirm={createButtonFromTemplate}
        />
      )}

      {popupDesignerDocument && (
        <PopupDesignerModal
          document={popupDesignerDocument}
          fields={popupDesignerFields}
          onChange={setPopupDesignerFields}
          onCancel={() => setPopupDesignerDocument(null)}
          onSave={savePopupDesigner}
        />
      )}

      {runtimePrompt && (
        <RuntimePromptModal
          title={runtimePrompt.title}
          plan={runtimePrompt.plan}
          answers={runtimeAnswers}
          message={runtimeMessage}
          busy={busy}
          onAnswer={(fieldId, value) => setRuntimeAnswers((previous) => {
            const previousSourceValue = previous[fieldId] ?? '';
            const next = { ...previous, [fieldId]: value };
            for (const prompt of runtimePrompt.plan.prompts) {
              if (prompt.linked_to !== fieldId) continue;
              const linkedCurrent = previous[prompt.field_id] ?? '';
              if (!linkedCurrent || linkedCurrent === previousSourceValue) next[prompt.field_id] = value;
            }
            return next;
          })}
          onCancel={() => { setRuntimePrompt(null); setRuntimeMessage(''); setStatus('Создание отменено: данные не изменены.'); }}
          onSubmit={submitRuntimePrompt}
        />
      )}

      {guidedScanner && (
        <GuidedScannerModal
          mode={guidedScanner.target.mode}
          session={guidedScanner.session}
          capture={guidedScanner.capture}
          suggestions={guidedScanner.suggestions}
          selectedFieldId={guidedScanner.selectedFieldId}
          rememberRule={guidedScanner.rememberRule}
          addQuestion={guidedScanner.addQuestion}
          markupAction={guidedScanner.markupAction}
          busy={busy}
          targetLabel={guidedScanner.target.label}
          onCapture={captureGuidedScannerValue}
          onReturnToWord={returnToGuidedWord}
          onRetry={retryGuidedScannerSelection}
          onSelectedFieldIdChange={(selectedFieldId) => setGuidedScanner((current) => current ? { ...current, selectedFieldId } : current)}
          onRememberRuleChange={(rememberRule) => setGuidedScanner((current) => current ? { ...current, rememberRule } : current)}
          onAddQuestionChange={(addQuestion) => setGuidedScanner((current) => current ? { ...current, addQuestion } : current)}
          onMarkupActionChange={(markupAction) => setGuidedScanner((current) => current ? { ...current, markupAction } : current)}
          onConfirm={confirmGuidedScanner}
          onCancel={cancelGuidedScanner}
        />
      )}

    </div>
  );
}

function inferGuidedMarkupAction(capture: WordScannerCapture): GuidedScannerMarkupAction {
  const selected = capture.selected_text.trim();
  const looksLikeLabel = /[:：№#]\s*$/.test(selected)
    || /^(?:номер|дата|фио|ф\.и\.о|инн|кпп|огрн|адрес|телефон|должность|диагноз|лечение|сумма|итого|vin)\b/i.test(selected);
  return looksLikeLabel ? 'insert_after' : 'replace';
}

function ensureSuggestedPopupField(
  fields: PopupFieldConfig[],
  fieldId: string,
  title: string,
  inputKind: PopupFieldConfig['input_kind'],
): PopupFieldConfig[] {
  if (fields.some((field) => field.field_id === fieldId)) return fields;
  return [...fields, {
    ...newPopupField(fieldId),
    title,
    input_kind: inputKind,
    help_text: 'Если программа не найдёт это значение в исходном документе, она попросит специалиста его ввести.',
  }];
}

function promptToPopupField(prompt: PromptSpec): PopupFieldConfig {
  return {
    field_id: prompt.field_id,
    title: prompt.title,
    required: prompt.required,
    input_kind: prompt.input_kind ?? 'text',
    ask_mode: prompt.ask_mode ?? 'if_missing',
    options: prompt.options ?? [],
    allow_custom_option: prompt.allow_custom_option ?? false,
    help_text: prompt.help_text ?? prompt.validation_hint ?? null,
    section: prompt.section ?? 'Данные документа',
    default_value: null,
    linked_to: prompt.linked_to ?? null,
    order: prompt.order ?? 500,
  };
}

function loadOutputFolderParts(): FolderNamePartDto[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(OUTPUT_PREFS_KEY) || 'null');
    if (Array.isArray(parsed) && parsed.every((value) => typeof value === 'string')) {
      return parsed as FolderNamePartDto[];
    }
  } catch { /* use privacy-safe default */ }
  return ['DocumentNumber', 'DocumentDate'];
}

function loadAutoPrintPreference(): boolean {
  try { return localStorage.getItem(AUTO_PRINT_KEY) === 'true'; } catch { return false; }
}

function loadPrintCopyPreferences(): Record<string, number> {
  try {
    const parsed = JSON.parse(localStorage.getItem(PRINT_COPIES_KEY) || '{}');
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return Object.fromEntries(Object.entries(parsed).map(([key, value]) => [key, normalizeCopyCount(Number(value))]));
    }
  } catch { /* use one copy by default */ }
  return {};
}

function normalizeCopyCount(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.max(0, Math.min(99, Math.trunc(value)));
}

function fileLabel(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop()?.replace(/\.[^.]+$/, '') || 'Документ';
}

function cursorMarkedTemplatePath(inputPath: string, documentId: string): string {
  const extension = inputPath.match(/\.[^./\\]+$/)?.[0] ?? '.docx';
  const base = inputPath.slice(0, -extension.length);
  const safeId = documentId.replace(/[^a-zA-Z0-9_-]/g, '_');
  return `${base}.cursor-${safeId}-${Date.now()}${extension}`;
}

function replaceAllLiteral(source: string, needle: string, replacement: string): string {
  return needle ? source.split(needle).join(replacement) : source;
}

function createdPrintItems(
  created: CreatedDocumentOutput[] | undefined,
  paths: string[],
  documents: DocumentTemplateSpec[],
  requestedIds: string[] = [],
): GeneratedPrintItem[] {
  if (created?.length) return created.map((item) => ({ ...item }));
  return paths.map((path, index) => {
    const documentId = requestedIds[index] ?? `generated:${index}`;
    const document = documents.find((item) => item.id === documentId);
    return { document_id: documentId, label: document?.button_label ?? fileLabel(path), path };
  });
}

/** File.arrayBuffer с fallback на FileReader (нужен для jsdom в тестах). */
function readFileBytes(file: File): Promise<ArrayBuffer> {
  if (typeof file.arrayBuffer === 'function') return file.arrayBuffer();
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as ArrayBuffer);
    reader.onerror = () => reject(reader.error ?? new Error('file read failed'));
    reader.readAsArrayBuffer(file);
  });
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

function newDocumentId(): string {
  const random = globalThis.crypto?.randomUUID?.() ?? `${Date.now()}_${Math.random().toString(36).slice(2)}`;
  return `template_${random.replace(/[^a-zA-Z0-9_-]/g, '')}`;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try { return JSON.stringify(error); } catch { return 'Неизвестная ошибка'; }
}

function detectTitle(text: string): string | null {
  for (const raw of text.split(/\r?\n/).slice(0, 20)) {
    const line = raw.trim();
    if (!line) continue;
    return line.replace(/\s*\{\{[^}]+\}\}.*/, '').replace(/^\d{1,2}[./-]\d{1,2}[./-]\d{2,4}\s+/, '').trim() || line;
  }
  return null;
}
