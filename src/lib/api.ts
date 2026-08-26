import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { validateRustResponse } from './runtimeValidation';
import type { BusinessRegistryImportResult, BusinessRegistryRecord, OrganizationKnowledgeRecord, OrganizationKnowledgeCategory, CalibratedThresholdStatus, AuditEventRecord, AutomationExceptionRecord, AutomationMetrics, DailyAutomationDashboard, QueueStatus, CorpusStatus, QualityTelemetryReport, CorpusExportResponse, CaseRunRecord, PrivacyPreferences, WorkspaceHygieneReport, LocalSemanticModelConfig, LocalSemanticModelStatus, SemanticModelConfigurationResponse, ReferenceDataStatus, ClauseBlockRecord, MailMergeTable, PrepareMailMergeFileResult, RenderMailMergeResult, TemplateMarkupCandidate, TemplateVersionRecord, TemplateMarkupReplacement, TemplateMarkupReport, ImportLearningExampleFileResult, TemplateLearningReport, TemplateLearningMapField, TemplateLearningMapReport, TemplateRegressionReport, BackgroundWatcherPlan, ImportTemplateFileResult, PrintFilesResponse, PrintJobDto, PrintPreferences, PrinterInventory, PrintTriageReport, TemplateApprovalRecord, ExportPdfResponse, CreateKedoPackageResponse, DiaryEntryPlanDto, DocumentPack, DocumentTemplateSpec, DomainKind, FirstRunStateResponse, ProcessBlueprintState, FolderNamePartDto, Icd10Suggestion, IntakeRouteResponse, IntakeCapability, SidecarToolStatus, ComponentStatus, ParseWebSourceResponse, OutputPlanDto, ParseSourceResponse, ParseSourceFileResponse, DocumentTemplateTextResponse, PopupAnswerDto, PopupApplyResult, PopupFieldConfig, ProductAccessResponse, RenderDocxBatchResult, RenderResult, ScannerApplyReportDto, ScannerMarkDto, SemanticCase, TemplateCandidateDto, TemplateConfirmationRowDto, WorkflowPlan, CreatedDocumentsIntakeResult, SemanticExtractResult, SeriesEntryPlanDto, SeriesPlanRequestDto, GuidedScannerMarkupAction, GuidedScannerMode, LearnedScannerRule, PromptInputKind, UpdateCheckResponse, WordScannerApplyResult, WordScannerCapture, WordScannerSession } from './types';

export type InvokeFn = <T>(command: string, payload?: Record<string, unknown>) => Promise<T>;
let invokeFn: InvokeFn = (command, payload) => tauriInvoke(command, payload);

/**
 * Thin UI API layer.
 *
 * All business decisions must live in Rust (dokkomplekt-core) and be reached only through
 * Tauri commands. This file deliberately contains no parser, workflow, template, licensing,
 * ICD, diary, or rendering logic. Browser/dev fallback is explicit and non-authoritative.
 */
export function __setInvokeForTests(fn: InvokeFn): void {
  invokeFn = fn;
}

export function __resetInvokeForTests(): void {
  invokeFn = (command, payload) => tauriInvoke(command, payload);
}

async function callRust<T>(command: string, payload?: Record<string, unknown>): Promise<T> {
  const value = await invokeFn<unknown>(command, payload);
  return validateRustResponse<T>(command, value);
}

export type AnalyzeTemplateResponse = {
  document: DocumentTemplateSpec;
  analysis_json: unknown;
  core_pipeline_json?: unknown;
  extracted_text: string;
};


export async function firstRunState(): Promise<FirstRunStateResponse> {
  return callRust('first_run_state');
}

export async function getDefaultOutputRoot(): Promise<string> {
  return callRust('get_default_output_root');
}

export async function ensureOutputRoot(outputRoot: string): Promise<string> {
  return callRust('ensure_output_root', { req: { output_root: outputRoot } });
}

export async function getProcessBlueprints(): Promise<ProcessBlueprintState> {
  return callRust('get_process_blueprints');
}

export async function selectProcessBlueprint(processId: string): Promise<ProcessBlueprintState> {
  return callRust('select_process_blueprint', { req: { process_id: processId } });
}

