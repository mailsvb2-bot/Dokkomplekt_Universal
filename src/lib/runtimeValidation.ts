import type { CreatedDocumentsIntakeResult } from './types';

export class BackendContractError extends Error {
  readonly command: string;

  constructor(command: string, detail: string) {
    super(`Некорректный ответ внутреннего модуля «${command}»: ${detail}`);
    this.name = 'BackendContractError';
    this.command = command;
  }
}

function record(command: string, value: unknown, label = 'ответ'): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new BackendContractError(command, `${label} должен быть объектом`);
  }
  return value as Record<string, unknown>;
}

function array(command: string, value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new BackendContractError(command, `${label} должен быть массивом`);
  }
  return value;
}

function objectArray(command: string, value: unknown, label: string): Record<string, unknown>[] {
  const items = array(command, value, label);
  return items.map((item, index) => record(command, item, `${label}[${index}]`));
}

function string(command: string, value: unknown, label: string): string {
  if (typeof value !== 'string') {
    throw new BackendContractError(command, `${label} должен быть строкой`);
  }
  return value;
}

function boolean(command: string, value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') {
    throw new BackendContractError(command, `${label} должен быть логическим значением`);
  }
  return value;
}

function number(command: string, value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new BackendContractError(command, `${label} должен быть конечным числом`);
  }
  return value;
}

function stringArray(command: string, value: unknown, label: string): string[] {
  const items = array(command, value, label);
  if (!items.every((item) => typeof item === 'string')) {
    throw new BackendContractError(command, `${label} должен содержать только строки`);
  }
  return items as string[];
}

function optionalStringArray(command: string, value: unknown, label: string): void {
  if (value !== undefined && value !== null) stringArray(command, value, label);
}

function validateDocument(command: string, value: unknown): void {
  const item = record(command, value, 'документ');
  string(command, item.id, 'document.id');
  string(command, item.button_label, 'document.button_label');
  string(command, item.template_path, 'document.template_path');
  array(command, item.required_fields, 'document.required_fields');
  array(command, item.placeholders, 'document.placeholders');
}

function validateDocumentPack(command: string, value: unknown): void {
  const pack = record(command, value, 'pack');
  string(command, pack.pack_id, 'pack.pack_id');
  string(command, pack.name, 'pack.name');
  objectArray(command, pack.documents, 'pack.documents').forEach((item) => validateDocument(command, item));
}

function validateFirstRun(command: string, value: unknown): void {
  const root = record(command, value);
  validateDocumentPack(command, root.pack);
  boolean(command, root.has_user_buttons, 'has_user_buttons');
  string(command, root.message, 'message');
}

function validateWorkflow(command: string, value: unknown): void {
  const root = record(command, value);
  objectArray(command, root.prompts, 'prompts');
  boolean(command, root.blocked, 'blocked');
  stringArray(command, root.block_reasons, 'block_reasons');
}

function validateBatch(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.output_folder, 'output_folder');
  stringArray(command, root.created_files, 'created_files');
  if (root.created_documents !== undefined) objectArray(command, root.created_documents, 'created_documents');
  optionalStringArray(command, root.warnings, 'warnings');
  if (root.backup_folder !== undefined && root.backup_folder !== null) string(command, root.backup_folder, 'backup_folder');
}

function validateSemantic(command: string, value: unknown): void {
  const root = record(command, value);
  objectArray(command, root.fields, 'fields');
  stringArray(command, root.warnings, 'warnings');
  boolean(command, root.model_applied, 'model_applied');
  string(command, root.prompt, 'prompt');
}

function validateRouting(command: string, value: unknown): void {
  const routing = record(command, value, 'routing');
  stringArray(command, routing.recommended_document_ids, 'routing.recommended_document_ids');
  objectArray(command, routing.matches, 'routing.matches');
  stringArray(command, routing.reasons, 'routing.reasons');
  boolean(command, routing.auto_select, 'routing.auto_select');
  boolean(command, routing.review_required, 'routing.review_required');
}

function validateBundleDecision(command: string, value: unknown): void {
  const decision = record(command, value, 'bundle_decision');
  stringArray(command, decision.document_ids, 'bundle_decision.document_ids');
  string(command, decision.source, 'bundle_decision.source');
  number(command, decision.confidence, 'bundle_decision.confidence');
  boolean(command, decision.auto_apply, 'bundle_decision.auto_apply');
  boolean(command, decision.review_required, 'bundle_decision.review_required');
  stringArray(command, decision.reasons, 'bundle_decision.reasons');
  if (decision.question !== undefined && decision.question !== null) {
    string(command, decision.question, 'bundle_decision.question');
  }
}

