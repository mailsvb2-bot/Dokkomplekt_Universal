import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { CreatedDocumentsIntakeResult, GeneratedOutput, GeneratedPrintItem, IntakeCapability, ParseSourceFileResponse, SidecarToolStatus, PrintJobDto, PrintTriageReport, SemanticExtractResult, BundleDecision, DocumentRoutingRecommendation, DocumentTemplateSpec, DomainKind, Icd10Suggestion, LearnedScannerRule, PopupFieldConfig, WorkflowPlan } from './lib/types';
import {
  activateWordScanner, analyzeTemplate, analyzeTemplateFile, applyPopup, applyPopupBatch, applyScanner, applyTemplateLearningMap, applyTemplateMarkup, applyWordScannerSelection, captureWordScanner, closeWordScanner, confirmTemplateSetup,
  getRecordSeriesPlan, getDocumentTemplateText, getIntakeCapabilities, getSidecarStatus, getComponentStatuses, installComponent, getOutputPlan, getWorkflowPlan, getWorkflowPlanBatch, icd10Suggest, loadState, parseSource, parseSourceFile, parseSourcePath, parseWebSource,
  approveDocumentTemplate, createKedoPackage, exportFilesToPdf, getPrintTriage, importLearningExampleFile, importTemplateFile, learnTemplateFromExamples, listLearnedScannerRules, openInFileManager, prepareTemplateSetup, printFiles, removeDocumentButton, renameDocumentButton, renderDocxBatch, renderPreview, resetCase, runCreatedDocumentsIntake, saveLearnedScannerRule, semanticExtract, saveState, setField, startWordScanner, uninstallBackgroundWatcher, updateDocumentPopupFields, updateDocumentTemplate,
  checkForUpdates, pickSourceFile, pickTemplateFiles, validateProductAccess, verifyRustLicenseText,
} from './lib/api';
import { ThemeSwitcher } from './components/ThemeSwitcher';
import { UtilityPanel } from './components/UtilityPanel';
import { TemplateSetupModal } from './components/TemplateSetupModal';
import { DocumentRail } from './components/DocumentRail';
import { Workspace } from './components/Workspace';
import { FolderNamingOnboarding } from './components/FolderNamingOnboarding';
import { PopupDesignerModal } from './components/PopupDesignerModal';
import { GuidedScannerModal } from './components/GuidedScannerModal';
import { GenerationPreflightModal } from './components/GenerationPreflightModal';
import { AppDialogProvider, useAppDialog } from './components/AppDialogProvider';
import { ensurePopupField, newPopupField } from './components/PopupFieldEditor';
import { bestScannerSuggestion, suggestScannerFields } from './lib/scannerSuggestions';
import { applyTheme, buildTheme, loadTheme, saveTheme, type ThemeState } from './theme';
import { useActionRunner } from './hooks/useActionRunner';
import { useGenerationPreflight, type GenerationSnapshot } from './hooks/useGenerationPreflight';
import { useOutputDestination } from './hooks/useOutputDestination';
import { useWorkspaceBootstrap } from './hooks/useWorkspaceBootstrap';
import { useWatcherPreferenceSync } from './hooks/useWatcherPreferenceSync';
import { applyWorkspaceDomainToPending, pendingTemplateCandidates, useWorkspaceProfileInference } from './hooks/useWorkspaceProfileInference';
import { normalizeCreatedDocumentsIntakeResult } from './lib/runtimeValidation';
import { buildTemplateConfirmationRows, importBrowserTemplateFiles, partitionPickedTemplates, templateButtonLabelFromFileName, uniqueTemplateButtonLabel, templatePickerCompletionMessage, templateSetupCompletionMessage } from './lib/templateSetupSupport';
import { createPendingTemplateIntelligenceHandlers } from './lib/pendingTemplateIntelligence';
import { chooseExistingOutputPolicyFlow, openCreatedOutputFolderSilently } from './lib/outputFlow';
import {
  AUTO_PRINT_KEY, PRINT_COPIES_KEY, STATE_DB,
  arrayBufferToBase64, bundleSelectionFromDecision, createdPrintItems, currentDefaultYear, jobsForItems, cursorMarkedTemplatePath, detectTitle, ensureSuggestedPopupField, generationDocumentRevisionTokens, generationDocumentRevisionsMatch,
  errorMessage, fileLabel, inferGuidedMarkupAction, loadAutoPrintPreference,
  loadPrintCopyPreferences, newDocumentId, normalizeCopyCount, preserveSelectedDocumentIds, promptToPopupField, readFileBytes,
  replaceAllLiteral, withPendingTemplateDomain, type GuidedScannerState, type PendingTemplate,
} from './lib/appSupport';
export function App() {
  return <AppDialogProvider><AppContent /></AppDialogProvider>;
}
function AppContent() {
  const dialogs = useAppDialog();
  const [theme, setTheme] = useState<ThemeState>(() => loadTheme());
  useEffect(() => { applyTheme(buildTheme(theme)); saveTheme(theme); }, [theme]);

  const [documents, setDocuments] = useState<DocumentTemplateSpec[]>([]);
  const [activeDoc, setActiveDoc] = useState<string | null>(null);
  const [selectedDocIds, setSelectedDocIds] = useState<string[]>([]);
  const manualSelectionTouched = useRef(false);
  const [status, setStatus] = useState('Загружаем сохранённый рабочий набор…');
  const { busy, run } = useActionRunner(setStatus);
  const { workspaceStateReady, workspaceStateLoading, workspaceStateError, retryWorkspaceStateLoad } = useWorkspaceBootstrap({ setDocuments, setSelectedDocIds, setStatus });

  const [sourceText, setSourceText] = useState('');
  const [sourceFileName, setSourceFileName] = useState<string | null>(null);
  const [sourceFilePath, setSourceFilePath] = useState<string | null>(null);
  const [webSourceUrl, setWebSourceUrl] = useState('');
  const [intakeCapabilities, setIntakeCapabilities] = useState<IntakeCapability[]>([]);
  const [sidecarStatuses, setSidecarStatuses] = useState<SidecarToolStatus[]>([]);
  const [parsed, setParsed] = useState<{
    title: string;
    count: number;
    warnings: string[];
    sourceKind?: string;
    layoutRows?: number;
    tableRows?: number;
  } | null>(null);
  const [plan, setPlan] = useState<WorkflowPlan | null>(null);
  const [preflightPlan, setPreflightPlan] = useState<WorkflowPlan | null>(null);
  const [preflightLoading, setPreflightLoading] = useState(false);
  const [answers, setAnswers] = useState<Record<string, string>>({}); const [skippedAnswers, setSkippedAnswers] = useState<Record<string, boolean>>({});
  const [sickLeave, setSickLeave] = useState(false);

  const [activeTemplateText, setActiveTemplateText] = useState('');
  const [preview, setPreview] = useState<{ text: string; missing: number; label: string } | null>(null);

  const [setupOpen, setSetupOpen] = useState(false);
  const [templateText, setTemplateText] = useState('');
  const [buttonLabel, setButtonLabel] = useState('');
  const [importedTemplatePath, setImportedTemplatePath] = useState<string | null>(null);
  const [pendingTemplates, setPendingTemplates] = useState<PendingTemplate[]>([]);
  const { workspaceInference, workspaceShape, setWorkspaceInference, refreshWorkspaceInference } = useWorkspaceProfileInference(setStatus, pendingTemplates);
  const [draftPopupState, setDraftPopupState] = useState<{ fields: PopupFieldConfig[]; edited: boolean }>({ fields: [], edited: false });
  const [draftDomainOverride, setDraftDomainOverride] = useState<DomainKind | null>(null);
  const [autoInferStaticTemplates, setAutoInferStaticTemplates] = useState(false);
  const [popupDesignerDocument, setPopupDesignerDocument] = useState<DocumentTemplateSpec | null>(null);
  const [popupDesignerFields, setPopupDesignerFields] = useState<PopupFieldConfig[]>([]);
  const [icdQuery, setIcdQuery] = useState('');
  const [icdHits, setIcdHits] = useState<Icd10Suggestion[]>([]);

  const [licenseText, setLicenseText] = useState('');
  const [utilityOpen, setUtilityOpen] = useState(false);

  const [intakeSource, setIntakeSource] = useState('');
  const [intakeResult, setIntakeResult] = useState<CreatedDocumentsIntakeResult | null>(null);
  const [semantic, setSemantic] = useState<SemanticExtractResult | null>(null);
  const [modelOutput, setModelOutput] = useState('');
  const [seriesStart, setSeriesStart] = useState('');
  const [seriesEnd, setSeriesEnd] = useState('');
  const [seriesSkipWeekends, setSeriesSkipWeekends] = useState(false);
  const [scannerField, setScannerField] = useState('');
  const [scannerText, setScannerText] = useState('');
  const {
    watchFolder, outputRoot, outputRootDraft, folderParts, folderNamingConfirmed, outputPreferencesReady, outputRootRecoveryRequired, watcherRefreshRevision,
    setOutputRootDraft, setFolderNamingConfirmed, updateFolderParts, commitOutputRoot,
    chooseAndCommitOutputFolder, chooseWatchFolder, outputPlan, installWatcher, uninstallWatcher,
  } = useOutputDestination(run, setStatus);
  const [autoPrint, setAutoPrint] = useState(loadAutoPrintPreference);
  const [printCopies, setPrintCopies] = useState<Record<string, number>>(loadPrintCopyPreferences);
  const [lastOutput, setLastOutput] = useState<GeneratedOutput | null>(null);
  const [guidedScanner, setGuidedScanner] = useState<GuidedScannerState | null>(null);

  const { markupPendingTemplate, learnPendingTemplateFromExamples } = createPendingTemplateIntelligenceHandlers({
  pendingTemplates,
  setPendingTemplates,
  importedTemplatePath,
  setImportedTemplatePath,
  templateText,
  setTemplateText,
  setStatus,
  run,
  confirm: dialogs.confirm,
});

  useEffect(() => {
    let alive = true;
    void getIntakeCapabilities()
      .then((items) => { if (alive) setIntakeCapabilities(items); })
      .catch(() => { /* browser/tests */ });
    void getSidecarStatus()
      .then((items) => { if (alive) setSidecarStatuses(items); })
      .catch(() => { /* browser/tests */ });
    return () => { alive = false; };
  }, []);

  useWatcherPreferenceSync({
    outputPreferencesReady, watcherRefreshRevision, folderNamingConfirmed, outputRoot, folderParts, autoPrint, printCopies,
    setAutoPrint, setPrintCopies, setStatus,
  });

  useEffect(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    listen<unknown>('document-batch-ready', (event) => {
      try {
        const result = normalizeCreatedDocumentsIntakeResult(event.payload);
        setLastOutput(null);
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
      } catch (error) {
        setStatus(`Фоновая обработка вернула некорректный результат: ${errorMessage(error)}`);
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
  const activeDocumentLabel = activeDoc
    ? documents.find((document) => document.id === activeDoc)?.button_label ?? null
    : null;
  const showSickLeaveOption = selectedDocIds.some((documentId) => {
    const document = documents.find((item) => item.id === documentId);
    const role = document?.role_id.toLowerCase() ?? '';
    return document?.category === 'Medical' && (role === 'discharge' || role.endsWith('.discharge'));
  });

  useEffect(() => {
    const existing = new Set(documents.map((document) => document.id));
    setSelectedDocIds((previous) => previous.filter((id) => existing.has(id)));
  }, [documents]);

  useEffect(() => {
    if (!showSickLeaveOption && sickLeave) setSickLeave(false);
  }, [showSickLeaveOption, sickLeave]);

  useEffect(() => {
    let cancelled = false;
    const sourceReady = Boolean(sourceFileName || parsed);
    if (!sourceReady || selectedDocIds.length === 0) {
      setPreflightPlan(null);
      setPreflightLoading(false);
      setAnswers({}); setSkippedAnswers({});
      return () => { cancelled = true; };
    }

    setPreflightLoading(true);
    void loadWorkflowPlan(selectedDocIds)
      .then((workflow) => {
        if (cancelled) return;
        setPreflightPlan(workflow);
        setAnswers((previous) => Object.fromEntries(
          workflow.prompts.map((prompt) => [prompt.field_id, previous[prompt.field_id] ?? prompt.current_value ?? '']),
        ));
        setSkippedAnswers((previous) => Object.fromEntries(workflow.prompts.filter((prompt) => previous[prompt.field_id]).map((prompt) => [prompt.field_id, true])));
      })
      .catch((error) => {
        if (!cancelled) {
          setPreflightPlan(null);
          setStatus(`Не удалось проверить выбранный комплект: ${errorMessage(error)}`);
        }
      })
      .finally(() => { if (!cancelled) setPreflightLoading(false); });
    return () => { cancelled = true; };
  }, [sourceFileName, parsed, selectedDocIds, sickLeave, folderParts]);

  const previewTitle = detectTitle(templateText) || 'Документ';
  const previewLabel = buttonLabel.trim() || previewTitle;

  function requiredToolsAvailable(statuses: SidecarToolStatus[], requiredTools: string[]): boolean {
    return requiredTools.every((tool) => statuses.some((status) => status.tool === tool && status.available));
  }

  async function currentSidecarStatuses(): Promise<SidecarToolStatus[]> {
    try {
      const statuses = await getSidecarStatus();
      setSidecarStatuses(statuses);
      return statuses;
    } catch {
      return sidecarStatuses;
    }
  }

  async function ensureOptionalComponent(id: string, fallbackLabel: string, requiredTools: string[] = []): Promise<boolean> {
    const runtimeStatuses = await currentSidecarStatuses();
    if (requiredTools.length && requiredToolsAvailable(runtimeStatuses, requiredTools)) return true;

    const statuses = await run('get_component_statuses', () => getComponentStatuses());
    if (!statuses) return false;
    const component = statuses.find(item => item.id === id);
    if (component?.available || component?.installed) {
      if (!requiredTools.length) return true;
      const refreshed = await currentSidecarStatuses();
      if (requiredToolsAvailable(refreshed, requiredTools)) return true;
      setStatus(`${component.label || fallbackLabel}: компонент отмечен установленным, но требуемые программы не запускаются.`);
      return false;
    }
    const label = component?.label || fallbackLabel;
    const size = component?.size_label || 'размер будет показан после проверки подписанного каталога';
    const accepted = await dialogs.confirm({
      title: `Установить компонент «${label}»?`,
      message: `Размер: ${size}. Разовая загрузка; после установки компонент работает офлайн.`,
      confirmLabel: 'Скачать и установить',
    });
    if (!accepted) {
      setStatus(`${label}: пользователь отказался от загрузки; функция не запущена.`);
      return false;
    }
    const installed = await run('install_component', () => installComponent(id));
    if (!installed?.installed) return false;
    const refreshed = await currentSidecarStatuses();
    if (requiredTools.length && !requiredToolsAvailable(refreshed, requiredTools)) {
      setStatus(`${installed.label}: загрузка завершена, но требуемые программы не прошли проверку запуска.`);
      return false;
    }
    setStatus(`${installed.label}: компонент установлен и доступен офлайн.`);
    return true;
  }

  async function ensureComponentForSource(fileName: string): Promise<boolean> {
    const extension = fileName.split('.').at(-1)?.toLowerCase() || '';
    if (['jpg', 'jpeg', 'png', 'tif', 'tiff', 'bmp', 'webp'].includes(extension)) {
      return ensureOptionalComponent('ocr', 'Распознавание сканов (OCR)', ['tesseract']);
    }
    if (extension === 'pdf') {
      const statuses = await currentSidecarStatuses();
      // Do not force a large OCR download when the installed runtime can at
      // least inspect the PDF text layer. Image-only pages still fail closed in
      // the backend unless pdftoppm and Tesseract are actually available.
      if (requiredToolsAvailable(statuses, ['pdftotext'])) return true;
      return ensureOptionalComponent('ocr', 'Распознавание PDF и сканов', ['pdftotext', 'pdftoppm', 'tesseract']);
    }
    if (['doc', 'xls', 'ods'].includes(extension)) {
      return ensureOptionalComponent('office', 'Конвертация Office-файлов', ['soffice']);
    }
    if (['ppt', 'pptx'].includes(extension)) {
      const officeReady = await ensureOptionalComponent('office', 'Конвертация презентаций', ['soffice']);
      if (!officeReady) return false;
      return ensureOptionalComponent('ocr', 'Извлечение текста презентаций', ['pdftotext']);
    }
    if (['7z', 'rar'].includes(extension)) {
      return ensureOptionalComponent('archive', 'Распаковка входящих архивов', ['7z']);
    }
    return true;
  }

  function clearSourceScopedUiState() {
    setSemantic(null);
    setAnswers({});
    setSkippedAnswers({});
    setPlan(null);
    setPreflightPlan(null);
    setPreview(null);
    setScannerField('');
    setScannerText('');
    setModelOutput('');
    setIntakeResult(null);
    setLastOutput(null);
  }

  async function resetCurrentCase() {
    const cleared = await run('reset_case', () => resetCase());
    if (!cleared) return;
    setSourceText('');
    setSourceFileName(null);
    setSourceFilePath(null);
    setWebSourceUrl('');
    setParsed(null);
    clearSourceScopedUiState();
    manualSelectionTouched.current = false;
    setSelectedDocIds([]);
    setSickLeave(false);
    setStatus('Новый комплект начат. Данные и результаты предыдущего комплекта очищены.');
  }

  function applyBundleDecision(decision: BundleDecision, routing: DocumentRoutingRecommendation): string {
    const selection = bundleSelectionFromDecision(decision, routing, documents);
    if (!manualSelectionTouched.current) {
      setSelectedDocIds(selection.documentIds);
      return selection.summary;
    }
    return `${selection.summary} Ручной выбор документов сохранён и не был заменён автоматически.`;
  }

  async function parseSourceNow() {
    const res = await run('parse_source', () => parseSource(sourceText, currentDefaultYear()));
    if (!res) return;
    setSourceFileName(null);
    setSourceFilePath(null);
    setWebSourceUrl('');
    clearSourceScopedUiState();
    const semanticResult = await run('semantic_extract', () => semanticExtract(sourceText, currentDefaultYear()));
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
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Источник прочитан. Найдено значений: ${count}.${routingSummary}`);
  }

  async function applyParsedSourceFile(res: ParseSourceFileResponse, fileName: string) {
    clearSourceScopedUiState();
    setSourceFileName(fileName);
    setSourceFilePath(res.source_path);
    setSourceText(res.source_text);
    setWebSourceUrl('');
    const semanticResult = await run('semantic_extract', () => semanticExtract(res.source_text, currentDefaultYear()));
    setSemantic(semanticResult ?? null);
    const count = Object.keys(res.semantic_case?.values ?? {}).length;
    const layoutItems = res.layout_items ?? [];
    setParsed({
      title: res.report?.recognized_title ?? fileName,
      count,
      warnings: res.report?.warnings ?? [],
      sourceKind: res.source_kind ?? 'file',
      layoutRows: layoutItems.length,
      tableRows: layoutItems.filter((item) => item.item_kind === 'table_row').length,
    });
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Файл «${fileName}» прочитан. Найдено значений: ${count}.${routingSummary}`);
  }

  async function pickSourceFileNative() {
    const picked = await run('pick_source_file', () => pickSourceFile());
    if (!picked || !(await ensureComponentForSource(picked.file_name))) return;
    const res = await run('parse_source_path', () => parseSourcePath(picked.selected_path, currentDefaultYear()));
    if (!res) return;
    await applyParsedSourceFile(res, picked.file_name);
  }

  async function processSourceFile(file: File) {
    if (!(await ensureComponentForSource(file.name))) return;
    const buffer = await readFileBytes(file);
    const res = await run('parse_source_file', () =>
      parseSourceFile(file.name, arrayBufferToBase64(buffer), currentDefaultYear()));
    if (!res) return;
    await applyParsedSourceFile(res, file.name);
  }


  async function loadWebSource() {
    const url = webSourceUrl.trim();
    if (!url) {
      setStatus('Укажите HTTPS-адрес сайта или API.');
      return;
    }
    const res = await run('parse_web_source', () => parseWebSource(url, currentDefaultYear()));
    if (!res) return;
    clearSourceScopedUiState();
    setSourceFileName(res.final_url);
    setSourceFilePath(null);
    setSourceText(res.source_text);
    const semanticResult = await run('semantic_extract', () => semanticExtract(res.source_text, currentDefaultYear()));
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
    const routingSummary = applyBundleDecision(res.bundle_decision, res.routing);
    setStatus(`Источник загружен. Найдено значений: ${count}.${routingSummary}`);
  }

  async function selectDocument(doc: DocumentTemplateSpec) {
    manualSelectionTouched.current = true;
    setSelectedDocIds((previous) => previous.includes(doc.id) ? previous : [...previous, doc.id]);
    setActiveDoc(doc.id);
    setPreview(null);
    const [workflow, template] = await Promise.all([
      run('get_workflow_plan', () => getWorkflowPlan(doc.id, sickLeave)),
      run('get_document_template_text', () => getDocumentTemplateText(doc.id)),
    ]);
    if (template) setActiveTemplateText(template.template_text);
    if (!workflow) return;
    setPlan(workflow);
    setStatus(workflow.prompts.length ? `Требуется уточнить полей: ${workflow.prompts.length}.` : 'Все поля распознаны — документ готов.');
  }

  function toggleDocumentSelected(documentId: string) {
    manualSelectionTouched.current = true;
    setSelectedDocIds((previous) => previous.includes(documentId)
      ? previous.filter((id) => id !== documentId)
      : [...previous, documentId]);
  }

  function selectAllVisibleDocuments() {
    manualSelectionTouched.current = true;
    const visibleIds = visibleDocs.map((document) => document.id);
    setSelectedDocIds((previous) => Array.from(new Set([...previous, ...visibleIds])));
  }

  function clearSelectedDocuments() {
    manualSelectionTouched.current = true;
    setSelectedDocIds([]);
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
    const opened = await openCreatedOutputFolderSilently(target, openInFileManager);
    setStatus(opened.opened
      ? 'Папка готового комплекта открыта.'
      : `Не удалось открыть папку готового комплекта: ${opened.error}. Путь: ${target}`);
  }

  async function printLastOutput() {
    if (!lastOutput) return;
    const items = lastOutput.print_items?.length
      ? lastOutput.print_items
      : lastOutput.files.map((path, index) => ({ document_id: `generated:${index}`, label: fileLabel(path), path }));
    await queuePrint(jobsForItems(items, printCopies));
  }

  async function exportLastOutput(pdfa1: boolean) {
    if (!lastOutput?.files.length) return;
    if (!(await ensureOptionalComponent('office', 'Конвертация и печать (LibreOffice)', ['soffice']))) return;
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
    const packageRoot = lastOutput.folder || outputRoot.trim();
    if (!packageRoot) { setStatus('Папка готовых документов не определена. Пакет обмена не создан.'); return; }
    const result = await run('create_kedo_package', () => createKedoPackage(lastOutput.files, packageRoot));
    if (!result) return;
    setStatus(`Пакет обмена создан: ${result.package_folder}.`);
  }

  async function chooseExistingOutputPolicy(snapshot: GenerationSnapshot, onError?: (detail: string) => void) {
    const labels = snapshot.documentIds.map(id => documents.find(document => document.id === id)?.button_label).filter((value): value is string => Boolean(value));
    return chooseExistingOutputPolicyFlow({
      outputRoot: snapshot.outputRoot, folderParts: snapshot.folderParts, labels,
      getPlan: (root, parts, names) => run('get_output_plan', () => getOutputPlan(root, parts, names), onError),
      confirm: (options) => dialogs.confirm(options),
      openFolder: openInFileManager,
      onStatus: setStatus, onMissingRoot: () => setFolderNamingConfirmed(false),
    });
  }

  async function performGenerateSelectedDocuments(snapshot: GenerationSnapshot): Promise<string | null> {
    if (!generationDocumentRevisionsMatch(snapshot.documentRevisionTokens, documents)) {
      return 'Комплект изменился после проверки. Нажмите «Проверить и создать» ещё раз. Ничего не создано.';
    }
    if (!snapshot.outputRoot.trim()) {
      setFolderNamingConfirmed(false);
      return 'Папка готовых документов не определена. Выберите папку и повторите создание.';
    }
    let policyError: string | null = null;
    const existingOutputPolicy = await chooseExistingOutputPolicy(snapshot, (detail) => { policyError = detail; });
    if (!existingOutputPolicy) return policyError ? `Не удалось подготовить папку результата: ${policyError}` : null;
    // The previous successful batch remains useful while the user only reviews or
    // cancels preflight. Once a new render actually starts it is no longer the
    // current result and must not survive a failed attempt as a false green state.
    setLastOutput(null);
    let renderError: string | null = null;
    const res = await run(
      'render_docx_batch',
      () => renderDocxBatch(snapshot.documentIds, snapshot.outputRoot, snapshot.folderParts, true, existingOutputPolicy, snapshot.sickLeaveEnabled),
      (detail) => { renderError = detail; },
    );
    if (!res) return `Не удалось создать документы: ${renderError ?? 'backend не вернул результат генерации.'}`;
    const printItems = createdPrintItems(res.created_documents, res.created_files, documents, snapshot.documentIds);
    setLastOutput({ folder: res.output_folder, files: res.created_files, source: 'batch', print_items: printItems });
    const backupNote = res.backup_folder ? ` Предыдущий комплект сохранён: ${res.backup_folder}.` : '';
    const creationStatus = res.warnings?.length
      ? `Комплект создан: ${res.created_files.length} документ(ов) в ${res.output_folder}.${backupNote} Требует внимания: ${res.warnings.join(' ')}`
      : `Комплект создан: ${res.created_files.length} документ(ов) в ${res.output_folder}.${backupNote}`;
    const opened = await openCreatedOutputFolderSilently(res.output_folder, openInFileManager);
    setStatus(opened.opened
      ? creationStatus
      : `${creationStatus} Папка не открылась автоматически: ${opened.error}. Используйте кнопку «Открыть папку с документами».`);
    if (autoPrint) await queuePrint(jobsForItems(printItems, printCopies), true, snapshot.documentIds, null, res.output_folder);
    return null;
  }

  function changeGenerationSickLeave(value: boolean) {
    setSickLeave(value); closeGenerationPreflight();
    setStatus('Параметр больничного изменён. Нажмите «Проверить и создать» ещё раз, чтобы пересчитать обязательные вопросы.');
  }
  const loadWorkflowPlan = (documentIds: string[], sickLeaveEnabled = sickLeave, parts = folderParts) => documentIds.length === 1 ? getWorkflowPlan(documentIds[0], sickLeaveEnabled, parts) : getWorkflowPlanBatch(documentIds, sickLeaveEnabled, parts);
  const { generationPreflightOpen, generationDocumentIds, generationError, generationValidationFieldId, closeGenerationPreflight, openGenerationPreflight, confirmGenerationPreflight } = useGenerationPreflight({
    selectedDocumentIds: selectedDocIds, sickLeaveEnabled: sickLeave, folderParts, outputRoot, documentRevisionTokens: generationDocumentRevisionTokens(documents, selectedDocIds),
    preflightPlan, preflightLoading, answers, skippedAnswers, setPreflightPlan, setStatus,
    requestWorkflowPlan: (snapshot) => run(snapshot.documentIds.length === 1 ? 'get_workflow_plan' : 'get_workflow_plan_batch', () => loadWorkflowPlan(snapshot.documentIds, snapshot.sickLeaveEnabled, snapshot.folderParts)),
    applyAnswers: (snapshot, payload) => snapshot.documentIds.length === 1
      ? run('apply_popup', () => applyPopup(snapshot.documentIds[0], snapshot.sickLeaveEnabled, payload, snapshot.folderParts))
      : run('apply_popup_batch', () => applyPopupBatch(snapshot.documentIds, snapshot.sickLeaveEnabled, payload, snapshot.folderParts)),
    onConfirmed: performGenerateSelectedDocuments,
  });

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
    const requested = await dialogs.prompt({
      title: 'Переименовать документ',
      label: 'Новое название кнопки',
      initialValue: current?.button_label ?? '',
      required: true,
      confirmLabel: 'Переименовать',
    });
    if (!requested || requested === current?.button_label) return;
    const pack = await run('rename_document_button', () => renameDocumentButton(activeDoc, requested));
    if (!pack) return;
    setDocuments(pack.documents);
    setStatus(`Документ переименован: ${requested}. Исходный шаблон не изменён.`);
  }

  async function approveActiveTemplate() {
    if (!activeDoc) return;
    const current = documents.find((document) => document.id === activeDoc);
    const approvalForm = await dialogs.form({
      title: 'Подтвердить версию шаблона',
      message: `Подтверждение относится только к точной ревизии «${current?.button_label ?? activeDoc}».`,
      fields: [
        { name: 'jurisdiction', label: 'Юрисдикция', initialValue: 'Российская Федерация', required: true },
        { name: 'approvedBy', label: 'Кто утвердил форму (ФИО или роль)', required: true },
      ],
      acknowledgement: {
        label: 'Организация проверила шаблон и принимает ответственность за его применение.',
        required: true,
      },
      confirmLabel: 'Подтвердить версию',
    });
    if (!approvalForm) return;
    const jurisdiction = approvalForm.jurisdiction;
    const approvedBy = approvalForm.approvedBy;
    const acknowledgement = true;
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
    const confirmed = await dialogs.confirm({
      title: 'Убрать документ из набора?',
      message: `Документ «${current?.button_label ?? activeDoc}» исчезнет из списка, но файл шаблона останется на диске.`,
      confirmLabel: 'Убрать из набора',
      danger: true,
    });
    if (!confirmed) return;
    const pack = await run('remove_document_button', () => removeDocumentButton(activeDoc));
    if (!pack) return;
    setDocuments(pack.documents);
    setActiveDoc(null);
    setPlan(null);
    setPreflightPlan(null);
    setPreview(null);
    setStatus('Документ убран из набора. Исходный шаблон сохранён.');
  }

  async function refreshPreflightPlan(documentIds = selectedDocIds) {
    if (!documentIds.length) {
      setPreflightPlan(null);
      setAnswers({}); setSkippedAnswers({});
      return;
    }
    const workflow = await loadWorkflowPlan(documentIds);
    setPreflightPlan(workflow);
    setAnswers(Object.fromEntries(workflow.prompts.map((prompt) => [prompt.field_id, prompt.current_value ?? ''])));
    setSkippedAnswers((previous) => Object.fromEntries(workflow.prompts.filter((prompt) => previous[prompt.field_id]).map((prompt) => [prompt.field_id, true])));
  }

  async function pinField(fieldId: string) {
    const value = answers[fieldId] ?? '';
    const saved = await run('set_field', () => setField(fieldId, value));
    if (!saved) return;
    await refreshPreflightPlan();
    setStatus('Значение сохранено и будет использовано в других документах комплекта.');
  }

  async function previewNow() {
    if (!activeDoc) {
      setStatus('Откройте документ справа, чтобы посмотреть его перед созданием комплекта.');
      return;
    }
    const document = documents.find((item) => item.id === activeDoc);
    if (!document) return;
    const res = await run('render_preview', () => renderPreview(activeTemplateText, false));
    if (!res) return;
    setPreview({ text: res.output_text ?? '', missing: res.missing_fields?.length ?? 0, label: document.button_label });
    setStatus(res.missing_fields?.length
      ? `Предпросмотр «${document.button_label}»: не заполнено полей — ${res.missing_fields.length}.`
      : `Предпросмотр «${document.button_label}» готов.`);
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

  async function openTemplateSetup() {
    setAutoInferStaticTemplates(false);
    setTemplateText(''); setButtonLabel(''); setImportedTemplatePath(null);
    setPendingTemplates([]); setDraftPopupState({ fields: [], edited: false }); setDraftDomainOverride(null);
    setSetupOpen(false);
    setStatus('Выберите шаблоны Word в системном окне…');

    const picked = await run('pick_template_files', () => pickTemplateFiles());
    if (!picked) return;
    if (!picked.length) {
      setStatus('Выбор шаблонов отменён. Нажмите «Создать свои кнопки», когда будете готовы.');
      return;
    }

    const { acceptedTemplates, rejectedTemplates, rejectedDetails } = partitionPickedTemplates(picked);
    const importedRows: PendingTemplate[] = [];
    const usedLabels = new Set<string>();
    for (const file of acceptedTemplates) {
      const id = newDocumentId();
      const detectedLabel = uniqueTemplateButtonLabel(templateButtonLabelFromFileName(file.file_name), usedLabels);
      const analyzed = await run('analyze_template_file', () => analyzeTemplateFile(file.template_path, id, detectedLabel));
      if (!analyzed) continue;
      importedRows.push({
        document_id: id,
        template_path: file.template_path,
        extracted_text: file.extracted_text,
        file_name: file.file_name,
        button_label: detectedLabel,
        popup_fields: analyzed.document.popup_fields ?? [],
        domain_override: null,
      });
    }
    if (!importedRows.length) {
      setStatus(rejectedDetails
        ? `Не удалось подготовить выбранные шаблоны. ${rejectedDetails}`
        : 'Не удалось подготовить выбранные шаблоны. Проверьте, что это безопасные DOCX без макросов и внешних связей.');
      return;
    }

    setPendingTemplates(importedRows); await refreshWorkspaceInference(importedRows);
    const last = importedRows.at(-1)!;
    setImportedTemplatePath(last.template_path);
    setTemplateText(last.extracted_text);
    setButtonLabel(last.button_label);
    setSetupOpen(true);
    setStatus(templatePickerCompletionMessage(importedRows.length, rejectedTemplates));
  }

  function openTextTemplateSetup() {
    setAutoInferStaticTemplates(false);
    setTemplateText('');
    setButtonLabel('');
    setImportedTemplatePath(null);
    setPendingTemplates([]);
    setDraftPopupState({ fields: [], edited: false }); setDraftDomainOverride(null);
    setSetupOpen(true);
    setStatus('Вставьте текст документа, проверьте название кнопки и создайте шаблон.');
  }

  async function processTemplateFiles(files: File[]) {
    const imported = await run('import_template_files', () => importBrowserTemplateFiles(files, {
      readFileBytes, importTemplateFile, analyzeTemplateFile,
    }));
    if (!imported) return;
    const { importedRows, rejectedTemplates } = imported;
    if (!importedRows.length) {
      setStatus(rejectedTemplates.length
        ? `Не удалось подготовить выбранные шаблоны. ${rejectedTemplates.map(file => `${file.file_name}: ${file.import_error}`).join('; ')}`
        : 'Шаблоны должны быть в формате DOCX или DOCM.');
      return;
    }
    const usedLabels = new Set(pendingTemplates.map((item) => item.button_label.trim().replace(/\s+/g, ' ').replace(/ё/gi, 'е').toLocaleLowerCase('ru-RU')));
    const uniqueImportedRows = importedRows.map((item) => ({
      ...item,
      button_label: uniqueTemplateButtonLabel(item.button_label, usedLabels),
    }));
    const combinedTemplates = [...pendingTemplates, ...uniqueImportedRows];
    setPendingTemplates(combinedTemplates); await refreshWorkspaceInference(combinedTemplates);
    const last = uniqueImportedRows.at(-1)!;
    setImportedTemplatePath(last.template_path);
    setTemplateText(last.extracted_text);
    setButtonLabel(last.button_label);
    setStatus(templatePickerCompletionMessage(importedRows.length, rejectedTemplates));
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
      item.document_id === documentId ? { ...item, popup_fields: fields, popup_fields_edited: true } : item
    )));
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
            popup_fields_edited: item.popup_fields_edited || current.addQuestion,
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
      ? pendingTemplateCandidates(pendingTemplates)
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
    const confirmedRows = buildTemplateConfirmationRows(rows, pendingTemplates, buttonLabel, draftPopupState.fields, draftDomainOverride, draftPopupState.edited);
    const staticRows = confirmedRows.filter((row) => row.is_static_copy);
    if (staticRows.length) {
      setStatus(autoInferStaticTemplates
        ? `Безопасная авторазметка включена для ${staticRows.length} статического шаблона(ов): изменяться будут только производные копии с однозначными пустыми зонами.`
        : `Кнопки будут созданы сразу. Неразмеченные шаблоны останутся точными статическими копиями: ${staticRows.map((row) => row.detected_title).join(', ')}.`);
    }
    const previousDocumentIds = new Set(documents.map((document) => document.id));
    const pack = await run('confirm_template_setup', () => confirmTemplateSetup(confirmedRows, autoInferStaticTemplates));
    if (!pack) return;
    const createdCount = pack.documents.filter((document) => !previousDocumentIds.has(document.id)).length;
    setDocuments(pack.documents);
    setSelectedDocIds((previous) => preserveSelectedDocumentIds(previous, pack.documents));
    setActiveTemplateText(templateText);
    setImportedTemplatePath(null);
    setPendingTemplates([]); setWorkspaceInference(null);
    setDraftPopupState({ fields: [], edited: false }); setDraftDomainOverride(null);
    setAutoInferStaticTemplates(false);
    setSetupOpen(false);
    setStatus(templateSetupCompletionMessage(confirmedRows.length, createdCount));
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
      default_year: currentDefaultYear(),
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
  async function saveSession() {
    await run('save_state', () => saveState(STATE_DB));
    setStatus('Настройки и текущий набор сохранены.');
  }
  async function loadSession() {
    const res = await run('load_state', () => loadState(STATE_DB));
    if (res?.pack?.documents) {
      setDocuments(res.pack.documents);
      const existingIds = new Set(res.pack.documents.map((document) => document.id));
      setSelectedDocIds((previous) => previous.filter((id) => existingIds.has(id)));
      setStatus(`Рабочий набор загружен: ${res.pack.documents.length} документ(ов).`);
    }
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

  async function runZeroTouch() {
    if (!intakeSource.trim()) {
      setStatus('Укажите путь к исходному файлу поддерживаемого формата.');
      return;
    }
    if (!outputRoot.trim()) {
      setFolderNamingConfirmed(false);
      setStatus('Сначала сохраните проверенную папку готовых документов. Ничего не создано.');
      return;
    }
    setIntakeResult(null);
    setLastOutput(null);
    const res = await run('run_created_documents_intake', () =>
      runCreatedDocumentsIntake(intakeSource.trim(), outputRoot, folderParts, currentDefaultYear(), sickLeave));
    if (!res) return;
    setIntakeResult(res);
    setStatus(res.message);
    if (res.status === 'processed' && res.created_files.length) {
      const printItems = createdPrintItems(res.created_documents, res.created_files, documents);
      setLastOutput({ folder: res.patient_folder, files: res.created_files, source: 'zero_touch', print_items: printItems });
      if (autoPrint) await queuePrint(jobsForItems(printItems, printCopies), true, printItems.map((item) => item.document_id), res.print_triage ?? null, res.patient_folder);
    }
  }

  async function understand() {
    const res = await run('semantic_extract', () => semanticExtract(sourceText, currentDefaultYear(), modelOutput.trim() || undefined));
    if (!res) return;
    setSemantic(res);
    setStatus(res.model_applied
      ? `Проверка завершена: найдено значений — ${res.fields.length}.`
      : `Проверка завершена: уверенно найдено значений — ${res.fields.length}.`);
  }


  const interactionBusy = busy || workspaceStateLoading || !workspaceStateReady;

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

        {workspaceStateError && (
          <section className="startupRecovery" role="alert" aria-label="Не удалось загрузить рабочий набор">
            <div>
              <strong>Рабочий набор не загружен</strong>
              <span>Чтобы не потерять сохранённые кнопки и настройки, создание нового комплекта заблокировано до успешного чтения состояния.</span>
              <small>{workspaceStateError}</small>
            </div>
            <button className="primaryBtn" type="button" onClick={() => { void retryWorkspaceStateLoad(); }} disabled={workspaceStateLoading}>Повторить загрузку</button>
          </section>
        )}

        <div className="grid clientGrid">
          <Workspace
            busy={interactionBusy}
            documents={documents}
            selectedDocumentIds={selectedDocIds}
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
            plan={preflightPlan}
            planLoading={preflightLoading}
            selectedDocumentCount={selectedDocIds.length}
            activeDocumentLabel={activeDocumentLabel}
            showSickLeaveOption={showSickLeaveOption}
            sickLeaveEnabled={sickLeave}
            answers={answers}
            skippedAnswers={skippedAnswers}
            preview={preview}
            onPickWatchFolder={() => void chooseWatchFolder()}
            setIntakeSource={setIntakeSource}
            setAutoPrint={updateAutoPrint}
            setSourceText={setSourceText}
            setSourceFileName={(value) => { setSourceFileName(value); if (value === null) setSourceFilePath(null); }}
            setWebSourceUrl={setWebSourceUrl}
            setScannerField={setScannerField}
            setScannerText={setScannerText}
            setModelOutput={setModelOutput}
            setAnswers={setAnswers}
            setSkippedAnswers={setSkippedAnswers}
            onSickLeaveChange={changeGenerationSickLeave}
            onRunZeroTouch={runZeroTouch}
            onOpenLastOutput={openLastOutput}
            onPrintLastOutput={printLastOutput}
            onExportLastOutputPdf={() => void exportLastOutput(false)}
            onExportLastOutputPdfa={() => void exportLastOutput(true)}
            onExportLastOutputKedo={() => void exportLastOutputKedo()}
            onPickSourceFile={() => { void pickSourceFileNative(); }}
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
            onCreateSelected={openGenerationPreflight}
          />
          <DocumentRail
            documents={visibleDocs}
            activeDocumentId={activeDoc}
            selectedDocumentIds={selectedDocIds}
            busy={interactionBusy}
            workspaceStateReady={workspaceStateReady}
            printCopies={printCopies}
            onSelect={selectDocument}
            onToggleSelected={toggleDocumentSelected}
            onPrintCopiesChange={updatePrintCopies}
            onSelectAll={selectAllVisibleDocuments}
            onClearSelected={clearSelectedDocuments}
            onRename={renameActiveDocument}
            onConfigurePopups={openPopupDesigner}
            onScanTemplate={startGuidedExistingTemplateScanner}
            onRemove={removeActiveDocument}
            onApprove={approveActiveTemplate}
            onAdd={openTemplateSetup}
            onAddFromText={openTextTemplateSetup}
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
            outputRoot={outputRootDraft}
            savedOutputRoot={outputRoot}
            folderParts={folderParts}
            licenseText={licenseText}
            onSeriesStartChange={setSeriesStart}
            onSeriesEndChange={setSeriesEnd}
            onSeriesSkipWeekendsChange={setSeriesSkipWeekends}
            onScannerFieldChange={setScannerField}
            onScannerTextChange={setScannerText}
            onOutputRootChange={setOutputRootDraft}
            onPickOutputFolder={() => void chooseAndCommitOutputFolder()}
            onSaveOutputFolder={() => void commitOutputRoot(outputRootDraft)}
            onFolderPartsChange={updateFolderParts}
            onLicenseTextChange={setLicenseText}
            onSeriesPlan={seriesPlan}
            onScanMarks={() => void scanMarks(false)}
            onOutputPlan={() => void outputPlan(visibleDocs.map((document) => document.button_label))}
            onSaveSession={saveSession}
            onLoadSession={loadSession}
            onCheckAccess={checkAccess}
            onCheckUpdates={checkUpdates}
            onInstallWatcher={() => void installWatcher(sickLeave, autoPrint, printCopies)}
            onUninstallWatcher={() => void uninstallWatcher()}
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

        {outputRootRecoveryRequired && (
          <div role="alert">Не удалось подготовить папку готовых документов</div>
        )}

        <footer className="statusBar">
          <span className={busy ? 'dot busy' : 'dot'} aria-hidden="true" />
          {status}
        </footer>
      </div>

      {(!folderNamingConfirmed || !outputRoot.trim()) && (
        <FolderNamingOnboarding
          currentRoot={outputRoot}
          currentParts={folderParts}
          onPickRoot={() => void chooseAndCommitOutputFolder()}
          onConfirm={(parts) => { void updateFolderParts(parts); }}
        />
      )}

      {setupOpen && (
        <TemplateSetupModal
          templateText={templateText}
          buttonLabel={buttonLabel}
          previewTitle={previewTitle}
          pendingTemplates={pendingTemplates}
          draftPopupFields={draftPopupState.fields}
          draftDomainOverride={draftDomainOverride}
          autoInferStaticTemplates={autoInferStaticTemplates}
          workspaceInference={workspaceInference} workspaceShape={workspaceShape}
          onTemplateTextChange={setTemplateText}
          onButtonLabelChange={setButtonLabel}
          onDraftPopupFieldsChange={(fields) => setDraftPopupState({ fields, edited: true })}
          onDraftDomainOverrideChange={setDraftDomainOverride}
          onAutoInferStaticTemplatesChange={setAutoInferStaticTemplates}
          onPendingTemplateLabelChange={updatePendingTemplateLabel}
          onPendingTemplateDomainChange={(documentId, value) => setPendingTemplates((previous) => withPendingTemplateDomain(previous, documentId, value))}
          onApplyWorkspaceDomain={(domain) => { setPendingTemplates((previous) => applyWorkspaceDomainToPending(previous, domain)); setStatus('Предложенный рабочий профиль применён ко всем подготовленным кнопкам.'); }}
          onPendingPopupFieldsChange={updatePendingPopupFields}
          onMarkupPendingTemplate={markupPendingTemplate}
          onLearnPendingTemplate={learnPendingTemplateFromExamples}
          onStartGuidedPendingScanner={startGuidedPendingTemplateScanner}
          onAnalyze={analyzeInDialog}
          onPickFile={pickTemplateFile}
          onDropFiles={processTemplateFiles}
          onCancel={() => setSetupOpen(false)}
          onConfirm={createButtonFromTemplate}
        />
      )}

      {generationPreflightOpen && preflightPlan && (
        <GenerationPreflightModal
          plan={preflightPlan}
          documents={documents}
          selectedDocumentIds={generationDocumentIds}
          answers={answers}
          skippedAnswers={skippedAnswers}
          busy={busy}
          loading={preflightLoading}
          generationError={generationError}
          invalidFieldId={generationValidationFieldId}
          showSickLeaveOption={showSickLeaveOption}
          sickLeaveEnabled={sickLeave}
          setAnswers={setAnswers}
          setSkippedAnswers={setSkippedAnswers}
          onSickLeaveChange={setSickLeave}
          onCancel={closeGenerationPreflight}
          onConfirm={() => void confirmGenerationPreflight()}
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