export async function analyzeTemplate(templateText: string, documentId: string, templatePath: string, buttonLabel?: string): Promise<AnalyzeTemplateResponse> {
  return callRust('analyze_template', {
    req: { template_text: templateText, document_id: documentId, template_path: templatePath, button_label: buttonLabel ?? null }
  });
}

export async function analyzeTemplateFile(templatePath: string, documentId: string, buttonLabel?: string): Promise<AnalyzeTemplateResponse> {
  return callRust('analyze_template_file', {
    req: { template_path: templatePath, document_id: documentId, button_label: buttonLabel ?? null }
  });
}



export async function prepareTemplateSetup(candidates: TemplateCandidateDto[]): Promise<TemplateConfirmationRowDto[]> {
  return callRust('prepare_template_setup', { req: { candidates } });
}

export async function importLearningExampleFile(fileName: string, bytesBase64: string): Promise<ImportLearningExampleFileResult> {
  return callRust('import_learning_example_file', { req: { file_name: fileName, bytes_base64: bytesBase64 } });
}

export async function learnTemplateFromExamples(input: {
  blankTemplatePath: string;
  completedExamplePaths: string[];
  sourceExamplePaths?: string[];
  defaultYear: number;
  locale?: string;
}): Promise<TemplateLearningReport> {
  return callRust('learn_template_from_examples_command', {
    req: {
      blank_template_path: input.blankTemplatePath,
      completed_example_paths: input.completedExamplePaths,
      source_example_paths: input.sourceExamplePaths ?? [],
      default_year: input.defaultYear,
      locale: input.locale ?? 'ru-RU',
    },
  });
}

export async function applyTemplateLearningMap(inputPath: string, outputPath: string, confirmedFields: TemplateLearningMapField[]): Promise<TemplateLearningMapReport> {
  return callRust('apply_template_learning_map', {
    req: { input_path: inputPath, output_path: outputPath, confirmed_fields: confirmedFields },
  });
}

export async function registerLearnedTemplate(documentId: string, buttonLabel: string, templatePath: string): Promise<DocumentPack> {
  return callRust('register_learned_template', { req: { document_id: documentId, button_label: buttonLabel, template_path: templatePath } });
}

export async function confirmTemplateSetup(rows: TemplateConfirmationRowDto[], autoInferStaticTemplates = false): Promise<DocumentPack> {
  return callRust('confirm_template_setup', { req: { rows, auto_infer_static_templates: autoInferStaticTemplates } });
}

export async function renameDocumentButton(documentId: string, buttonLabel: string): Promise<DocumentPack> {
  return callRust('rename_document_button', { req: { document_id: documentId, button_label: buttonLabel } });
}

export async function removeDocumentButton(documentId: string): Promise<DocumentPack> {
  return callRust('remove_document_button', { req: { document_id: documentId } });
}

export async function updateDocumentPopupFields(documentId: string, popupFields: PopupFieldConfig[]): Promise<DocumentPack> {
  return callRust('update_document_popup_fields', { req: { document_id: documentId, popup_fields: popupFields } });
}

export async function resetCase(): Promise<SemanticCase> {
  return callRust('reset_case');
}

export async function parseSource(sourceText: string, defaultYear: number): Promise<ParseSourceResponse> {
  return callRust('parse_source', { req: { source_text: sourceText, default_year: defaultYear } });
}

export interface PickedSourceFile {
  file_name: string;
  selected_path: string;
}

export async function pickSourceFile(initialPath?: string | null): Promise<PickedSourceFile | null> {
  return callRust('pick_source_file', { req: { initial_path: initialPath ?? null } });
}

export async function parseSourcePath(selectedPath: string, defaultYear: number): Promise<ParseSourceFileResponse> {
  return callRust('parse_source_path', { req: { selected_path: selectedPath, default_year: defaultYear } });
}

export async function parseSourceFile(fileName: string, bytesBase64: string, defaultYear: number): Promise<ParseSourceFileResponse> {
  return callRust('parse_source_file', { req: { file_name: fileName, bytes_base64: bytesBase64, default_year: defaultYear } });
}

export async function getIntakeCapabilities(): Promise<IntakeCapability[]> {
  return callRust('get_intake_capabilities');
}

export async function getSidecarStatus(): Promise<SidecarToolStatus[]> {
  return callRust('get_sidecar_status');
}

export async function getComponentStatuses(): Promise<ComponentStatus[]> {
  return callRust('get_component_statuses');
}