function validateParseSource(command: string, value: unknown): void {
  const root = record(command, value);
  const semanticCase = record(command, root.semantic_case, 'semantic_case');
  record(command, semanticCase.values, 'semantic_case.values');
  const report = record(command, root.report, 'report');
  stringArray(command, report.warnings, 'report.warnings');
  validateRouting(command, root.routing);
  validateBundleDecision(command, root.bundle_decision);
}

function validateRender(command: string, value: unknown): void {
  const root = record(command, value);
  optionalStringArray(command, root.missing_fields, 'missing_fields');
  optionalStringArray(command, root.unknown_fields, 'unknown_fields');
  optionalStringArray(command, root.warnings, 'warnings');
  optionalStringArray(command, root.template_errors, 'template_errors');
  if (root.output_text !== undefined) string(command, root.output_text, 'output_text');
  if (root.output_path !== undefined) string(command, root.output_path, 'output_path');
}

function validatePopup(command: string, value: unknown): void {
  const root = record(command, value);
  boolean(command, root.accepted, 'accepted');
  const semanticCase = record(command, root.semantic_case, 'semantic_case');
  record(command, semanticCase.values, 'semantic_case.values');
  objectArray(command, root.still_missing, 'still_missing');
  string(command, root.message, 'message');
  optionalStringArray(command, root.errors, 'errors');
}

function validateWatcher(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.platform, 'platform');
  boolean(command, root.installed, 'installed');
  optionalStringArray(command, root.args, 'args');
  optionalStringArray(command, root.autostart_files, 'autostart_files');
  optionalStringArray(command, root.removed_files, 'removed_files');
  optionalStringArray(command, root.commands, 'commands');
  optionalStringArray(command, root.warnings, 'warnings');
}

function validatePrintFiles(command: string, value: unknown): void {
  const root = record(command, value);
  stringArray(command, root.queued_files, 'queued_files');
  number(command, root.queued_copies, 'queued_copies');
  objectArray(command, root.failed_files, 'failed_files');
}

function validatePrinterInventory(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.platform, 'platform');
  objectArray(command, root.printers, 'printers');
  record(command, root.preferences, 'preferences');
  if (root.discovery_error !== undefined && root.discovery_error !== null) {
    string(command, root.discovery_error, 'discovery_error');
  }
  string(command, root.advanced_options_note, 'advanced_options_note');
}

function validatePdfExport(command: string, value: unknown): void {
  const root = record(command, value);
  stringArray(command, root.created_files, 'created_files');
  objectArray(command, root.failed_files, 'failed_files');
  boolean(command, root.pdfa_1_requested, 'pdfa_1_requested');
  string(command, root.conformance_note, 'conformance_note');
}

function validateProcessBlueprints(command: string, value: unknown): void {
  const root = record(command, value);
  objectArray(command, root.processes, 'processes');
  string(command, root.notice, 'notice');
}

function validateOutputPlan(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.root_folder, 'root_folder');
  string(command, root.patient_folder, 'patient_folder');
  stringArray(command, root.files, 'files');
  stringArray(command, root.warnings, 'warnings');
  boolean(command, root.exists, 'exists');
}

function validateIntakeRoute(command: string, value: unknown): void {
  const root = record(command, value);
  boolean(command, root.should_start_ui, 'should_start_ui');
  boolean(command, root.should_raise_existing_window, 'should_raise_existing_window');
  string(command, root.reason, 'reason');
}

function validateScanner(command: string, value: unknown): void {
  const root = record(command, value);
  stringArray(command, root.applied_fields, 'applied_fields');
  stringArray(command, root.rejected_fields, 'rejected_fields');
}

function validateUpdateCheck(command: string, value: unknown): void {
  const root = record(command, value);
  boolean(command, root.available, 'available');
  string(command, root.current_version, 'current_version');
  string(command, root.latest_version, 'latest_version');
  string(command, root.platform, 'platform');
  string(command, root.message, 'message');
}

