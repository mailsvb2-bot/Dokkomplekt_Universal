import { describe, expect, it } from 'vitest';
import {
  BackendContractError,
  COMMAND_RESPONSE_KIND,
  normalizeCreatedDocumentsIntakeResult,
  validateRustResponse,
} from './runtimeValidation';

describe('runtime backend contracts', () => {
  it('registers a fail-closed response kind for every current Tauri command', () => {
    expect(Object.keys(COMMAND_RESPONSE_KIND)).toHaveLength(125);
    expect(COMMAND_RESPONSE_KIND.pick_template_files).toBe('object');
    expect(COMMAND_RESPONSE_KIND.pick_source_file).toBe('nullable-object');
    expect(COMMAND_RESPONSE_KIND.parse_source_path).toBe('object');
    expect(COMMAND_RESPONSE_KIND.import_component_bundle).toBe('array');
    expect(COMMAND_RESPONSE_KIND.pick_component_bundle).toBe('nullable-object');
    expect(COMMAND_RESPONSE_KIND.replace_clause_blocks).toBe('boolean');
    expect(() => validateRustResponse('new_unregistered_command', {})).toThrow(/не зарегистрирован контракт/);
  });

  it('rejects null where the UI expects an array', () => {
    expect(() => validateRustResponse('get_intake_capabilities', null)).toThrow(BackendContractError);
  });

  it('rejects primitives and malformed array items before React renders them', () => {
    expect(() => validateRustResponse('parse_source', 'bad')).toThrow(/объектом/);
    expect(() => validateRustResponse('get_component_statuses', [null])).toThrow(/ответ\[0\]/);
    expect(() => validateRustResponse('verify_rust_license_text', {})).toThrow(/логическим/);
    expect(() => validateRustResponse('export_one_c_counterparties', [])).toThrow(/строкой/);
  });

  it('enforces void and nullable-object contracts separately', () => {
    expect(validateRustResponse('save_state', null)).toBeNull();
    expect(() => validateRustResponse('save_state', {})).toThrow(/без результата/);
    expect(validateRustResponse('lookup_business_registry', null)).toBeNull();
    expect(validateRustResponse('pick_source_file', null)).toBeNull();
    expect(validateRustResponse('pick_source_file', { file_name: 'source.docx', selected_path: 'C:/source.docx' })).toMatchObject({ file_name: 'source.docx' });
    expect(() => validateRustResponse('lookup_business_registry', 'bad')).toThrow(/объектом/);
  });

  it('rejects a malformed workflow before React reads .length', () => {
    expect(() => validateRustResponse('get_workflow_plan', {
      document_id: 'x',
      prompts: null,
      blocked: false,
      block_reasons: [],
    })).toThrow(/prompts/);
  });

  it('rejects malformed nested arrays in high-risk UI responses', () => {
    expect(() => validateRustResponse('print_files', {
      queued_files: [],
      queued_copies: 0,
      failed_files: null,
    })).toThrow(/failed_files/);
    expect(() => validateRustResponse('test_semantic_model', {
      configured: true,
      reachable: true,
      available_models: null,
      message: 'ok',
    })).toThrow(/available_models/);
    expect(() => validateRustResponse('run_workspace_hygiene', {
      archived_processed_sources: [],
      archived_service_files: [],
      removed_orphan_markers: [],
      removed_expired_archived_files: [],
      warnings: null,
    })).toThrow(/warnings/);
  });

  it('normalizes a valid watcher payload and rejects malformed created_files', () => {
    expect(normalizeCreatedDocumentsIntakeResult({
      status: 'processed',
      patient_folder: 'out',
      created_files: ['a.docx'],
      missing: [],
      attention_file: null,
      message: 'ok',
    }).created_files).toEqual(['a.docx']);
    expect(() => normalizeCreatedDocumentsIntakeResult({
      status: 'processed',
      patient_folder: 'out',
      created_files: null,
      missing: [],
      attention_file: null,
      message: 'bad',
    })).toThrow(/created_files/);
  });
});