export async function refreshComponentCatalog(): Promise<ComponentStatus[]> {
  return callRust('refresh_component_catalog');
}

export async function installComponent(id: string): Promise<ComponentStatus> {
  return callRust('install_component', { id });
}

export async function removeComponent(id: string): Promise<ComponentStatus> {
  return callRust('remove_component', { id });
}

export async function parseWebSource(url: string, defaultYear: number): Promise<ParseWebSourceResponse> {
  return callRust('parse_web_source', { req: { url, default_year: defaultYear } });
}

export async function getDocumentTemplateText(documentId: string): Promise<DocumentTemplateTextResponse> {
  return callRust('get_document_template_text', { req: { document_id: documentId } });
}

export async function renderDocx(documentId: string, outputPath: string, strict = true): Promise<RenderResult> {
  return callRust('render_docx', { req: { document_id: documentId, output_path: outputPath, strict } });
}

export type ExistingOutputPolicy = 'version' | 'replace_with_backup';

export async function renderDocxBatch(
  documentIds: string[],
  outputRoot: string,
  folderParts: FolderNamePartDto[],
  strict = true,
  existingOutputPolicy: ExistingOutputPolicy = 'version',
  sickLeaveEnabled = false,
): Promise<RenderDocxBatchResult> {
  return callRust('render_docx_batch', {
    req: {
      document_ids: documentIds,
      output_root: outputRoot,
      folder_parts: folderParts,
      strict,
      sick_leave_enabled: sickLeaveEnabled,
      existing_output_policy: existingOutputPolicy,
    },
  });
}

export async function setField(fieldId: string, value: string): Promise<SemanticCase> {
  return callRust('set_field', { req: { field_id: fieldId, value } });
}

export async function getWorkflowPlan(
  documentId: string,
  sickLeaveEnabled: boolean,
  folderParts: FolderNamePartDto[] = [],
): Promise<WorkflowPlan> {
  const req: { document_id: string; sick_leave_enabled: boolean; folder_parts?: FolderNamePartDto[] } = {
    document_id: documentId,
    sick_leave_enabled: sickLeaveEnabled,
  };
  if (folderParts.length) req.folder_parts = folderParts;
  return callRust('get_workflow_plan', { req });
}

export async function getWorkflowPlanBatch(
  documentIds: string[],
  sickLeaveEnabled: boolean,
  folderParts: FolderNamePartDto[] = [],
): Promise<WorkflowPlan> {
  const req: { document_ids: string[]; sick_leave_enabled: boolean; folder_parts?: FolderNamePartDto[] } = {
    document_ids: documentIds,
    sick_leave_enabled: sickLeaveEnabled,
  };
  if (folderParts.length) req.folder_parts = folderParts;
  return callRust('get_workflow_plan_batch', { req });
}

export async function applyPopup(
  documentId: string,
  sickLeaveEnabled: boolean,
  answers: PopupAnswerDto[],
  folderParts: FolderNamePartDto[] = [],
): Promise<PopupApplyResult> {
  const req: {
    document_id: string;
    sick_leave_enabled: boolean;
    answers: PopupAnswerDto[];
    folder_parts?: FolderNamePartDto[];
  } = { document_id: documentId, sick_leave_enabled: sickLeaveEnabled, answers };
  if (folderParts.length) req.folder_parts = folderParts;
  return callRust('apply_popup', { req });
}

export async function applyPopupBatch(
  documentIds: string[],
  sickLeaveEnabled: boolean,
  answers: PopupAnswerDto[],
  folderParts: FolderNamePartDto[] = [],
): Promise<PopupApplyResult> {
  const req: {
    document_ids: string[];
    sick_leave_enabled: boolean;
    answers: PopupAnswerDto[];
    folder_parts?: FolderNamePartDto[];
  } = { document_ids: documentIds, sick_leave_enabled: sickLeaveEnabled, answers };
  if (folderParts.length) req.folder_parts = folderParts;
  return callRust('apply_popup_batch', { req });
}

export async function renderPreview(templateText: string, strict = true): Promise<RenderResult> {
  return callRust('render_preview', { req: { template_text: templateText, strict } });
}



export async function applyScanner(marks: ScannerMarkDto[]): Promise<ScannerApplyReportDto> {
  return callRust('apply_scanner', { req: { marks } });
}