function validateSemanticModelStatus(command: string, value: unknown, label = 'status'): void {
  const root = record(command, value, label);
  boolean(command, root.configured, `${label}.configured`);
  boolean(command, root.reachable, `${label}.reachable`);
  stringArray(command, root.available_models, `${label}.available_models`);
  string(command, root.message, `${label}.message`);
}

function validateWorkspaceHygiene(command: string, value: unknown): void {
  const root = record(command, value);
  stringArray(command, root.archived_processed_sources, 'archived_processed_sources');
  stringArray(command, root.archived_service_files, 'archived_service_files');
  stringArray(command, root.removed_orphan_markers, 'removed_orphan_markers');
  stringArray(command, root.removed_expired_archived_files, 'removed_expired_archived_files');
  stringArray(command, root.warnings, 'warnings');
}

function validateQualityTelemetry(command: string, value: unknown): void {
  const root = record(command, value);
  objectArray(command, root.stop_reasons, 'stop_reasons');
  objectArray(command, root.unrecognized_fields, 'unrecognized_fields');
  objectArray(command, root.broken_templates, 'broken_templates');
  objectArray(command, root.excluded_documents, 'excluded_documents');
  objectArray(command, root.repeated_confirmations, 'repeated_confirmations');
  objectArray(command, root.suggestions, 'suggestions');
}

function validateReferenceData(command: string, value: unknown): void {
  const root = record(command, value);
  boolean(command, root.installed, 'installed');
  boolean(command, root.cached, 'cached');
  boolean(command, root.restart_required, 'restart_required');
  array(command, root.complete_years, 'complete_years').forEach((item, index) => number(command, item, `complete_years[${index}]`));
  array(command, root.listed_years, 'listed_years').forEach((item, index) => number(command, item, `listed_years[${index}]`));
  string(command, root.message, 'message');
}

function validateComponentArray(command: string, value: unknown): void {
  objectArray(command, value, 'ответ').forEach((item, index) => {
    string(command, item.id, `ответ[${index}].id`);
    string(command, item.label, `ответ[${index}].label`);
    boolean(command, item.installed, `ответ[${index}].installed`);
    boolean(command, item.available, `ответ[${index}].available`);
  });
}

export function normalizeCreatedDocumentsIntakeResult(
  value: unknown,
  command = 'document-batch-ready',
): CreatedDocumentsIntakeResult {
  const root = record(command, value);
  const status = string(command, root.status, 'status');
  if (!['processed', 'attention', 'setup_needed', 'ignored'].includes(status)) {
    throw new BackendContractError(command, `неизвестный status: ${status}`);
  }
  const patientFolder = root.patient_folder;
  if (patientFolder !== null && typeof patientFolder !== 'string') {
    throw new BackendContractError(command, 'patient_folder должен быть строкой или null');
  }
  const attentionFile = root.attention_file;
  if (attentionFile !== null && typeof attentionFile !== 'string') {
    throw new BackendContractError(command, 'attention_file должен быть строкой или null');
  }
  const createdFiles = stringArray(command, root.created_files, 'created_files');
  const missing = stringArray(command, root.missing, 'missing');
  const message = string(command, root.message, 'message');
  if (root.created_documents !== undefined) objectArray(command, root.created_documents, 'created_documents');
  return {
    ...(root as unknown as CreatedDocumentsIntakeResult),
    status: status as CreatedDocumentsIntakeResult['status'],
    patient_folder: patientFolder,
    attention_file: attentionFile,
    created_files: createdFiles,
    missing,
    message,
  };
}

type ResponseKind = 'array' | 'boolean' | 'string' | 'void' | 'nullable-object' | 'object';

