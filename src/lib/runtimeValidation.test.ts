import { describe, expect, it } from 'vitest';
import { BackendContractError, normalizeCreatedDocumentsIntakeResult, validateRustResponse } from './runtimeValidation';

describe('runtime backend contracts', () => {
  it('rejects null where the UI expects an array', () => {
    expect(() => validateRustResponse('get_intake_capabilities', null)).toThrow(BackendContractError);
  });

  it('rejects a malformed workflow before React reads .length', () => {
    expect(() => validateRustResponse('get_workflow_plan', {
      document_id: 'x',
      prompts: null,
      blocked: false,
      block_reasons: [],
    })).toThrow(/prompts/);
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