export async function startWordScanner(path: string, mode: GuidedScannerMode, makeWorkingCopy: boolean): Promise<WordScannerSession> {
  return callRust('start_word_scanner', { req: { path, mode, make_working_copy: makeWorkingCopy } });
}

export async function activateWordScanner(sessionId: string): Promise<boolean> {
  return callRust('activate_word_scanner', { req: { session_id: sessionId } });
}

export async function captureWordScanner(sessionId: string, closeAfterCapture: boolean): Promise<WordScannerCapture> {
  return callRust('capture_word_scanner', { req: { session_id: sessionId, close_after_capture: closeAfterCapture } });
}

export async function applyWordScannerSelection(
  sessionId: string,
  fieldId: string,
  action: GuidedScannerMarkupAction,
): Promise<WordScannerApplyResult> {
  return callRust('apply_word_scanner_selection', { req: { session_id: sessionId, field_id: fieldId, action } });
}

export async function closeWordScanner(sessionId: string, discardWorkingCopy = false): Promise<boolean> {
  return callRust('close_word_scanner', { req: { session_id: sessionId, discard_working_copy: discardWorkingCopy } });
}

export async function saveLearnedScannerRule(input: {
  fieldId: string;
  title: string;
  selectedText: string;
  contextText: string;
  beforeText: string;
  afterText: string;
  inputKind: PromptInputKind;
  sourceText?: string;
}): Promise<LearnedScannerRule[]> {
  return callRust('save_learned_scanner_rule', {
    req: {
      field_id: input.fieldId,
      title: input.title,
      selected_text: input.selectedText,
      context_text: input.contextText,
      before_text: input.beforeText,
      after_text: input.afterText,
      input_kind: input.inputKind,
      source_text: input.sourceText ?? null,
    },
  });
}

export async function listLearnedScannerRules(): Promise<LearnedScannerRule[]> {
  return callRust('list_learned_scanner_rules');
}

export async function deleteLearnedScannerRule(ruleId: string): Promise<LearnedScannerRule[]> {
  return callRust('delete_learned_scanner_rule', { req: { rule_id: ruleId } });
}

export async function checkTemplateRegression(documentId: string, candidateTemplatePath: string): Promise<TemplateRegressionReport | null> {
  return callRust('check_template_regression', { req: { document_id: documentId, candidate_template_path: candidateTemplatePath } });
}

export async function updateDocumentTemplate(documentId: string, templatePath: string, acknowledgeRegressions = false): Promise<DocumentPack> {
  return callRust('update_document_template', { req: { document_id: documentId, template_path: templatePath, acknowledge_regressions: acknowledgeRegressions } });
}

export async function listTemplateVersions(documentId: string): Promise<TemplateVersionRecord[]> {
  return callRust('list_template_versions', { req: { document_id: documentId } });
}

export async function rollbackTemplateVersion(versionId: string): Promise<DocumentPack> {
  return callRust('rollback_template_version', { req: { version_id: versionId } });
}

export async function getDiaryPlan(admissionDate: string | null, dischargeDate: string | null, defaultYear: number): Promise<DiaryEntryPlanDto[]> {
  return callRust('get_diary_plan', { req: { admission_date: admissionDate, discharge_date: dischargeDate, default_year: defaultYear } });
}

export async function getRecordSeriesPlan(req: SeriesPlanRequestDto): Promise<SeriesEntryPlanDto[]> {
  return callRust('get_record_series_plan', { req });
}

export async function getOutputPlan(rootFolder: string, folderParts: FolderNamePartDto[], buttonLabels: string[]): Promise<OutputPlanDto> {
  return callRust('get_output_plan', { req: { root_folder: rootFolder, folder_parts: folderParts, button_labels: buttonLabels } });
}

export async function routeIntake(appAlreadyRunning: boolean, userRequestedUi: boolean): Promise<IntakeRouteResponse> {
  return callRust('route_intake', { req: { app_already_running: appAlreadyRunning, user_requested_ui: userRequestedUi } });
}

export async function runCreatedDocumentsIntake(
  sourcePath: string,
  outputRoot: string,
  folderParts: FolderNamePartDto[],
  defaultYear: number,
  sickLeaveEnabled: boolean,
): Promise<CreatedDocumentsIntakeResult> {
  return callRust('run_created_documents_intake', {
    req: {
      source_path: sourcePath,
      output_root: outputRoot,
      folder_parts: folderParts,
      default_year: defaultYear,
      sick_leave_enabled: sickLeaveEnabled,
    },
  });
}

