import { afterEach, describe, expect, it } from 'vitest';
import {
  __resetInvokeForTests,
  __setInvokeForTests,
  analyzeTemplate,
  getWorkflowPlan,
  renderPreview,
  validateProductAccess,
  rustCommandNames
} from './api';

const sampleDocument = {
  id: 'template_1',
  button_label: 'Выписной эпикриз',
  template_path: 'x.docx',
  category: 'Medical',
  role_id: 'discharge',
  required_fields: ['medical.case_number'],
  placeholders: ['medical.case_number'],
  is_static_copy: false
};

describe('thin Tauri API', () => {
  afterEach(() => __resetInvokeForTests());

  it('routes template analysis to Rust command without local parser fallback', async () => {
    const calls: Array<{ command: string; payload?: Record<string, unknown> }> = [];
    __setInvokeForTests(async (command, payload) => {
      calls.push({ command, payload });
      return { document: sampleDocument, analysis_json: { from: 'rust' }, core_pipeline_json: { from: 'rust-core' } } as never;
    });

    const result = await analyzeTemplate('Выписной эпикриз\n{{medical.case_number}}', 'template_1', 'x.docx');
    expect(result.document.button_label).toBe('Выписной эпикриз');
    expect(calls[0].command).toBe('analyze_template');
    expect(JSON.stringify(calls[0].payload)).toContain('template_text');
  });

  it('routes workflow planning to Rust core', async () => {
    __setInvokeForTests(async (command) => {
      expect(command).toBe('get_workflow_plan');
      return { document_id: 'template_1', prompts: [], blocked: false, block_reasons: [] } as never;
    });
    await expect(getWorkflowPlan('template_1', true)).resolves.toMatchObject({ blocked: false });
  });

  it('routes rendering and license/product access through Rust commands', async () => {
    const seen: string[] = [];
    __setInvokeForTests(async (command) => {
      seen.push(command);
      if (command === 'validate_product_access') {
        return { accepted: true, mode: 'vip', plan: 'vip', reason: 'vip_code_accepted_locally', document_limit_month: 1000000, max_documents_per_run: 5000 } as never;
      }
      return { output_text: 'ok', missing_fields: [], unknown_fields: [], warnings: [] } as never;
    });
    await renderPreview('Документ {{field}}', true);
    const access = await validateProductAccess('000000');
    expect(access.accepted).toBe(true);
    expect(seen).toEqual(['render_preview', 'validate_product_access']);
  });

  it('documents the command surface exported by the Rust backend', () => {
    expect(rustCommandNames).toContain('validate_product_access');
    expect(rustCommandNames).toContain('install_background_watcher');
    expect(rustCommandNames).toContain('icd10_suggest');
    expect(rustCommandNames).toContain('save_state');
    expect(rustCommandNames).toContain('load_state');
    expect(rustCommandNames).toContain('prepare_template_setup');
    expect(rustCommandNames).toContain('confirm_template_setup');
    expect(rustCommandNames).toContain('get_diary_plan');
    expect(rustCommandNames).toContain('get_output_plan');
    expect(rustCommandNames).toContain('apply_scanner');
    expect(rustCommandNames).toContain('route_intake');
    expect(rustCommandNames).toContain('get_calibrated_threshold_status');
    expect(rustCommandNames).toContain('import_calibrated_thresholds');
  });
});