export const COMMAND_RESPONSE_KIND = {
  'activate_word_scanner': 'boolean',
  'analyze_template': 'object',
  'analyze_template_file': 'object',
  'apply_business_registry_record': 'object',
  'apply_organization_knowledge': 'object',
  'apply_popup': 'object',
  'apply_popup_batch': 'object',
  'apply_scanner': 'object',
  'apply_template_learning_map': 'object',
  'apply_template_markup_command': 'object',
  'apply_word_scanner_selection': 'object',
  'approve_document_template': 'object',
  'capture_word_scanner': 'object',
  'check_for_updates': 'object',
  'check_template_regression': 'nullable-object',
  'close_word_scanner': 'boolean',
  'confirm_bundle_exception_and_retry': 'object',
  'confirm_risk_exception_and_retry': 'object',
  'confirm_template_setup': 'object',
  'create_kedo_package': 'object',
  'delete_clause_block': 'array',
  'delete_learned_scanner_rule': 'array',
  'delete_organization_knowledge': 'array',
  'export_corpus': 'object',
  'export_files_to_pdf': 'object',
  'export_one_c_counterparties': 'string',
  'first_run_state': 'object',
  'get_default_output_root': 'string',
  'get_automation_metrics': 'object',
  'get_calibrated_threshold_status': 'array',
  'get_component_statuses': 'array',
  'get_corpus_status': 'object',
  'get_daily_automation_dashboard': 'object',
  'get_diary_plan': 'array',
  'get_document_template_text': 'object',
  'get_intake_capabilities': 'array',
  'get_learned_kit_decision': 'nullable-object',
  'get_output_plan': 'object',
  'get_print_triage': 'object',
  'get_printer_inventory': 'object',
  'get_privacy_preferences': 'object',
  'get_process_blueprints': 'object',
  'get_quality_telemetry': 'object',
  'get_queue_status': 'object',
  'get_record_series_plan': 'array',
  'get_reference_data_status': 'object',
  'get_semantic_model_config': 'object',
  'get_sidecar_status': 'array',
  'get_workflow_plan': 'object',
  'get_workflow_plan_batch': 'object',
  'icd10_suggest': 'array',
  'import_business_registry': 'object',
  'import_calibrated_thresholds': 'object',
  'import_learning_example_file': 'object',
  'import_reference_data': 'object',
  'import_template_file': 'object',
  'install_background_watcher': 'object',
  'install_component': 'object',
  'learn_template_from_examples_command': 'object',
  'list_audit_events': 'array',
  'list_automation_exceptions': 'array',
  'list_case_runs': 'array',
  'list_clause_blocks': 'array',
  'list_learned_scanner_rules': 'array',
  'list_organization_knowledge': 'array',
  'list_template_approvals': 'array',
  'list_template_versions': 'array',
  'load_state': 'object',
  'lookup_business_registry': 'nullable-object',
  'open_in_file_manager': 'void',
  'pick_template_files': 'object',
  'pick_folder': 'object',
  'parse_source': 'object',
  'parse_source_file': 'object',
  'parse_web_source': 'object',
  'prepare_mail_merge_file': 'object',
  'prepare_template_setup': 'array',
  'preview_mail_merge': 'object',
  'print_files': 'object',
  'refresh_component_catalog': 'array',
  'register_learned_template': 'object',
  'remove_component': 'object',
  'remove_document_button': 'object',
  'rename_document_button': 'object',
  'render_docx': 'object',
  'render_docx_batch': 'object',
  'render_mail_merge': 'object',
  'render_preview': 'object',
  'reset_case': 'object',
  'resolve_automation_exception': 'boolean',
  'retry_case_run': 'object',
  'revoke_document_template_approval': 'array',
  'rollback_template_version': 'object',
  'route_intake': 'object',
  'run_created_documents_intake': 'object',
  'run_workspace_hygiene': 'object',
  'save_clause_block': 'array',
  'save_learned_scanner_rule': 'array',
  'save_state': 'void',
  'select_process_blueprint': 'object',
  'semantic_extract': 'object',
  'set_field': 'object',
  'start_word_scanner': 'object',
  'suggest_template_markup_command': 'array',
  'test_semantic_model': 'object',
  'uninstall_background_watcher': 'object',
  'update_background_watcher_preferences': 'boolean',
  'update_document_popup_fields': 'object',
  'update_document_template': 'object',
  'update_print_preferences': 'object',
  'update_privacy_preferences': 'object',
  'update_reference_data': 'object',
  'update_semantic_model_config': 'object',
  'upsert_organization_knowledge': 'array',
  'validate_product_access': 'object',
  'verify_rust_license_text': 'boolean',
} as const satisfies Readonly<Record<string, ResponseKind>>;