export async function semanticExtract(
  sourceText: string,
  defaultYear: number,
  modelOutput?: string,
): Promise<SemanticExtractResult> {
  return callRust('semantic_extract', {
    req: { source_text: sourceText, default_year: defaultYear, model_output: modelOutput ?? null },
  });
}

export async function saveState(dbPath: string): Promise<void> {
  return callRust('save_state', { req: { db_path: dbPath } });
}

export async function loadState(dbPath: string): Promise<FirstRunStateResponse> {
  return callRust('load_state', { req: { db_path: dbPath } });
}

export async function icd10Suggest(query: string): Promise<Icd10Suggestion[]> {
  return callRust('icd10_suggest', { query });
}

export async function validateProductAccess(code?: string | null): Promise<ProductAccessResponse> {
  return callRust('validate_product_access', { req: { code: code ?? null } });
}

/**
 * License verification against the trust anchor compiled into the Rust binary.
 * The UI never supplies the public key: allowing that would let anyone "verify"
 * a self-signed license.
 */
export async function verifyRustLicenseText(licenseText: string): Promise<boolean> {
  return callRust('verify_rust_license_text', { req: { license_text: licenseText } });
}

export async function checkForUpdates(): Promise<UpdateCheckResponse> {
  return callRust('check_for_updates');
}

export async function installBackgroundWatcher(
  watchFolder: string,
  defaultYear?: number,
  sickLeaveEnabled = false,
  folderParts: FolderNamePartDto[] = [],
  autoPrint = false,
  printCopiesByDocument: Record<string, number> = {},
): Promise<BackgroundWatcherPlan> {
  return callRust('install_background_watcher', {
    req: {
      watch_folder: watchFolder,
      default_year: defaultYear ?? null,
      sick_leave_enabled: sickLeaveEnabled,
      folder_parts: folderParts,
      auto_print: autoPrint,
      print_copies_by_document: printCopiesByDocument,
    },
  });
}

export async function updateBackgroundWatcherPreferences(
  autoPrint: boolean,
  printCopiesByDocument: Record<string, number>,
): Promise<boolean> {
  return callRust('update_background_watcher_preferences', {
    req: {
      auto_print: autoPrint,
      print_copies_by_document: printCopiesByDocument,
    },
  });
}

export async function uninstallBackgroundWatcher(): Promise<BackgroundWatcherPlan> {
  return callRust('uninstall_background_watcher');
}

export async function getPrintTriage(documentIds: string[], outputFolder?: string | null): Promise<PrintTriageReport> {
  return callRust('get_print_triage', {
    req: { document_ids: documentIds, output_folder: outputFolder ?? null },
  });
}

export async function listTemplateApprovals(): Promise<TemplateApprovalRecord[]> {
  return callRust('list_template_approvals');
}

export async function approveDocumentTemplate(input: {
  documentId: string;
  jurisdiction: string;
  approvedBy: string;
  note?: string;
  acknowledgement: boolean;
}): Promise<TemplateApprovalRecord> {
  return callRust('approve_document_template', {
    req: {
      document_id: input.documentId,
      jurisdiction: input.jurisdiction,
      approved_by: input.approvedBy,
      note: input.note ?? '',
      acknowledgement: input.acknowledgement,
    },
  });
}

export async function revokeDocumentTemplateApproval(documentId: string): Promise<TemplateApprovalRecord[]> {
  return callRust('revoke_document_template_approval', { documentId });
}

export async function printFiles(jobs: PrintJobDto[]): Promise<PrintFilesResponse> {
  return callRust('print_files', { req: { jobs } });
}

export async function getPrinterInventory(): Promise<PrinterInventory> {
  return callRust('get_printer_inventory');
}

export async function updatePrintPreferences(preferences: PrintPreferences): Promise<PrinterInventory> {
  return callRust('update_print_preferences', { req: { preferences } });
}

export async function exportFilesToPdf(paths: string[], pdfa1 = false, outputDir?: string | null): Promise<ExportPdfResponse> {
  return callRust('export_files_to_pdf', { req: { paths, pdfa_1: pdfa1, output_dir: outputDir ?? null } });
}

