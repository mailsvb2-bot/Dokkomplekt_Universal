import { afterEach, describe, expect, it } from 'vitest';
import {
  __resetInvokeForTests,
  __setInvokeForTests,
  activateWordScanner,
  analyzeTemplate,
  analyzeTemplateFile,
  applyPopup,
  applyPopupBatch,
  applyScanner,
  applyWordScannerSelection,
  captureWordScanner,
  closeWordScanner,
  confirmTemplateSetup,
  createKedoPackage,
  getPrintTriage,
  listTemplateApprovals,
  approveDocumentTemplate,
  revokeDocumentTemplateApproval,
  importBusinessRegistry,
  lookupBusinessRegistry,
  applyBusinessRegistryRecord,
  exportOneCCounterparties,
  firstRunState,
  deleteLearnedScannerRule,
  getDiaryPlan,
  getIntakeCapabilities,
  getSidecarStatus,
  getComponentStatuses,
  refreshComponentCatalog,
  installComponent,
  removeComponent,
  getPrivacyPreferences,
  updatePrivacyPreferences,
  runWorkspaceHygiene,
  listAutomationExceptions,
  resolveAutomationException,
  confirmRiskExceptionAndRetry,
  getAutomationMetrics,
  getQueueStatus,
  getCorpusStatus,
  getLearnedKitDecision,
  exportCorpus,
  getCalibratedThresholdStatus,
  importCalibratedThresholdsFile,
  listAuditEvents,
  getRecordSeriesPlan,
  getOutputPlan,
  getWorkflowPlan,
  getWorkflowPlanBatch,
  icd10Suggest,
  importTemplateFile,
  installBackgroundWatcher,
  loadState,
  openInFileManager,
  pickFolder,
  printFiles,
  parseSource,
  parseSourceFile,
  parseWebSource,
  getDocumentTemplateText,
  prepareTemplateSetup,
  renameDocumentButton,
  removeDocumentButton,
  resetCase,
  updateDocumentPopupFields,
  updateDocumentTemplate,
  renderDocx,
  renderDocxBatch,
  listClauseBlocks,
  listLearnedScannerRules,
  listTemplateVersions,
  rollbackTemplateVersion,
  saveClauseBlock,
  deleteClauseBlock,
  suggestTemplateMarkup,
  applyTemplateMarkup,
  previewMailMerge,
  renderMailMerge,
  renderPreview,
  routeIntake,
  runCreatedDocumentsIntake,
  semanticExtract,
  rustCommandNames,
  saveLearnedScannerRule,
  saveState,
  startWordScanner,
  setField,
  uninstallBackgroundWatcher,
  updateBackgroundWatcherPreferences,
  validateProductAccess,
  verifyRustLicenseText,
} from './api';

type Call = { command: string; payload?: Record<string, unknown> };

export const registeredBackendCommands = [
  'first_run_state',
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
  'pick_folder',
  'open_in_file_manager',
  'get_semantic_model_config',
  'update_semantic_model_config',
  'test_semantic_model',
  'get_reference_data_status',
  'update_reference_data',
  'import_reference_data',
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
  'import_template_file',
] as const;

const document = {
  id: 'doc_1',
  button_label: 'Документ',
  template_path: 'template.docx',
  category: 'Generic',
  role_id: 'generic',
  required_fields: [],
  placeholders: [],
  is_static_copy: false,
};

const confirmationRow = {
  document_id: 'doc_1',
  template_path: 'template.docx',
  detected_title: 'Документ',
  suggested_button_label: 'Документ',
  editable_button_label: 'Документ',
  role_id: 'generic',
  is_static_copy: false,
  analysis: { title: 'Документ' },
  popup_fields: [],
};

const pack = { pack_id: 'default', name: 'Пакет', documents: [document] };
const caseDto = { values: {} };
const workflowDto = { document_id: 'doc_1', prompts: [], blocked: false, block_reasons: [] };
const renderDto = { output_text: 'ok', missing_fields: [], unknown_fields: [], warnings: [] };

