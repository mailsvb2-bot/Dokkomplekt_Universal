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

function validateDocument(command: string, value: unknown): void {
  const item = record(command, value, 'документ');
  string(command, item.id, 'document.id');
  string(command, item.button_label, 'document.button_label');
  string(command, item.template_path, 'document.template_path');
  array(command, item.required_fields, 'document.required_fields');
  array(command, item.placeholders, 'document.placeholders');
}

function validateFirstRun(command: string, value: unknown): void {
  const root = record(command, value);
  const pack = record(command, root.pack, 'pack');
  const documents = array(command, pack.documents, 'pack.documents');
  documents.forEach((item) => validateDocument(command, item));
  boolean(command, root.has_user_buttons, 'has_user_buttons');
  string(command, root.message, 'message');
}

function validateWorkflow(command: string, value: unknown): void {
  const root = record(command, value);
  array(command, root.prompts, 'prompts');
  boolean(command, root.blocked, 'blocked');
  stringArray(command, root.block_reasons, 'block_reasons');
}

function validateBatch(command: string, value: unknown): void {
  const root = record(command, value);
  string(command, root.output_folder, 'output_folder');
  stringArray(command, root.created_files, 'created_files');
  if (root.created_documents !== undefined) array(command, root.created_documents, 'created_documents');
}

function validateSemantic(command: string, value: unknown): void {
  const root = record(command, value);
  array(command, root.fields, 'fields');
  stringArray(command, root.warnings, 'warnings');
  boolean(command, root.model_applied, 'model_applied');
  string(command, root.prompt, 'prompt');
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
  if (root.created_documents !== undefined) array(command, root.created_documents, 'created_documents');
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

const ARRAY_COMMANDS = new Set([
  'get_intake_capabilities',
  'get_sidecar_status',
  'get_component_statuses',
  'refresh_component_catalog',
  'prepare_template_setup',
  'list_learned_scanner_rules',
  'get_diary_plan',
  'get_record_series_plan',
  'icd10_suggest',
  'list_organization_knowledge',
  'get_calibrated_threshold_status',
]);

const NULLABLE_COMMANDS = new Set([
  'save_state',
  'open_in_file_manager',
  'check_template_regression',
]);

export function validateRustResponse<T>(command: string, value: unknown): T {
  if (value === null || value === undefined) {
    if (NULLABLE_COMMANDS.has(command)) return value as T;
    throw new BackendContractError(command, 'получено пустое значение');
  }
  if (ARRAY_COMMANDS.has(command)) {
    array(command, value, 'ответ');
    return value as T;
  }
  switch (command) {
    case 'first_run_state':
    case 'load_state':
      validateFirstRun(command, value);
      break;
    case 'get_workflow_plan':
    case 'get_workflow_plan_batch':
      validateWorkflow(command, value);
      break;
    case 'render_docx_batch':
      validateBatch(command, value);
      break;
    case 'run_created_documents_intake':
      normalizeCreatedDocumentsIntakeResult(value, command);
      break;
    case 'semantic_extract':
      validateSemantic(command, value);
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