export async function createKedoPackage(paths: string[], outputRoot: string, title = 'КЭДО-пакет'): Promise<CreateKedoPackageResponse> {
  return callRust('create_kedo_package', { req: { paths, output_root: outputRoot, title } });
}

export async function pickFolder(initialPath?: string | null): Promise<string | null> {
  const response = await callRust<{ selected_path: string | null }>('pick_folder', { req: { initial_path: initialPath ?? null } });
  return response.selected_path;
}

export interface PickedTemplateFile {
  file_name: string;
  template_path: string;
  extracted_text: string;
  import_error?: string | null;
}

export async function pickTemplateFiles(initialPath?: string | null): Promise<PickedTemplateFile[]> {
  const response = await callRust<{ files: PickedTemplateFile[] }>('pick_template_files', {
    req: { initial_path: initialPath ?? null },
  });
  return response.files;
}

export async function openInFileManager(path: string): Promise<void> {
  return callRust('open_in_file_manager', { req: { path } });
}

export async function importTemplateFile(
  documentId: string,
  source: { fileName?: string; bytesBase64?: string; templateText?: string },
): Promise<ImportTemplateFileResult> {
  return callRust('import_template_file', {
    req: {
      document_id: documentId,
      file_name: source.fileName ?? null,
      bytes_base64: source.bytesBase64 ?? null,
      template_text: source.templateText ?? null,
    },
  });
}


export async function getPrivacyPreferences(): Promise<PrivacyPreferences> {
  return callRust('get_privacy_preferences');
}

export async function updatePrivacyPreferences(preferences: PrivacyPreferences): Promise<PrivacyPreferences> {
  return callRust('update_privacy_preferences', { req: { preferences } });
}

export async function runWorkspaceHygiene(): Promise<WorkspaceHygieneReport> {
  return callRust('run_workspace_hygiene');
}

export async function listCaseRuns(limit = 100): Promise<CaseRunRecord[]> {
  return callRust('list_case_runs', { req: { limit } });
}

export async function retryCaseRun(caseId: string): Promise<CreatedDocumentsIntakeResult> {
  return callRust('retry_case_run', { req: { case_id: caseId } });
}

export async function listAutomationExceptions(includeResolved = false): Promise<AutomationExceptionRecord[]> {
  return callRust('list_automation_exceptions', { req: { include_resolved: includeResolved } });
}

export async function resolveAutomationException(exceptionId: string, resolution: string): Promise<boolean> {
  return callRust('resolve_automation_exception', { req: { exception_id: exceptionId, resolution } });
}

export async function confirmRiskExceptionAndRetry(exceptionId: string): Promise<CreatedDocumentsIntakeResult> {
  return callRust('confirm_risk_exception_and_retry', { req: { exception_id: exceptionId } });
}

export async function confirmBundleExceptionAndRetry(exceptionId: string, documentIds: string[]): Promise<CreatedDocumentsIntakeResult> {
  return callRust('confirm_bundle_exception_and_retry', { req: { exception_id: exceptionId, document_ids: documentIds } });
}

export async function getAutomationMetrics(): Promise<AutomationMetrics> {
  return callRust('get_automation_metrics');
}

export async function getDailyAutomationDashboard(): Promise<DailyAutomationDashboard> {
  return callRust('get_daily_automation_dashboard');
}

export async function getQualityTelemetry(): Promise<QualityTelemetryReport> {
  return callRust('get_quality_telemetry');
}

export async function getQueueStatus(): Promise<QueueStatus> {
  return callRust('get_queue_status');
}

export async function getCorpusStatus(): Promise<CorpusStatus> {
  return callRust('get_corpus_status');
}

export interface KitLearningDecision {
  document_ids: string[];
  source: string;
  confidence: number;
  auto_apply: boolean;
  reason: string;
}

export async function getLearnedKitDecision(domain: DomainKind, clusterId: string, packId?: string): Promise<KitLearningDecision | null> {
  return callRust('get_learned_kit_decision', { req: { domain, cluster_id: clusterId, pack_id: packId ?? null } });
}

export async function exportCorpus(outputPath: string, limit = 10_000): Promise<CorpusExportResponse> {
  return callRust('export_corpus', { req: { output_path: outputPath, limit } });
}

export async function getCalibratedThresholdStatus(): Promise<CalibratedThresholdStatus[]> {
  return callRust('get_calibrated_threshold_status');
}