export function validateRustResponse<T>(command: string, value: unknown): T {
  const kind: ResponseKind | undefined = COMMAND_RESPONSE_KIND[command as keyof typeof COMMAND_RESPONSE_KIND];
  if (!kind) {
    throw new BackendContractError(command, 'для команды не зарегистрирован контракт ответа');
  }

  if (kind === 'void') {
    if (value !== null && value !== undefined) {
      throw new BackendContractError(command, 'команда без результата вернула непустое значение');
    }
    return value as T;
  }

  if (value === null || value === undefined) {
    if (kind === 'nullable-object') return value as T;
    throw new BackendContractError(command, 'получено пустое значение');
  }

  if (kind === 'array') {
    objectArray(command, value, 'ответ');
  } else if (kind === 'boolean') {
    boolean(command, value, 'ответ');
  } else if (kind === 'string') {
    string(command, value, 'ответ');
  } else {
    record(command, value);
  }

  switch (command) {
    case 'first_run_state':
    case 'load_state':
      validateFirstRun(command, value);
      break;
    case 'get_process_blueprints':
    case 'select_process_blueprint':
      validateProcessBlueprints(command, value);
      break;
    case 'analyze_template':
    case 'analyze_template_file': {
      const root = record(command, value);
      validateDocument(command, root.document);
      break;
    }
    case 'register_learned_template':
    case 'confirm_template_setup':
    case 'rename_document_button':
    case 'remove_document_button':
    case 'update_document_popup_fields':
    case 'update_document_template':
    case 'rollback_template_version':
      validateDocumentPack(command, value);
      break;
    case 'parse_source':
    case 'parse_source_file':
    case 'parse_web_source':
      validateParseSource(command, value);
      break;
    case 'get_workflow_plan':
    case 'get_workflow_plan_batch':
      validateWorkflow(command, value);
      break;
    case 'apply_popup':
    case 'apply_popup_batch':
      validatePopup(command, value);
      break;
    case 'render_docx':
    case 'render_preview':
      validateRender(command, value);
      break;
    case 'render_docx_batch':
      validateBatch(command, value);
      break;
    case 'apply_scanner':
      validateScanner(command, value);
      break;
    case 'get_output_plan':
      validateOutputPlan(command, value);
      break;
    case 'route_intake':
      validateIntakeRoute(command, value);
      break;
    case 'run_created_documents_intake':
    case 'retry_case_run':
    case 'confirm_risk_exception_and_retry':
    case 'confirm_bundle_exception_and_retry':
      normalizeCreatedDocumentsIntakeResult(value, command);
      break;
    case 'semantic_extract':
      validateSemantic(command, value);
      break;
    case 'pick_template_files': {
      const root = record(command, value);
      const files = array(command, root.files, 'files');
      files.forEach((item, index) => {
        const file = record(command, item, `files[${index}]`);
        string(command, file.file_name, `files[${index}].file_name`);
        string(command, file.template_path, `files[${index}].template_path`);
        string(command, file.extracted_text, `files[${index}].extracted_text`);
      });
      break;
    }
    case 'pick_folder': {
      const root = record(command, value);
      if (root.selected_path !== null) string(command, root.selected_path, 'selected_path');
      break;
    }
    case 'install_background_watcher':
    case 'uninstall_background_watcher':
      validateWatcher(command, value);
      break;
    case 'print_files':
      validatePrintFiles(command, value);
      break;
    case 'get_printer_inventory':
    case 'update_print_preferences':
      validatePrinterInventory(command, value);
      break;
    case 'export_files_to_pdf':
      validatePdfExport(command, value);
      break;
    case 'check_for_updates':
      validateUpdateCheck(command, value);
      break;
    case 'test_semantic_model':
      validateSemanticModelStatus(command, value);
      break;
    case 'get_semantic_model_config':
    case 'update_semantic_model_config': {
      const root = record(command, value);
      record(command, root.config, 'config');
      validateSemanticModelStatus(command, root.status, 'status');
      break;
    }
    case 'run_workspace_hygiene':
      validateWorkspaceHygiene(command, value);
      break;
    case 'get_quality_telemetry':
      validateQualityTelemetry(command, value);
      break;
    case 'get_reference_data_status':
    case 'update_reference_data':
    case 'import_reference_data':
      validateReferenceData(command, value);
      break;
    case 'get_component_statuses':
    case 'refresh_component_catalog':
      validateComponentArray(command, value);
      break;
    case 'install_component':
    case 'remove_component':
      validateComponentArray(command, [value]);
      break;
    case 'validate_product_access': {
      const root = record(command, value);
      boolean(command, root.accepted, 'accepted');
      string(command, root.plan, 'plan');
      number(command, root.document_limit_month, 'document_limit_month');
      number(command, root.max_documents_per_run, 'max_documents_per_run');
      break;
    }
    default:
      break;
  }
  return value as T;
}