function installContractMock(calls: Call[]) {
  __setInvokeForTests(async (command, payload) => {
    calls.push({ command, payload });
    switch (command) {
      case 'first_run_state':
      case 'load_state':
        return { pack, has_user_buttons: true, message: 'ok' } as never;
      case 'analyze_template':
      case 'analyze_template_file':
        return { document, analysis_json: { from: 'rust' }, core_pipeline_json: { from: 'rust-core' } } as never;
      case 'prepare_template_setup':
        return [confirmationRow] as never;
      case 'confirm_template_setup':
      case 'rename_document_button':
      case 'remove_document_button':
      case 'update_document_popup_fields':
      case 'update_document_template':
      case 'rollback_template_version':
        return pack as never;
      case 'list_template_versions':
        return [{ version_id: 'tpl-v1', document_id: 'doc_1', version_number: 1, template_path: 'template.docx', template_sha256: 'a'.repeat(64), note: 'published', status: 'published', created_at: 'now' }] as never;
      case 'parse_source':
        return { semantic_case: caseDto, report: { recognized_title: 'Первичный документ', warnings: [] } } as never;
      case 'parse_source_file':
        return { source_text: 'Первичный документ', source_path: '/app-data/intake-work/source.pdf', semantic_case: caseDto, report: { recognized_title: 'Первичный документ', warnings: [] } } as never;
      case 'get_intake_capabilities':
        return [{ format: 'Word', extensions: ['docx', 'docm'], ready: true, mode: 'встроенно', detail: 'ok' }] as never;
      case 'get_sidecar_status':
        return [{ tool: 'tesseract', available: true, bundled: false, state: 'downloaded', component_id: 'ocr', resolved_path: 'components/ocr/tesseract.exe', purpose: 'OCR' }] as never;
      case 'get_component_statuses':
      case 'refresh_component_catalog':
        return [{ id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42, size_label: '42 МБ', unlocks: ['tesseract'], state: 'downloaded', installed: true, available: true, catalog_available: true, message: 'ok' }] as never;
      case 'install_component':
      case 'remove_component':
        return { id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42, size_label: '42 МБ', unlocks: ['tesseract'], state: command === 'install_component' ? 'downloaded' : 'missing', installed: command === 'install_component', available: command === 'install_component', catalog_available: true, message: 'ok' } as never;
      case 'pick_folder':
        return { selected_path: 'C:/Desktop/output' } as never;
      case 'parse_web_source':
        return { source_text: 'HTTPS документ', final_url: 'https://example.com/doc', content_type: 'text/html', semantic_case: caseDto, report: { recognized_title: 'Документ', warnings: [] } } as never;
      case 'get_document_template_text':
        return { template_text: 'Документ {{field}}' } as never;
      case 'reset_case':
      case 'set_field':
        return caseDto as never;
      case 'get_workflow_plan':
      case 'get_workflow_plan_batch':
        return workflowDto as never;
      case 'apply_popup':
      case 'apply_popup_batch':
        return { accepted: true, semantic_case: caseDto, still_missing: [], message: 'ok' } as never;
      case 'render_docx':
        return { ...renderDto, output_path: 'out.docx' } as never;
      case 'render_docx_batch':
        return { output_folder: 'C:/Desktop/148_2026-06-01', created_files: ['C:/Desktop/148_2026-06-01/Документ.docx'] } as never;
      case 'get_privacy_preferences': return { copy_source_to_output: true, write_trust_report: true, include_values_in_trust_report: false, temp_retention_hours: 24, archive_processed_sources: true, archive_folder_name: '_обработано', service_note_retention_days: 30, processed_marker_retention_days: 7, archived_source_retention_days: 0 } as never;
      case 'update_privacy_preferences': return (payload as { req?: { preferences?: unknown } })?.req?.preferences as never;
      case 'list_automation_exceptions': return [{ exception_id: 'ex-1', category: 'quality_gate', source_path: 'source.pdf', message: 'Проверить поле', details_json: '{}', status: 'open', created_at: 'now', updated_at: 'now' }] as never;
      case 'resolve_automation_exception': return true as never;
      case 'confirm_risk_exception_and_retry': return { status: 'processed', patient_folder: 'output', created_files: [], created_documents: [], missing: [], attention_file: null, message: 'confirmed' } as never;
      case 'run_workspace_hygiene': return { archived_processed_sources: [], archived_service_files: [], removed_orphan_markers: [], removed_expired_archived_files: [], warnings: [] } as never;
      case 'list_case_runs': return [] as never;
      case 'retry_case_run': return { status: 'processed', patient_folder: 'output', created_files: [], created_documents: [], missing: [], attention_file: null, message: 'ok' } as never;
      case 'get_automation_metrics': return { processed_sources: 1, generated_documents: 2, blocked_sources: 1, failed_sources: 0, print_failures: 0, user_confirmations: 1 } as never;
      case 'get_queue_status': return { mode: 'shared_filesystem', configured: false, reachable: true, message: 'ok' } as never;
      case 'get_corpus_status': return { recording_enabled: false, entry_count: 2, privacy_mode: 'encrypted-hashed-no-raw-values', message: 'ok' } as never;
      case 'get_learned_kit_decision': return { document_ids: ['contract'], source: 'learned_corpus_rule', confidence: 0.99, auto_apply: true, reason: 'ok' } as never;
      case 'export_corpus': return { output_path: 'corpus.json', entry_count: 2, schema: 'dokkomplekt.ground-truth-corpus.v1' } as never;
      case 'get_calibrated_threshold_status': return [] as never;
      case 'import_calibrated_thresholds': return { installed: true, domain: 'hr', generated_at: 'now', imported_at: 'now', corpus_sha256: 'a'.repeat(64), auto_min_confidence: 0.995, review_min_confidence: 0.9, max_auto_error_rate: 0.005, training_observations: 80, holdout_observations: 10, message: 'ok' } as never;
      case 'list_audit_events': return [{ event_id: 'a-1', event_type: 'processed', object_hash: 'obj', detail_json: '{}', previous_hash: '', event_hash: 'hash', created_at: 'now' }] as never;
      case 'list_clause_blocks': return [] as never;
      case 'save_clause_block': return [{ block_id: 'requisites', title: 'Реквизиты', content: '{{org.name}}', updated_at: 'now' }] as never;
      case 'delete_clause_block': return [] as never;
      case 'suggest_template_markup_command': return [{ field_id: 'org.inn', title: 'ИНН', value: '7736050003', confidence: 0.99, occurrences: 1, selected_by_default: true }] as never;
      case 'apply_template_markup_command': return { output_path: 'marked.docx', replacement_count: 1, replaced_occurrences: 1, skipped_values: [] } as never;
      case 'preview_mail_merge': return { delimiter: ';', headers: ['ФИО'], canonical_headers: ['subject.name'], rows: [['Иванов']], warnings: [] } as never;
      case 'render_mail_merge': return { output_folder: 'output/mm', row_count: 1, created_files: ['output/mm/doc.docx'] } as never;
      case 'render_preview':
        return renderDto as never;
      case 'apply_scanner':
        return { applied_fields: ['scanner.note'], rejected_fields: [] } as never;
      case 'start_word_scanner':
        return { session_id: 'scan-1', mode: 'source', original_path: 'source.docx', opened_path: 'source.docx', working_copy: false, word_was_running: false, automation_available: true, message: 'opened' } as never;
      case 'activate_word_scanner': return true as never;
      case 'capture_word_scanner':
        return { session_id: 'scan-1', selected_text: '148', context_text: 'Номер: 148', before_text: 'Номер: ', after_text: '', selection_start: 7, selection_end: 10, expanded_from_cursor: false, document_path: 'source.docx', document_closed: true } as never;
      case 'apply_word_scanner_selection':
        return { session_id: 'scan-1', output_path: 'marked.docx', selected_text: '148', placeholder: '{{document.number}}', extracted_text: 'Номер: {{document.number}}', document_closed: true } as never;
      case 'close_word_scanner': return true as never;
      case 'save_learned_scanner_rule':
      case 'list_learned_scanner_rules': return [] as never;
      case 'delete_learned_scanner_rule': return [] as never;
      case 'get_diary_plan':
        return [{ day_number: 1, date: '02.06.2026', month: 6, year: 2026 }] as never;
      case 'get_record_series_plan':
        return [{ sequence_number: 1, timestamp: '2026-06-02T09:00:00', date: '02.06.2026', time: '09:00' }] as never;
      case 'icd10_suggest':
        return [{ code: 'F20.0', title: 'Example' }] as never;
      case 'get_output_plan':
        return { root_folder: 'C:/Desktop', patient_folder: 'C:/Desktop/Иванов', files: ['C:/Desktop/Иванов/Документ.docx'], warnings: [] } as never;
      case 'route_intake':
        return { should_start_ui: false, should_raise_existing_window: true, reason: 'raise existing window' } as never;
      case 'save_state':
        return undefined as never;
      case 'validate_product_access':
        return { accepted: true, mode: 'vip', plan: 'vip', reason: 'ok', watermark: null, document_limit_month: 1000000, max_documents_per_run: 5000 } as never;
      case 'verify_rust_license_text':
        return true as never;
      case 'check_for_updates':
        return { current_version: '18.0.7', latest_version: '18.0.8', update_available: true, artifact_path: '/tmp/app.zip', release_notes: null } as never;
      case 'install_background_watcher':
        return { platform: 'windows', installed: true, watch_folder: 'C:/x', commands: [], warnings: [] } as never;
      case 'update_background_watcher_preferences':
        return true as never;
      case 'uninstall_background_watcher':
        return { platform: 'windows', installed: false, commands: [], warnings: [] } as never;
      case 'get_print_triage':
        return { decision: 'auto_print', auto_print_allowed: true, confidence_score: 0.99, checked_document_ids: ['doc_1'], unapproved_document_ids: [], missing_fields: [], blockers: [], diff: [], reasons: [] } as never;
      case 'list_template_approvals':
        return [{ document_id: 'doc_1', template_sha256: 'a'.repeat(64), jurisdiction: 'RU', approved_by: 'Иванов И.И.', approved_at: 'now', note: '' }] as never;
      case 'approve_document_template':
        return { document_id: 'doc_1', template_sha256: 'a'.repeat(64), jurisdiction: 'RU', approved_by: 'Иванов И.И.', approved_at: 'now', note: '' } as never;
      case 'revoke_document_template_approval':
        return [] as never;
      case 'import_business_registry':
        return { total_records: 1, imported_records: 1, replaced: false } as never;
      case 'lookup_business_registry':
        return { inn: '7736050003', name: 'ООО Ромашка', kpp: '773601001', ogrn: '1027700000000', legal_address: 'Москва', source: 'authorized-export', source_updated_at: '2026-07-21' } as never;
      case 'apply_business_registry_record':
        return { values: { 'counterparty.inn': '7736050003', 'counterparty.name': 'ООО Ромашка' } } as never;
      case 'export_one_c_counterparties':
        return 'output/Контрагенты_1С.json' as never;
      case 'print_files':
        return { queued_files: ['out.docx'], queued_copies: 3, failed_files: [] } as never;
      case 'open_in_file_manager':
        return undefined as never;
      case 'run_created_documents_intake':
        return { status: 'processed', patient_folder: 'C:/Desktop/Созданные документы/Иванов', created_files: ['C:/Desktop/Созданные документы/Иванов/Документ.docx'], missing: [], attention_file: null, message: 'ok' } as never;
      case 'import_template_file':
        return { template_path: '/app-data/user-templates/doc_1.docx', extracted_text: 'Title {{field}}' } as never;
      case 'semantic_extract':
        return { fields: [{ field_id: 'org.inn', value: '7736050003', confidence: 0.9, method: 'typed:inn' }], warnings: [], model_applied: false, prompt: 'PROMPT' } as never;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

describe('Tauri command DTO contracts', () => {
  afterEach(() => __resetInvokeForTests());

  it('exports every command registered by the Rust backend', () => {
    expect([...rustCommandNames].sort()).toEqual([...registeredBackendCommands].sort());
  });

  it('uses Rust DTO envelopes for template setup commands', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await analyzeTemplate('Title\n{{field}}', 'doc_1', 'template.docx', 'Button');
    await analyzeTemplateFile('template.docx', 'doc_2');
    await prepareTemplateSetup([{ document_id: 'doc_1', template_path: 'template.docx', extracted_text: 'Title', preferred_button_label: 'Button' }]);
    await confirmTemplateSetup([confirmationRow]);
    await renameDocumentButton('doc_1', 'Новое имя');
    await removeDocumentButton('doc_1');
    await updateDocumentPopupFields('doc_1', []);
    expect(calls).toMatchObject([
      { command: 'analyze_template', payload: { req: { template_text: 'Title\n{{field}}', document_id: 'doc_1', template_path: 'template.docx', button_label: 'Button' } } },
      { command: 'analyze_template_file', payload: { req: { template_path: 'template.docx', document_id: 'doc_2', button_label: null } } },
      { command: 'prepare_template_setup', payload: { req: { candidates: [{ document_id: 'doc_1', template_path: 'template.docx', extracted_text: 'Title', preferred_button_label: 'Button' }] } } },
      { command: 'confirm_template_setup', payload: { req: { rows: [confirmationRow] } } },
      { command: 'rename_document_button', payload: { req: { document_id: 'doc_1', button_label: 'Новое имя' } } },
      { command: 'remove_document_button', payload: { req: { document_id: 'doc_1' } } },
      { command: 'update_document_popup_fields', payload: { req: { document_id: 'doc_1', popup_fields: [] } } },
    ]);
  });

  it('uses Rust DTO envelopes for source, field, workflow, popup, and scanner commands', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await firstRunState();
    await resetCase();
    await parseSource('Первичный документ', 2026);
    await parseSourceFile('source.pdf', 'JVBERi0=', 2026);
    await getIntakeCapabilities();
    await getSidecarStatus();
    await getComponentStatuses();
    await refreshComponentCatalog();
    await installComponent('ocr');
    await removeComponent('ocr');
    await parseWebSource('https://example.com/doc', 2026);
    await getDocumentTemplateText('doc_1');
    await setField('person.full_name', 'Иванов Иван');
    await getWorkflowPlan('doc_1', true);
    await getWorkflowPlanBatch(['doc_1'], true);
    await applyPopup('doc_1', true, [{ field_id: 'medical.case_number', value: '123' }]);
    await applyPopupBatch(['doc_1'], true, [{ field_id: 'medical.case_number', value: '123' }]);
    await applyScanner([{ field_id: 'scanner.note', selected_text: 'note', page_index: 0, confidence: 0.9 }]);
    await startWordScanner('source.docx', 'source', false);
    await activateWordScanner('scan-1');
    await captureWordScanner('scan-1', true);
    await applyWordScannerSelection('scan-1', 'document.number', 'replace');
    await closeWordScanner('scan-1', false);
    await saveLearnedScannerRule({ fieldId: 'document.number', title: 'Номер документа', selectedText: '148', contextText: 'Номер: 148', beforeText: 'Номер: ', afterText: '', inputKind: 'text' });
    await listLearnedScannerRules();
    await deleteLearnedScannerRule('rule-1');
    await updateDocumentTemplate('doc_1', 'marked.docx');
    await listTemplateVersions('doc_1');
    await rollbackTemplateVersion('tpl-v1');
    expect(calls).toMatchObject([
      { command: 'first_run_state', payload: undefined },
      { command: 'reset_case', payload: undefined },
      { command: 'parse_source', payload: { req: { source_text: 'Первичный документ', default_year: 2026 } } },
      { command: 'parse_source_file', payload: { req: { file_name: 'source.pdf', bytes_base64: 'JVBERi0=', default_year: 2026 } } },
      { command: 'get_intake_capabilities', payload: undefined },
      { command: 'get_sidecar_status', payload: undefined },
      { command: 'get_component_statuses', payload: undefined },
      { command: 'refresh_component_catalog', payload: undefined },
      { command: 'install_component', payload: { id: 'ocr' } },
      { command: 'remove_component', payload: { id: 'ocr' } },
      { command: 'parse_web_source', payload: { req: { url: 'https://example.com/doc', default_year: 2026 } } },
      { command: 'get_document_template_text', payload: { req: { document_id: 'doc_1' } } },
      { command: 'set_field', payload: { req: { field_id: 'person.full_name', value: 'Иванов Иван' } } },
      { command: 'get_workflow_plan', payload: { req: { document_id: 'doc_1', sick_leave_enabled: true } } },
      { command: 'get_workflow_plan_batch', payload: { req: { document_ids: ['doc_1'], sick_leave_enabled: true } } },
      { command: 'apply_popup', payload: { req: { document_id: 'doc_1', sick_leave_enabled: true, answers: [{ field_id: 'medical.case_number', value: '123' }] } } },
      { command: 'apply_popup_batch', payload: { req: { document_ids: ['doc_1'], sick_leave_enabled: true, answers: [{ field_id: 'medical.case_number', value: '123' }] } } },
      { command: 'apply_scanner', payload: { req: { marks: [{ field_id: 'scanner.note', selected_text: 'note', page_index: 0, confidence: 0.9 }] } } },
      { command: 'start_word_scanner', payload: { req: { path: 'source.docx', mode: 'source', make_working_copy: false } } },
      { command: 'activate_word_scanner', payload: { req: { session_id: 'scan-1' } } },
      { command: 'capture_word_scanner', payload: { req: { session_id: 'scan-1', close_after_capture: true } } },
      { command: 'apply_word_scanner_selection', payload: { req: { session_id: 'scan-1', field_id: 'document.number', action: 'replace' } } },
      { command: 'close_word_scanner', payload: { req: { session_id: 'scan-1', discard_working_copy: false } } },
      { command: 'save_learned_scanner_rule', payload: { req: { field_id: 'document.number', title: 'Номер документа', selected_text: '148', context_text: 'Номер: 148', before_text: 'Номер: ', after_text: '', input_kind: 'text' } } },
      { command: 'list_learned_scanner_rules', payload: undefined },
      { command: 'delete_learned_scanner_rule', payload: { req: { rule_id: 'rule-1' } } },
      { command: 'update_document_template', payload: { req: { document_id: 'doc_1', template_path: 'marked.docx' } } },
      { command: 'list_template_versions', payload: { req: { document_id: 'doc_1' } } },
      { command: 'rollback_template_version', payload: { req: { version_id: 'tpl-v1' } } },
    ]);
  });

  it('uses Rust DTO envelopes for privacy, exceptions, metrics and audit', async () => {
    const calls: Call[] = []; installContractMock(calls);
    const preferences = { copy_source_to_output: false, write_trust_report: true, include_values_in_trust_report: false, temp_retention_hours: 12, archive_processed_sources: true, archive_folder_name: '_обработано', service_note_retention_days: 30, processed_marker_retention_days: 7, archived_source_retention_days: 0 };
    await getPrivacyPreferences();
    await updatePrivacyPreferences(preferences);
    await runWorkspaceHygiene();
    await listAutomationExceptions(true);
    await resolveAutomationException('ex-1', 'Проверено специалистом');
    await confirmRiskExceptionAndRetry('ex-1');
    await getAutomationMetrics();
    await getQueueStatus();
    await getCorpusStatus();
    await getLearnedKitDecision('hr', 'employment-intake', 'hr.ru.v1');
    await exportCorpus('corpus.json', 500);
    await getCalibratedThresholdStatus();
    await importCalibratedThresholdsFile('thresholds.signed.json', 'e30=');
    await listAuditEvents(25);
    expect(calls).toMatchObject([
      { command: 'get_privacy_preferences', payload: undefined },
      { command: 'update_privacy_preferences', payload: { req: { preferences } } },
      { command: 'run_workspace_hygiene', payload: undefined },
      { command: 'list_automation_exceptions', payload: { req: { include_resolved: true } } },
      { command: 'resolve_automation_exception', payload: { req: { exception_id: 'ex-1', resolution: 'Проверено специалистом' } } },
      { command: 'confirm_risk_exception_and_retry', payload: { req: { exception_id: 'ex-1' } } },
      { command: 'get_automation_metrics', payload: undefined },
      { command: 'get_queue_status', payload: undefined },
      { command: 'get_corpus_status', payload: undefined },
      { command: 'get_learned_kit_decision', payload: { req: { domain: 'hr', cluster_id: 'employment-intake', pack_id: 'hr.ru.v1' } } },
      { command: 'export_corpus', payload: { req: { output_path: 'corpus.json', limit: 500 } } },
      { command: 'get_calibrated_threshold_status', payload: undefined },
      { command: 'import_calibrated_thresholds', payload: { req: { path: null, file_name: 'thresholds.signed.json', bytes_base64: 'e30=' } } },
      { command: 'list_audit_events', payload: { req: { limit: 25 } } },
    ]);
  });

  it('uses Rust DTO envelopes for v18 blocks, markup and mail merge', async () => {
    const calls: Call[] = []; installContractMock(calls);
    await listClauseBlocks(); await saveClauseBlock('requisites', 'Реквизиты', '{{org.name}}'); await deleteClauseBlock('requisites');
    await suggestTemplateMarkup('t.docx', 'UEsDBA==', 2026); await applyTemplateMarkup('t.docx', 't.marked.docx', [{ field_id: 'org.inn', value: '7736050003' }]);
    await previewMailMerge('subject.name\nИванов'); await renderMailMerge(['doc_1'], 'subject.name\nИванов', 'output', true);
    expect(calls).toMatchObject([
      { command: 'list_clause_blocks', payload: undefined },
      { command: 'save_clause_block', payload: { req: { block_id: 'requisites', title: 'Реквизиты', content: '{{org.name}}' } } },
      { command: 'delete_clause_block', payload: { req: { block_id: 'requisites' } } },
      { command: 'suggest_template_markup_command', payload: { req: { file_name: 't.docx', bytes_base64: 'UEsDBA==', default_year: 2026 } } },
      { command: 'apply_template_markup_command', payload: { req: { input_path: 't.docx', output_path: 't.marked.docx', replacements: [{ field_id: 'org.inn', value: '7736050003' }] } } },
      { command: 'preview_mail_merge', payload: { req: { delimited_text: 'subject.name\nИванов' } } },
      { command: 'render_mail_merge', payload: { req: { document_ids: ['doc_1'], delimited_text: 'subject.name\nИванов', output_root: 'output', strict: true } } },
    ]);
  });

  it('uses Rust DTO envelopes for diary, output, intake, and storage commands', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await getDiaryPlan('01.06.2026', '03.06.2026', 2026);
    await getRecordSeriesPlan({ start_date: '01.06.2026', end_date: '03.06.2026', default_year: 2026, start_offset_days: 0, cadence: { kind: 'daily' }, day_start_time: '09:00', day_end_time: null, skip_weekdays: [], excluded_dates: [] });
    await getOutputPlan('C:/Desktop', ['FullSubjectName'], ['Документ']);
    await routeIntake(true, true);
    await saveState('state.sqlite');
    await loadState('state.sqlite');
    expect(calls).toMatchObject([
      { command: 'get_diary_plan', payload: { req: { admission_date: '01.06.2026', discharge_date: '03.06.2026', default_year: 2026 } } },
      { command: 'get_record_series_plan', payload: { req: { start_date: '01.06.2026', end_date: '03.06.2026', default_year: 2026, start_offset_days: 0, cadence: { kind: 'daily' }, day_start_time: '09:00', day_end_time: null, skip_weekdays: [], excluded_dates: [] } } },
      { command: 'get_output_plan', payload: { req: { root_folder: 'C:/Desktop', folder_parts: ['FullSubjectName'], button_labels: ['Документ'] } } },
      { command: 'route_intake', payload: { req: { app_already_running: true, user_requested_ui: true } } },
      { command: 'save_state', payload: { req: { db_path: 'state.sqlite' } } },
      { command: 'load_state', payload: { req: { db_path: 'state.sqlite' } } },
    ]);
  });

  it('uses Rust DTO envelopes for print triage, approvals, and business registry commands', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await getPrintTriage(['doc_1'], 'C:/review');
    await listTemplateApprovals();
    await approveDocumentTemplate({ documentId: 'doc_1', jurisdiction: 'RU', approvedBy: 'Иванов И.И.', note: 'Утверждено организацией', acknowledgement: true });
    await revokeDocumentTemplateApproval('doc_1');
    const record = { inn: '7736050003', name: 'ООО Ромашка', kpp: '773601001', ogrn: '1027700000000', legal_address: 'Москва', source: 'authorized-export', source_updated_at: '2026-07-21' };
    await importBusinessRegistry([record], false);
    await lookupBusinessRegistry('7736050003');
    await applyBusinessRegistryRecord('7736050003', 'counterparty');
    await exportOneCCounterparties('C:/output/Контрагенты_1С.json', ['7736050003']);
    expect(calls).toMatchObject([
      { command: 'get_print_triage', payload: { req: { document_ids: ['doc_1'], output_folder: 'C:/review' } } },
      { command: 'list_template_approvals', payload: undefined },
      { command: 'approve_document_template', payload: { req: { document_id: 'doc_1', jurisdiction: 'RU', approved_by: 'Иванов И.И.', note: 'Утверждено организацией', acknowledgement: true } } },
      { command: 'revoke_document_template_approval', payload: { documentId: 'doc_1' } },
      { command: 'import_business_registry', payload: { req: { records: [record], replace: false } } },
      { command: 'lookup_business_registry', payload: { req: { inn: '7736050003' } } },
      { command: 'apply_business_registry_record', payload: { req: { inn: '7736050003', target: 'counterparty' } } },
      { command: 'export_one_c_counterparties', payload: { req: { output_path: 'C:/output/Контрагенты_1С.json', inns: ['7736050003'] } } },
    ]);
  });

  it('uses Rust DTO envelopes for rendering, ICD, license, and background commands', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await renderPreview('Text {{field}}', false);
    await renderDocx('doc_1', 'out.docx', true);
    await renderDocxBatch(['doc_1'], 'C:/Desktop', ['DocumentNumber', 'DocumentDate'], true);
    await icd10Suggest('F20');
    await validateProductAccess('000000');
    await verifyRustLicenseText('license');
    await installBackgroundWatcher('C:/watch', 2026, false, ['DocumentNumber'], true, { doc_1: 3 });
    await updateBackgroundWatcherPreferences(false, { doc_1: 7 });
    await uninstallBackgroundWatcher();
    await runCreatedDocumentsIntake('C:/Desktop/Созданные документы/Первичный.docx', 'C:/Desktop/Созданные документы', ['FullSubjectName'], 2026, false);
    await printFiles([{ path: 'out.docx', copies: 3 }]);
    await pickFolder('C:/Desktop');
    await openInFileManager('C:/Desktop/output');
    await semanticExtract('ИНН 7736050003', 2026);
    await importTemplateFile('doc_1', { fileName: 'Договор.docx', bytesBase64: 'UEsDBA==' });
    await importTemplateFile('doc_2', { templateText: 'Title {{field}}' });
    expect(calls).toMatchObject([
      { command: 'render_preview', payload: { req: { template_text: 'Text {{field}}', strict: false } } },
      { command: 'render_docx', payload: { req: { document_id: 'doc_1', output_path: 'out.docx', strict: true } } },
      { command: 'render_docx_batch', payload: { req: { document_ids: ['doc_1'], output_root: 'C:/Desktop', folder_parts: ['DocumentNumber', 'DocumentDate'], strict: true } } },
      { command: 'icd10_suggest', payload: { query: 'F20' } },
      { command: 'validate_product_access', payload: { req: { code: '000000' } } },
      { command: 'verify_rust_license_text', payload: { req: { license_text: 'license' } } },
      { command: 'install_background_watcher', payload: { req: { watch_folder: 'C:/watch', default_year: 2026, sick_leave_enabled: false, folder_parts: ['DocumentNumber'], auto_print: true, print_copies_by_document: { doc_1: 3 } } } },
      { command: 'update_background_watcher_preferences', payload: { req: { auto_print: false, print_copies_by_document: { doc_1: 7 } } } },
      { command: 'uninstall_background_watcher', payload: undefined },
      { command: 'run_created_documents_intake', payload: { req: { source_path: 'C:/Desktop/Созданные документы/Первичный.docx', output_root: 'C:/Desktop/Созданные документы', folder_parts: ['FullSubjectName'], default_year: 2026, sick_leave_enabled: false } } },
      { command: 'print_files', payload: { req: { jobs: [{ path: 'out.docx', copies: 3 }] } } },
      { command: 'pick_folder', payload: { req: { initial_path: 'C:/Desktop' } } },
      { command: 'open_in_file_manager', payload: { req: { path: 'C:/Desktop/output' } } },
      { command: 'semantic_extract', payload: { req: { source_text: 'ИНН 7736050003', default_year: 2026, model_output: null } } },
      { command: 'import_template_file', payload: { req: { document_id: 'doc_1', file_name: 'Договор.docx', bytes_base64: 'UEsDBA==', template_text: null } } },
      { command: 'import_template_file', payload: { req: { document_id: 'doc_2', file_name: null, bytes_base64: null, template_text: 'Title {{field}}' } } },
    ]);
  });

  it('validates minimal response DTO shapes without implementing business logic in TypeScript', async () => {
    const calls: Call[] = [];
    installContractMock(calls);
    await expect(firstRunState()).resolves.toMatchObject({ has_user_buttons: true, pack: { documents: [expect.any(Object)] } });
    await expect(getWorkflowPlan('doc_1', false)).resolves.toMatchObject({ document_id: 'doc_1', prompts: [], blocked: false });
    await expect(getDiaryPlan('01.06.2026', '03.06.2026', 2026)).resolves.toEqual([expect.objectContaining({ date: expect.any(String) })]);
    await expect(getOutputPlan('C:/Desktop', ['FullSubjectName'], ['Документ'])).resolves.toMatchObject({ patient_folder: expect.any(String), files: expect.any(Array) });
    await expect(validateProductAccess(null)).resolves.toMatchObject({ accepted: true, document_limit_month: expect.any(Number) });
    await expect(icd10Suggest('F')).resolves.toEqual([expect.objectContaining({ code: expect.any(String), title: expect.any(String) })]);
  });
});