export async function importCalibratedThresholdsFile(fileName: string, bytesBase64: string): Promise<CalibratedThresholdStatus> {
  return callRust('import_calibrated_thresholds', { req: { path: null, file_name: fileName, bytes_base64: bytesBase64 } });
}

export async function listAuditEvents(limit = 100): Promise<AuditEventRecord[]> {
  return callRust('list_audit_events', { req: { limit } });
}

export async function getSemanticModelConfig(): Promise<SemanticModelConfigurationResponse> {
  return callRust('get_semantic_model_config');
}

export async function updateSemanticModelConfig(config: LocalSemanticModelConfig): Promise<SemanticModelConfigurationResponse> {
  return callRust('update_semantic_model_config', { req: { config } });
}

export async function testSemanticModel(): Promise<LocalSemanticModelStatus> {
  return callRust('test_semantic_model');
}

export async function getReferenceDataStatus(): Promise<ReferenceDataStatus> {
  return callRust('get_reference_data_status');
}

export async function updateReferenceData(): Promise<ReferenceDataStatus> {
  return callRust('update_reference_data');
}

export async function importReferenceData(path: string): Promise<ReferenceDataStatus> {
  return callRust('import_reference_data', { req: { path, file_name: null, bytes_base64: null } });
}

export async function importReferenceDataFile(fileName: string, bytesBase64: string): Promise<ReferenceDataStatus> {
  return callRust('import_reference_data', { req: { path: null, file_name: fileName, bytes_base64: bytesBase64 } });
}

export async function listClauseBlocks(): Promise<ClauseBlockRecord[]> { return callRust('list_clause_blocks'); }
export async function saveClauseBlock(blockId: string, title: string, content: string): Promise<ClauseBlockRecord[]> { return callRust('save_clause_block', { req: { block_id: blockId, title, content } }); }
export async function deleteClauseBlock(blockId: string): Promise<ClauseBlockRecord[]> { return callRust('delete_clause_block', { req: { block_id: blockId } }); }
export async function suggestTemplateMarkup(fileName: string, bytesBase64: string, defaultYear: number): Promise<TemplateMarkupCandidate[]> { return callRust('suggest_template_markup_command', { req: { file_name: fileName, bytes_base64: bytesBase64, default_year: defaultYear } }); }
export async function applyTemplateMarkup(inputPath: string, outputPath: string, replacements: TemplateMarkupReplacement[]): Promise<TemplateMarkupReport> { return callRust('apply_template_markup_command', { req: { input_path: inputPath, output_path: outputPath, replacements } }); }
export async function previewMailMerge(delimitedText: string): Promise<MailMergeTable> { return callRust('preview_mail_merge', { req: { delimited_text: delimitedText } }); }
export async function prepareMailMergeFile(fileName: string, bytesBase64: string): Promise<PrepareMailMergeFileResult> { return callRust('prepare_mail_merge_file', { req: { file_name: fileName, bytes_base64: bytesBase64 } }); }
export async function renderMailMerge(documentIds: string[], delimitedText: string, outputRoot: string, strict = true): Promise<RenderMailMergeResult> { return callRust('render_mail_merge', { req: { document_ids: documentIds, delimited_text: delimitedText, output_root: outputRoot, strict } }); }

export const rustCommandNames = [
  'first_run_state',
  'get_default_output_root',
  'ensure_output_root',
  'analyze_template',
  'analyze_template_file',
  'prepare_template_setup',
  'import_learning_example_file',
  'learn_template_from_examples_command',
  'apply_template_learning_map',
  'register_learned_template',
  'confirm_template_setup',
  'rename_document_button',
  'remove_document_button',
  'update_document_popup_fields',
  'reset_case',
  'set_field',
  'parse_source',
  'pick_source_file',
  'parse_source_path',
  'parse_source_file',
  'get_intake_capabilities',
  'get_sidecar_status',
  'get_component_statuses',
  'refresh_component_catalog',
  'install_component',
  'remove_component',
  'parse_web_source',
  'get_document_template_text',
  'get_workflow_plan',
  'get_workflow_plan_batch',
  'apply_popup',
  'apply_popup_batch',
  'render_preview',
  'render_docx',
  'render_docx_batch',
  'get_privacy_preferences',
  'update_privacy_preferences',
  'run_workspace_hygiene',
  'list_automation_exceptions',
  'resolve_automation_exception',
  'confirm_risk_exception_and_retry',
  'confirm_bundle_exception_and_retry',
  'get_automation_metrics',
  'get_daily_automation_dashboard',
  'get_queue_status',
  'get_corpus_status',
  'get_learned_kit_decision',
  'export_corpus',
  'get_calibrated_threshold_status',
  'import_calibrated_thresholds',
  'list_case_runs',
  'retry_case_run',
  'list_audit_events',
  'get_semantic_model_config',
  'update_semantic_model_config',
  'test_semantic_model',
  'get_reference_data_status',
  'update_reference_data',
  'import_reference_data',
  'list_clause_blocks',
  'save_clause_block',
  'delete_clause_block',
  'suggest_template_markup_command',
  'apply_template_markup_command',
  'preview_mail_merge',
  'prepare_mail_merge_file',
  'render_mail_merge',
  'apply_scanner',
  'start_word_scanner',
  'activate_word_scanner',
  'capture_word_scanner',
  'apply_word_scanner_selection',
  'close_word_scanner',
  'save_learned_scanner_rule',
  'list_learned_scanner_rules',
  'delete_learned_scanner_rule',
  'check_template_regression',
  'update_document_template',
  'list_template_versions',
  'rollback_template_version',
  'get_diary_plan',
  'get_record_series_plan',
  'icd10_suggest',
  'get_output_plan',
  'route_intake',
  'save_state',
  'load_state',
  'validate_product_access',
  'verify_rust_license_text',
  'check_for_updates',
  'install_background_watcher',
  'update_background_watcher_preferences',
  'uninstall_background_watcher',
  'run_created_documents_intake',
  'get_print_triage',
  'list_template_approvals',
  'approve_document_template',
  'revoke_document_template_approval',
  'print_files',
  'get_printer_inventory',
  'update_print_preferences',
  'export_files_to_pdf',
  'create_kedo_package',
  'pick_template_files',
  'pick_folder',
  'open_in_file_manager',
  'semantic_extract',
  'import_business_registry',
  'lookup_business_registry',
  'apply_business_registry_record',
  'export_one_c_counterparties',
  'list_organization_knowledge',
  'upsert_organization_knowledge',
  'delete_organization_knowledge',
  'apply_organization_knowledge',
  'get_quality_telemetry',
  'get_process_blueprints',
  'select_process_blueprint',
  'import_template_file'
] as const;


export async function importBusinessRegistry(records: BusinessRegistryRecord[], replace = false): Promise<BusinessRegistryImportResult> {
  return callRust('import_business_registry', { req: { records, replace } });
}

export async function lookupBusinessRegistry(inn: string): Promise<BusinessRegistryRecord | null> {
  return callRust('lookup_business_registry', { req: { inn } });
}

export async function applyBusinessRegistryRecord(inn: string, target: 'organization' | 'counterparty'): Promise<SemanticCase> {
  return callRust('apply_business_registry_record', { req: { inn, target } });
}

export async function exportOneCCounterparties(outputPath: string, inns: string[] = []): Promise<string> {
  return callRust('export_one_c_counterparties', { req: { output_path: outputPath, inns } });
}

export async function listOrganizationKnowledge(category?: OrganizationKnowledgeCategory, includeInactive = false): Promise<OrganizationKnowledgeRecord[]> {
  return callRust('list_organization_knowledge', { req: { category: category ?? null, include_inactive: includeInactive } });
}

export async function upsertOrganizationKnowledge(record: Omit<OrganizationKnowledgeRecord, 'updated_at'>): Promise<OrganizationKnowledgeRecord[]> {
  return callRust('upsert_organization_knowledge', {
    req: {
      record_id: record.record_id,
      category: record.category,
      label: record.label,
      fields: record.fields,
      valid_from: record.valid_from ?? null,
      valid_until: record.valid_until ?? null,
      active: record.active,
      note: record.note,
    },
  });
}

export async function deleteOrganizationKnowledge(recordId: string): Promise<OrganizationKnowledgeRecord[]> {
  return callRust('delete_organization_knowledge', { req: { record_id: recordId } });
}

export async function applyOrganizationKnowledge(recordId: string): Promise<SemanticCase> {
  return callRust('apply_organization_knowledge', { req: { record_id: recordId } });
}