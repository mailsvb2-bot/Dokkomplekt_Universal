import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { App } from './App';
import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';
import { OUTPUT_NAMING_CONFIRMED_KEY, OUTPUT_ROOT_KEY } from './lib/appSupport';

type Call = { command: string; payload?: Record<string, unknown> };

const accDoc = { id: 'acc_1', button_label: 'Счёт на оплату', template_path: 'a.docx', category: 'Accounting', role_id: 'invoice', required_fields: [], placeholders: ['org.inn'], is_static_copy: false };
const secondDoc = { id: 'doc_2', button_label: 'Сопроводительное письмо', template_path: 'b.docx', category: 'Generic', role_id: 'generic', required_fields: [], placeholders: [], is_static_copy: false };
const pack = { pack_id: 'default', name: 'Набор', documents: [accDoc, secondDoc] };
const caseDto = { values: { 'org.inn': { field_id: 'org.inn', value: '7701234567', source: 'parser', confidence: 0.9 } } };
const workflow = { document_id: 'acc_1', prompts: [{ field_id: 'org.inn', title: 'ИНН', required: true, skippable: true, current_value: '7701234567', validation_hint: null }], blocked: false, block_reasons: [] };

function installMock(calls: Call[], options: { componentInstalled?: boolean; componentState?: 'downloaded' | 'bundled' | 'system' | 'missing'; bundleMode?: 'auto' | 'review' | 'none'; firstRunFailures?: number; renderFailureOnCall?: number } = {}) {
  const bundleMode = options.bundleMode ?? 'auto';
  let firstRunFailures = options.firstRunFailures ?? 0;
  let renderBatchCallCount = 0;
  const bundleDocumentIds = bundleMode === 'none' ? [] : bundleMode === 'review' ? ['acc_1'] : ['acc_1', 'doc_2'];
  const routing = { domain: 'Accounting', domain_confidence: 0.99, predicted_role: 'invoice', cluster_id: 'invoice', cluster_confidence: 0.99, recommended_document_ids: bundleDocumentIds, matches: [{ document_id: 'acc_1', button_label: 'Счёт на оплату', role_id: 'invoice', score: 0.99, evidence: ['title'] }], auto_select: bundleMode === 'auto', review_required: bundleMode !== 'auto', reasons: ['route'] };
  const bundleDecision = { document_ids: bundleDocumentIds, source: bundleMode === 'auto' ? 'deterministic_route' : bundleMode === 'review' ? 'review_proposal' : 'no_safe_proposal', confidence: 0.99, auto_apply: bundleMode === 'auto', review_required: bundleMode !== 'auto', question: bundleMode === 'auto' ? null : 'Подтвердите состав', reasons: ['route'] };
  let componentState = options.componentState ?? ((options.componentInstalled ?? true) ? 'downloaded' : 'missing');
  const componentInstalled = () => componentState === 'downloaded';
  const componentAvailable = () => componentState !== 'missing';
  let clauseBlocks: Array<{ block_id: string; title: string; content: string; updated_at: string }> = [];
  __setInvokeForTests(async (command, payload) => {
    calls.push({ command, payload });
    switch (command) {
      case 'first_run_state':
        if (firstRunFailures > 0) {
          firstRunFailures -= 1;
          throw new Error('state database unavailable');
        }
        return { pack, has_user_buttons: true, message: 'ok' } as never;
      case 'load_state':
        return { pack, has_user_buttons: true, message: 'ok' } as never;
      case 'parse_source':
        return { semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] }, routing, bundle_decision: bundleDecision } as never;
      case 'pick_source_file':
        return { file_name: 'Источник.docx', selected_path: 'C:/fixtures/Источник.docx' } as never;
      case 'parse_source_path':
      case 'parse_source_file':
        return { source_text: 'Счёт № 148', source_path: '/app-data/scanner-sources/source.docx', source_kind: 'docx', layout_items: [], semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] }, routing, bundle_decision: bundleDecision } as never;
      case 'get_intake_capabilities':
        return [{ format: 'PDF', extensions: ['pdf'], available: true, built_in: true, engine: 'pdftotext/OCR', details: 'готово' }] as never;
      case 'get_reference_data_status': return { installed: false, cached: false, restart_required: false, source: 'bundled', published_at: null, complete_years: [2025, 2026], listed_years: [2025, 2026, 2027], message: 'bundled' } as never;
      case 'update_reference_data': return { installed: true, cached: true, restart_required: false, source: 'signed-feed', published_at: '2026-07-18', complete_years: [2025, 2026, 2027], listed_years: [2025, 2026, 2027], message: 'updated' } as never;
      case 'import_reference_data': return { installed: true, cached: true, restart_required: false, source: 'imported', published_at: '2026-07-18', complete_years: [2025, 2026, 2027], listed_years: [2025, 2026, 2027], message: 'imported' } as never;
      case 'get_sidecar_status':
        return [
          { tool: 'tesseract', available: componentAvailable(), bundled: componentState === 'bundled', state: componentState, component_id: 'ocr', resolved_path: componentAvailable() ? 'tools/tesseract.exe' : null, purpose: 'OCR' },
          { tool: 'pdftotext', available: componentAvailable(), bundled: componentState === 'bundled', state: componentState, component_id: 'ocr', resolved_path: componentAvailable() ? 'tools/pdftotext.exe' : null, purpose: 'PDF text' },
          { tool: 'pdftoppm', available: componentAvailable(), bundled: componentState === 'bundled', state: componentState, component_id: 'ocr', resolved_path: componentAvailable() ? 'tools/pdftoppm.exe' : null, purpose: 'PDF images' },
          { tool: 'soffice', available: true, bundled: false, state: 'system', component_id: 'office', resolved_path: 'C:/Program Files/LibreOffice/program/soffice.exe', purpose: 'Office conversion' },
        ] as never;
      case 'get_component_statuses':
      case 'refresh_component_catalog':
        return [
          { id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42 * 1024 * 1024, size_label: '42 МБ', unlocks: ['tesseract'], state: componentState, installed: componentInstalled(), available: componentAvailable(), catalog_available: true, message: 'ok' },
          { id: 'office', label: 'Office', description: '', target: 'windows-x86_64', size_bytes: 210 * 1024 * 1024, size_label: '210 МБ', unlocks: ['soffice'], state: 'downloaded', installed: true, available: true, catalog_available: true, message: 'ok' },
          { id: 'semantic', label: 'Semantic', description: '', target: 'windows-x86_64', size_bytes: 980 * 1024 * 1024, size_label: '980 МБ', unlocks: ['llama_cpp'], state: 'downloaded', installed: true, available: true, catalog_available: true, message: 'ok' },
        ] as never;
      case 'install_component':
      case 'remove_component': {
        componentState = command === 'install_component' ? 'downloaded' : 'missing';
        return { id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42, size_label: '42 МБ', unlocks: ['tesseract'], state: componentState, installed: componentInstalled(), available: componentAvailable(), catalog_available: true, message: 'ok' } as never;
      }
      case 'pick_template_files':
        return { files: [
          { file_name: 'Договор.docx', template_path: '/app-data/user-templates/contract.docx', extracted_text: 'Договор\n{{org.inn}}' },
          { file_name: 'Акт.docx', template_path: '/app-data/user-templates/act.docx', extracted_text: 'Акт выполненных работ' },
        ] } as never;
      case 'pick_folder':
        return { selected_path: 'C:/Выбранная папка' } as never;
      case 'parse_web_source':
        return { source_text: 'Счёт № 148 из HTTPS', semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] }, final_url: 'https://example.com/doc', content_type: 'text/html', routing, bundle_decision: bundleDecision } as never;
      case 'get_document_template_text':
        return { template_text: 'СЧЁТ {{org.inn}}' } as never;
      case 'get_workflow_plan':
      case 'get_workflow_plan_batch':
        return workflow as never;
      case 'reset_case':
      case 'set_field':
        return caseDto as never;
      case 'apply_popup':
      case 'apply_popup_batch':
        return { accepted: true, semantic_case: caseDto, still_missing: [], message: 'ok' } as never;
      case 'render_preview':
        return { output_text: 'СЧЁТ-ПРЕВЬЮ', missing_fields: [], unknown_fields: [], warnings: [] } as never;
      case 'render_docx':
        return { output_text: 'ok', missing_fields: [], unknown_fields: [], warnings: [], output_path: 'output/acc_1.docx' } as never;
      case 'render_docx_batch': {
        renderBatchCallCount += 1;
        if (options.renderFailureOnCall === renderBatchCallCount) throw new Error('simulated render failure');
        return { output_folder: 'output/148_2026-02-01', created_files: ['output/148_2026-02-01/Счёт на оплату.docx'], created_documents: [{ document_id: 'acc_1', label: 'Счёт на оплату', path: 'output/148_2026-02-01/Счёт на оплату.docx' }] } as never;
      }
      case 'get_privacy_preferences': return { copy_source_to_output: true, write_trust_report: true, include_values_in_trust_report: false, temp_retention_hours: 24, archive_processed_sources: true, archive_folder_name: '_обработано', service_note_retention_days: 30, processed_marker_retention_days: 7, archived_source_retention_days: 0 } as never;
      case 'get_semantic_model_config': return { config: { enabled: false, provider: 'ollama', endpoint: 'http://127.0.0.1:11434', model: 'qwen2.5:7b-instruct', preferred_language: 'auto', timeout_seconds: 90, shadow_mode: true, corpus_recording_enabled: false, auto_apply_zero_touch: false, consistency_passes: 2 }, status: { configured: true, reachable: false, provider: 'ollama', endpoint: 'http://127.0.0.1:11434', model: 'qwen2.5:7b-instruct', available_models: [], message: 'disabled' } } as never;
      case 'get_printer_inventory': return { platform: 'windows', printers: [{ name: 'Office Printer', is_default: true, driver: 'driver', port: 'port' }], preferences: { printer_name: null, duplex_mode: 'simplex', tray: null }, advanced_options_note: 'ok' } as never;
      case 'update_privacy_preferences': return (payload as { req?: { preferences?: unknown } })?.req?.preferences as never;
      case 'list_automation_exceptions': return [{ exception_id: 'ex-1', category: 'risk_gate', source_path: 'C:/private/source.pdf', message: 'Нужно подтвердить дату', details_json: JSON.stringify({ blockers: [{ field_id: 'document.date', reason: 'confidence' }] }), status: 'open', created_at: '2026-07-16', updated_at: '2026-07-16' }] as never;
      case 'resolve_automation_exception': return true as never;
      case 'confirm_risk_exception_and_retry': return { status: 'processed', patient_folder: 'output', created_files: [], created_documents: [], missing: [], attention_file: null, message: 'confirmed' } as never;
      case 'run_workspace_hygiene': return { archived_processed_sources: ['C:/watch/_обработано/source.pdf'], archived_service_files: [], removed_orphan_markers: [], removed_expired_archived_files: [], warnings: [] } as never;
      case 'list_case_runs': return [] as never;
      case 'retry_case_run': return { status: 'processed', patient_folder: 'output', created_files: [], created_documents: [], missing: [], attention_file: null, message: 'ok' } as never;
      case 'get_queue_status': return { mode: 'shared_filesystem', configured: false, reachable: true, message: 'ok' } as never;
      case 'get_corpus_status': return { recording_enabled: false, entry_count: 1, privacy_mode: 'encrypted-hashed-no-raw-values', message: 'Корпус выключен' } as never;
      case 'get_quality_telemetry': return { generated_at: '2026-07-21', stop_reasons: [], unrecognized_fields: [], broken_templates: [], excluded_documents: [], repeated_confirmations: [], suggestions: [], privacy_mode: 'local_aggregate_only_no_document_text' } as never;
      case 'get_calibrated_threshold_status': return [] as never;
      case 'import_calibrated_thresholds': return { installed: true, domain: 'hr', generated_at: '2026-07-21T12:00:00Z', imported_at: '2026-07-21T12:01:00Z', corpus_sha256: 'a'.repeat(64), auto_min_confidence: 0.995, review_min_confidence: 0.9, max_auto_error_rate: 0.005, training_observations: 80, holdout_observations: 10, message: 'verified' } as never;
      case 'export_corpus': return { output_path: 'corpus.json', entry_count: 1, schema: 'dokkomplekt.ground-truth-corpus.v1' } as never;
      case 'get_automation_metrics': return { processed_sources: 3, generated_documents: 7, blocked_sources: 1, failed_sources: 0, print_failures: 0, user_confirmations: 1, zero_touch_sources: 2, attention_resolutions: 1, model_grounding_rejections: 0, shadow_model_runs: 2, shadow_model_proposals: 5, shadow_model_agreements: 4 } as never;
      case 'get_daily_automation_dashboard': return { date_utc: '2026-07-21', processed_cases: 3, automatically_completed_cases: 2, attention_cases: 1, failed_cases: 0, generated_documents: 7, printed_documents: 5, measured_processing_milliseconds: 12000 } as never;
      case 'list_audit_events': return [{ event_id: 'audit-1', event_type: 'source_processed', object_hash: 'obj', detail_json: '{}', previous_hash: '', event_hash: '1234567890abcdef', created_at: '2026-07-16' }] as never;
      case 'list_clause_blocks': return clauseBlocks as never;
      case 'save_clause_block': { const req=(payload as {req?:{block_id?:string;title?:string;content?:string}})?.req; clauseBlocks=[{block_id:req?.block_id||'requisites',title:req?.title||'Реквизиты',content:req?.content||'{{org.name}}',updated_at:'now'}]; return clauseBlocks as never; }
      case 'delete_clause_block': clauseBlocks=[]; return clauseBlocks as never;
      case 'suggest_template_markup_command': return [{ field_id:'org.inn', title:'ИНН', value:'7701234567', confidence:.99, occurrences:1, selected_by_default:true }] as never;
      case 'apply_template_markup_command': return { output_path:'tpl.marked.docx', replacement_count:1, replaced_occurrences:1, skipped_values:[] } as never;
      case 'preview_mail_merge': return { delimiter:';', headers:['subject.name'], canonical_headers:['subject.name'], rows:[['Иванов']], warnings:[] } as never;
      case 'prepare_mail_merge_file': return { delimited_text: 'subject.name\nИванов', table: { delimiter:'\t', headers:['subject.name'], canonical_headers:['subject.name'], rows:[['Иванов']], warnings:[] } } as never;
      case 'render_mail_merge': return { output_folder:'output/mm', row_count:1, created_files:['output/mm/doc.docx'] } as never;
      case 'icd10_suggest': return [{ code: 'A-101', title: 'Типовое значение' }] as never;
      case 'get_diary_plan':
        return [{ day_number: 1, date: '02.02.2026', month: 2, year: 2026 }] as never;
      case 'get_record_series_plan':
        return [{ sequence_number: 1, timestamp: '2026-02-02T09:00:00', date: '02.02.2026', time: '09:00' }] as never;
      case 'apply_scanner': { const req=(payload as {req?:{marks?:Array<{field_id:string}>}})?.req; const field=req?.marks?.[0]?.field_id ?? 'scanner.note'; return { applied_fields: [field], rejected_fields: [] } as never; }
      case 'start_word_scanner': { const req=(payload as {req?:{path?:string;mode?:'source'|'template';make_working_copy?:boolean}})?.req; return { session_id: `scan-${req?.mode ?? 'source'}`, mode: req?.mode ?? 'source', original_path: req?.path ?? 'source.docx', opened_path: req?.make_working_copy ? 'guided-copy.docx' : (req?.path ?? 'source.docx'), working_copy: Boolean(req?.make_working_copy), word_was_running: false, automation_available: true, message: 'opened' } as never; }
      case 'activate_word_scanner': return true as never;
      case 'capture_word_scanner': { const req=(payload as {req?:{session_id?:string;close_after_capture?:boolean}})?.req; return { session_id: req?.session_id ?? 'scan-source', selected_text: '148', context_text: 'Счёт № 148', before_text: 'Счёт № ', after_text: '', selection_start: 7, selection_end: 10, expanded_from_cursor: false, document_path: 'source.docx', document_closed: Boolean(req?.close_after_capture) } as never; }
      case 'apply_word_scanner_selection': return { session_id: 'scan-template', output_path: 'guided-copy.docx', selected_text: '148', placeholder: '{{accounting.invoice_number}}', extracted_text: 'Счёт № {{accounting.invoice_number}}', document_closed: true } as never;
      case 'close_word_scanner': return true as never;
      case 'save_learned_scanner_rule': return [] as never;
      case 'list_learned_scanner_rules': return [{ rule_id: 'rule-1', field_id: 'document.number', title: 'Номер документа', label_hint: 'Номер', before_text: '№ ', after_text: '', sample_value: '148', input_kind: 'text', created_at: '2026-08-01', learning_status: 'promoted', successful_applications: 3 }] as never;
      case 'delete_learned_scanner_rule': return [] as never;
      case 'list_template_approvals': return [{ document_id: 'acc_1', template_sha256: 'a'.repeat(64), jurisdiction: 'Российская Федерация', approved_by: 'Главный бухгалтер', approved_at: '2026-08-01', note: '' }] as never;
      case 'revoke_document_template_approval': return [] as never;
      case 'get_learned_kit_decision': return { document_ids: ['acc_1'], source: 'local_corpus', confidence: 0.97, auto_apply: true, reason: 'Устойчивое совпадение' } as never;
      case 'update_document_template': return pack as never;
      case 'list_template_versions': return [] as never;
      case 'rollback_template_version': return pack as never;
      case 'get_output_plan':
        return { root_folder: 'output', patient_folder: 'output/Готовые', files: ['output/Готовые/Счёт.docx'], warnings: [], exists: false } as never;
      case 'route_intake':
        return { should_start_ui: false, should_raise_existing_window: true, reason: 'raise existing window' } as never;
      case 'save_state':
        return undefined as never;
      case 'validate_product_access':
        return { accepted: true, mode: 'vip', plan: 'vip', reason: 'ok', watermark: null, document_limit_month: 1000, max_documents_per_run: 50 } as never;
      case 'verify_rust_license_text':
        return true as never;
      case 'check_for_updates':
        return { current_version: '18.0.7', latest_version: '18.0.8', update_available: true, artifact_path: '/app-data/verified-updates/18.0.8/app.zip', release_notes: 'Security update' } as never;
      case 'install_background_watcher':
        return { platform: 'windows', installed: true, watch_folder: 'watch', commands: [], warnings: [] } as never;
      case 'update_background_watcher_preferences':
        return true as never;
      case 'uninstall_background_watcher':
        return { platform: 'windows', installed: false, removed_files: [], warnings: [] } as never;
      case 'import_template_file':
        return { template_path: '/app-data/user-templates/tpl.docx', extracted_text: 'Договор\n{{org.inn}}' } as never;
      case 'print_files':
        return { queued_files: ['C:/Созданные документы/Иванов/Договор.docx'], queued_copies: 3, failed_files: [] } as never;
      case 'update_print_preferences': return { platform: 'windows', printers: [], preferences: (payload as { req?: { preferences?: unknown } })?.req?.preferences, advanced_options_note: 'ok' } as never;
      case 'update_semantic_model_config': return { config: (payload as { req?: { config?: unknown } })?.req?.config, status: { configured: true, reachable: false, provider: 'ollama', endpoint: 'http://127.0.0.1:11434', model: 'qwen2.5:7b-instruct', available_models: [], message: 'saved' } } as never;
      case 'test_semantic_model': return { configured: true, reachable: true, provider: 'ollama', endpoint: 'http://127.0.0.1:11434', model: 'qwen2.5:7b-instruct', available_models: ['qwen2.5:7b-instruct'], message: 'ok' } as never;
      case 'export_files_to_pdf': return { created_files: ['output/doc.pdf'], failed_files: [], pdfa_1_requested: false, conformance_note: 'ok' } as never;
      case 'create_kedo_package': return { package_folder: 'output/KEDO', manifest_path: 'output/KEDO/kedo-manifest.xml', checksum_path: 'output/KEDO/SHA256SUMS.txt', documents: [], conformance_note: 'ok' } as never;
      case 'open_in_file_manager':
        return undefined as never;
      case 'run_created_documents_intake':
        return { status: 'processed', patient_folder: 'C:/Созданные документы/Иванов', created_files: ['C:/Созданные документы/Иванов/Договор.docx'], created_documents: [{ document_id: 'acc_1', label: 'Договор', path: 'C:/Созданные документы/Иванов/Договор.docx' }], missing: [], attention_file: null, message: 'Комплект создан.' } as never;
      case 'semantic_extract':
        return { fields: [{ field_id: 'org.inn', value: '7736050003', confidence: 0.9, method: 'typed:inn' }], warnings: [], model_applied: false, prompt: 'PROMPT' } as never;
      case 'analyze_template':
      case 'analyze_template_file':
        return { document: { ...accDoc, placeholders: ['org.inn', 'org.name'] }, analysis_json: {}, core_pipeline_json: {} } as never;
      case 'prepare_template_setup': {
        const candidates = (payload as { req?: { candidates?: Array<{ document_id: string; template_path: string; preferred_button_label?: string }> } })?.req?.candidates ?? [];
        return candidates.map((candidate) => ({
          document_id: candidate.document_id,
          template_path: candidate.template_path,
          detected_title: candidate.preferred_button_label ?? 'Документ',
          suggested_button_label: candidate.preferred_button_label ?? 'Документ',
          editable_button_label: candidate.preferred_button_label ?? 'Документ',
          role_id: 'generic',
          is_static_copy: false,
          analysis: {},
          popup_fields: [],
        })) as never;
      }
      case 'confirm_template_setup':
        return { pack_id: 'default', name: 'Пакет', documents: [{ ...accDoc, id: 'tpl', button_label: 'Договор' }] } as never;
      case 'rename_document_button':
        return { pack_id: 'default', name: 'Пакет', documents: [{ ...accDoc, id: 'tpl', button_label: 'Договор новый' }] } as never;
      case 'remove_document_button':
        return { pack_id: 'default', name: 'Пакет', documents: [] } as never;
      case 'update_document_popup_fields':
        return pack as never;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
}

async function click(name: RegExp | string) {
  fireEvent.click(await screen.findByRole('button', { name }));
}

describe('Полный прогон пользовательских сценариев и тем', () => {
  beforeEach(() => {
    localStorage.setItem(OUTPUT_ROOT_KEY, 'C:/Test/Готовые документы');
    localStorage.setItem(OUTPUT_NAMING_CONFIRMED_KEY, 'true');
  });
  afterEach(() => { localStorage.clear(); __resetInvokeForTests(); vi.restoreAllMocks(); });

  it('каждый пользовательский сценарий вызывает соответствующую Rust-команду', async () => {
    const calls: Call[] = [];
    installMock(calls);
    render(<App />);

    // first_run_state populates a profession-neutral document set
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    expect(screen.getByRole('button', { name: 'Сопроводительное письмо' })).toBeTruthy();
    expect(screen.queryByText('Медицина')).toBeNull();

    // A new set must explicitly clear case-specific values before another person/contract.
    await click(/Новый комплект/);
    await waitFor(() => expect(calls.some((c) => c.command === 'reset_case')).toBe(true));

    // Document configuration stays out of the daily flow but remains available on demand.
    fireEvent.click(screen.getByText('Управление кнопками'));
    fireEvent.click(screen.getByText('Количество экземпляров'));
    // Each document keeps its own print-copy count (including 0 = do not print).
    fireEvent.change(screen.getByLabelText('Количество копий для Счёт на оплату'), { target: { value: '3' } });
    fireEvent.change(screen.getByLabelText('Количество копий для Сопроводительное письмо'), { target: { value: '10' } });
    expect(JSON.parse(localStorage.getItem('dokkomplekt.print-copies.v1') || '{}')).toMatchObject({ acc_1: 3, doc_2: 10 });

    // parse source text through the alternative-source path
    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Счёт № 148' } });
    await click(/Использовать текст/);
    await waitFor(() => expect(calls.some((c) => c.command === 'parse_source')).toBe(true));
    expect(parsePayload(calls, 'parse_source')).toMatchObject({ req: { default_year: expect.any(Number) } });

    // Packaged desktop button uses the native OS picker and then parses the selected path.
    fireEvent.click(screen.getByRole('button', { name: 'Заменить исходный файл' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'pick_source_file')).toBe(true));
    await waitFor(() => expect(parsePayload(calls, 'parse_source_path')).toMatchObject({ req: { selected_path: 'C:/fixtures/Источник.docx', default_year: expect.any(Number) } }));

    // Drag-and-drop remains a supported independent byte-upload path -> parse_source_file.
    const sourceFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Источник.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    const sourceDropZone = document.querySelector('.sourceStage');
    expect(sourceDropZone).toBeTruthy();
    fireEvent.drop(sourceDropZone as Element, { dataTransfer: { files: [sourceFile] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'parse_source_file')).toBe(true));

    // Guided Word scanner: the program opens the document, reads the mouse selection,
    // suggests the semantic destination, remembers it and closes Word itself.
    fireEvent.click(screen.getByText('Расширенные инструменты'));
    await click(/Показать значение в Word/);
    const guidedSource = await screen.findByRole('dialog', { name: 'Простой сканер мышью' });
    fireEvent.click(within(guidedSource).getByRole('button', { name: /Word не видно/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'activate_word_scanner')).toBe(true));
    fireEvent.click(within(guidedSource).getByRole('button', { name: /Я показал значение/ }));
    await waitFor(() => expect(within(guidedSource).getAllByText(/Номер счёта/).length).toBeGreaterThan(0));
    fireEvent.click(within(guidedSource).getByRole('button', { name: /Выделить другое/ }));
    await within(guidedSource).findByRole('button', { name: /Я показал значение/ });
    fireEvent.click(within(guidedSource).getByRole('button', { name: /Я показал значение/ }));
    await waitFor(() => expect(within(guidedSource).getAllByText(/Номер счёта/).length).toBeGreaterThan(0));
    fireEvent.click(within(guidedSource).getByRole('button', { name: /Да, всё правильно/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'save_learned_scanner_rule')).toBe(true));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Простой сканер мышью' })).toBeNull());

    // Cancellation also closes the document automatically.
    await click(/Показать значение в Word/);
    const guidedCancel = await screen.findByRole('dialog', { name: 'Простой сканер мышью' });
    fireEvent.click(within(guidedCancel).getByRole('button', { name: /Отмена — всё закрыть/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'close_word_scanner')).toBe(true));

    // Manual markup remains available inside advanced tools.
    fireEvent.change(screen.getByPlaceholderText('Идентификатор поля'), { target: { value: 'document.number' } });
    fireEvent.change(screen.getByPlaceholderText('Выделенный текст'), { target: { value: '148' } });
    await click(/Назначить выделение полю/);
    await waitFor(() => expect(calls.some((call) => {
      if (call.command !== 'apply_scanner') return false;
      const request = (call.payload as { req?: { marks?: Array<{ field_id?: string; selected_text?: string }> } } | undefined)?.req;
      return request?.marks?.some((mark) => mark.field_id === 'document.number' && mark.selected_text === '148') ?? false;
    })).toBe(true));

    // select document -> workflow plan + actual template text, fields render
    fireEvent.click(screen.getByRole('button', { name: 'Счёт на оплату' }));
    fireEvent.click(screen.getByRole('button', { name: 'Выбрать всё' }));
    await screen.findByDisplayValue('7701234567');
    await waitFor(() => expect(calls.some((c) => c.command === 'get_document_template_text')).toBe(true));

    // Existing template is marked through the same guided Word workflow.
    await click(/Разметить шаблон/);
    const guidedTemplate = await screen.findByRole('dialog', { name: 'Простой сканер мышью' });
    fireEvent.click(within(guidedTemplate).getByRole('button', { name: /Я показал значение/ }));
    await waitFor(() => expect(within(guidedTemplate).getAllByText(/Номер счёта/).length).toBeGreaterThan(0));
    fireEvent.click(within(guidedTemplate).getByRole('button', { name: /Да, всё правильно/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'apply_word_scanner_selection')).toBe(true));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_document_template')).toBe(true));

    // pin field -> set_field
    fireEvent.click(screen.getByRole('button', { name: 'Использовать ИНН во всех документах' }));
    await waitFor(() => expect(parsePayload(calls, 'set_field')).toMatchObject({ req: { field_id: 'org.inn', value: '7701234567' } }));

    // Creation is deliberately two-step: the workspace action opens a blocking preflight,
    // and no Rust apply/render call happens until the user confirms that modal.
    const batchApplyCountBefore = calls.filter((call) => call.command === 'apply_popup_batch').length;
    await click(/Проверить и создать \(2\)/);
    const batchPreflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    expect(calls.filter((call) => call.command === 'apply_popup_batch')).toHaveLength(batchApplyCountBefore);
    fireEvent.click(within(batchPreflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: { document_ids: ['acc_1', 'doc_2'], folder_parts: ['DocumentNumber', 'DocumentDate'], answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
    await waitFor(() => expect(parsePayload(calls, 'render_docx_batch')).toMatchObject({
      req: { document_ids: ['acc_1', 'doc_2'], output_root: expect.any(String), folder_parts: ['DocumentNumber', 'DocumentDate'], strict: true },
    }));
    await waitFor(() => expect(calls.some((call) => call.command === 'open_in_file_manager'
      && (call.payload as { req?: { path?: string } } | undefined)?.req?.path === 'output/148_2026-02-01')).toBe(true));

    // specialist can configure the document-specific popup without changing the template.
    await click(/Настроить уточнения/);
    const popupDesigner = await screen.findByRole('dialog', { name: 'Конструктор уточняющих вопросов' });
    fireEvent.click(within(popupDesigner).getByRole('button', { name: /Сохранить вопросы/ }));
    await waitFor(() => expect(parsePayload(calls, 'update_document_popup_fields')).toMatchObject({ req: { document_id: 'acc_1' } }));

    // Preview is explicitly bound to the opened document and never starts a second generation flow.
    await click(/Предпросмотр «Счёт на оплату»/);
    await screen.findByText('СЧЁТ-ПРЕВЬЮ');
    expect(screen.getByText(/Предпросмотр: Счёт на оплату/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Создать только этот документ/ })).toBeNull();

    // A one-document package uses the same output plan and batch renderer.
    fireEvent.click(screen.getByRole('button', { name: 'Снять выбор' }));
    fireEvent.click(screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }));
    await waitFor(() => expect(screen.getByRole('button', { name: /Проверить и создать \(1\)/ })).toBeTruthy());
    await click(/Проверить и создать \(1\)/);
    const singlePreflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(singlePreflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup')).toMatchObject({
      req: { document_id: 'acc_1', folder_parts: ['DocumentNumber', 'DocumentDate'], answers: [{ field_id: 'org.inn', value: '7701234567' }] },
    }));
    const batchCalls = calls.filter((call) => call.command === 'render_docx_batch');
    expect(batchCalls.at(-1)?.payload).toMatchObject({
      req: { document_ids: ['acc_1'], output_root: expect.any(String), folder_parts: ['DocumentNumber', 'DocumentDate'], strict: true },
    });

    // Profile-specific dictionaries are tested separately and appear only when a template requests them.

    // utility scenarios use real user inputs, not demo constants
    await click(/^Настройки$/);
    fireEvent.click(screen.getByText('Экспертные и административные инструменты'));
    await screen.findByText('Конфиденциальность и хранение');
    const semanticCard = screen.getByText('Локальное понимание документов').closest('.utilityCard');
    expect(semanticCard).toBeTruthy();
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('checkbox', { name: /включить локальное понимание/ }));
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: 'Сохранить' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_semantic_model_config')).toBe(true));
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: /Проверить соединение/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'test_semantic_model')).toBe(true));
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: /Экспортировать историю проверок/ }));
    const exportDialog = await screen.findByRole('dialog', { name: 'Экспорт истории проверок' });
    fireEvent.change(within(exportDialog).getByLabelText('Имя файла *'), { target: { value: 'corpus-test.json' } });
    fireEvent.click(within(exportDialog).getByRole('button', { name: 'Экспортировать' }));
    await waitFor(() => expect(parsePayload(calls, 'export_corpus')).toMatchObject({ req: { output_path: 'corpus-test.json' } }));
    const thresholdCard = screen.getByText('Безопасная автопечать').closest('.utilityCard');
    expect(thresholdCard).toBeTruthy();
    const thresholdInput = within(thresholdCard as HTMLElement).getByLabelText('Импортировать подписанные пороги автопечати').querySelector('input[type=file]');
    expect(thresholdInput).toBeTruthy();
    fireEvent.change(thresholdInput as Element, { target: { files: [new File(['{}'], 'thresholds.signed.json', { type: 'application/json' })] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'import_calibrated_thresholds')).toBe(true));
    const calendarCard = screen.getByText('Производственный календарь').closest('.utilityCard');
    expect(calendarCard).toBeTruthy();
    fireEvent.click(within(calendarCard as HTMLElement).getByRole('button', { name: /Проверить подписанное обновление/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_reference_data')).toBe(true));
    const calendarInput = within(calendarCard as HTMLElement).getByLabelText('Импортировать подписанный календарь').querySelector('input[type=file]');
    expect(calendarInput).toBeTruthy();
    fireEvent.change(calendarInput as Element, { target: { files: [new File(['{}'], 'calendar.signed.json', { type: 'application/json' })] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'import_reference_data')).toBe(true));
    const printerCard = screen.getByText('Принтер и параметры вывода').closest('.utilityCard');
    expect(printerCard).toBeTruthy();
    fireEvent.click(within(printerCard as HTMLElement).getByRole('button', { name: /Сохранить печать/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_print_preferences')).toBe(true));
    await click(/Сохранить политику/);
    await click(/Очистить сейчас/);
    await waitFor(() => expect(calls.some((c) => c.command === 'run_workspace_hygiene')).toBe(true));
    const exceptionCard = screen.getByText(/Нужно подтвердить дату/).closest('.exceptionItem');
    expect(exceptionCard).toBeTruthy();
    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: 'Закрыть' }));
    const resolveDialog = await screen.findByRole('dialog', { name: 'Закрыть исключение' });
    fireEvent.change(within(resolveDialog).getByLabelText('Что исправлено или подтверждено? *'), { target: { value: 'Дата подтверждена по оригиналу' } });
    fireEvent.click(within(resolveDialog).getByRole('button', { name: 'Закрыть исключение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'resolve_automation_exception')).toBe(true));
    const refreshedExceptionCard = (await screen.findByText(/Нужно подтвердить дату/)).closest('.exceptionItem');
    expect(refreshedExceptionCard).toBeTruthy();
    fireEvent.click(within(refreshedExceptionCard as HTMLElement).getByRole('button', { name: /Подтвердить всё/ }));
    const riskDialog = await screen.findByRole('dialog', { name: 'Подтвердить спорные значения?' });
    fireEvent.click(within(riskDialog).getByRole('button', { name: 'Подтвердить и повторить' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'confirm_risk_exception_and_retry')).toBe(true));
    fireEvent.change(screen.getByPlaceholderText('дата начала'), { target: { value: '01.02.2026' } });
    fireEvent.change(screen.getByPlaceholderText('дата окончания'), { target: { value: '03.02.2026' } });
    await click(/Рассчитать/);
    fireEvent.change(screen.getByPlaceholderText(/поле, например/), { target: { value: 'document.number' } });
    fireEvent.change(screen.getByPlaceholderText('выделенный текст'), { target: { value: '148' } });
    await click(/Применить разметку/);
    fireEvent.change(screen.getByLabelText('Папка готовых документов'), { target: { value: 'C:/output' } });
    await click(/Проверить папку/);
    await click(/Сохранить сессию/);
    await click(/Загрузить сессию/);
    await click(/Проверить доступ/);
    await click(/Проверить обновления/);
    await waitFor(() => expect(calls.some((c) => c.command === 'check_for_updates')).toBe(true));
    await click(/Включить фоновый агент/);
    await click(/Отключить фоновый агент/);
    fireEvent.change(screen.getByPlaceholderText(/подписанную лицензию/), { target: { value: 'LIC-123' } });
    await click(/Активировать лицензию/);
    await waitFor(() => expect(parsePayload(calls, 'verify_rust_license_text')).toMatchObject({ req: { license_text: 'LIC-123' } }));
    expect((parsePayload(calls, 'verify_rust_license_text') as { req?: Record<string, unknown> })?.req).not.toHaveProperty('public_key_b64');

    const governance = screen.getByText('Обучение и подтверждения').closest('.governanceCard');
    expect(governance).toBeTruthy();
    fireEvent.click(within(governance as HTMLElement).getByText('Обучение и подтверждения'));
    fireEvent.click(await within(governance as HTMLElement).findByRole('button', { name: 'Удалить правило' }));
    const deleteRuleDialog = await screen.findByRole('dialog', { name: 'Удалить обученное правило?' });
    fireEvent.click(within(deleteRuleDialog).getByRole('button', { name: 'Удалить правило' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'delete_learned_scanner_rule')).toBe(true));
    fireEvent.change(within(governance as HTMLElement).getByLabelText('Идентификатор кластера'), { target: { value: 'invoice-cluster' } });
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Показать решение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'get_learned_kit_decision')).toBe(true));
    fireEvent.click(within(governance as HTMLElement).getByRole('button', { name: 'Отозвать подтверждение' }));
    const revokeApprovalDialog = await screen.findByRole('dialog', { name: 'Отозвать подтверждение?' });
    fireEvent.click(within(revokeApprovalDialog).getByRole('button', { name: 'Отозвать подтверждение' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'revoke_document_template_approval')).toBe(true));

    fireEvent.change(screen.getByPlaceholderText('идентификатор блока'), { target: { value: 'requisites' } });
    fireEvent.change(screen.getByPlaceholderText('название'), { target: { value: 'Реквизиты' } });
    fireEvent.change(screen.getByPlaceholderText('текст блока с условиями и полями'), { target: { value: '{{org.name}}' } });
    await click(/Сохранить блок/); await click('Удалить requisites');
    const wizardFile=new File([new Uint8Array([0x50,0x4b,0x03,0x04])],'Реквизиты.docx',{type:'application/vnd.openxmlformats-officedocument.wordprocessingml.document'});
    const wizardInput=screen.getByText('Выбрать DOCX/DOCM').querySelector('input[type=file]'); expect(wizardInput).toBeTruthy();
    fireEvent.change(wizardInput as Element,{target:{files:[wizardFile]}}); await waitFor(()=>expect(calls.some(c=>c.command==='suggest_template_markup_command')).toBe(true)); await click(/Создать размеченную копию/); await click(/Сухой прогон без печати/); await screen.findByText(/^✓ Роль:/);
    const xlsxInput = screen.getByText('Загрузить XLSX/CSV/TSV').querySelector('input[type=file]'); expect(xlsxInput).toBeTruthy();
    fireEvent.change(xlsxInput as Element, { target: { files: [new File([new Uint8Array([0x50,0x4b,0x03,0x04])], 'Реестр.xlsx')] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'prepare_mail_merge_file')).toBe(true));
    fireEvent.change(screen.getByPlaceholderText(/Наименование;document\.number/),{target:{value:'subject.name;contract.number\nИванов;Д-1'}}); await click(/^Проверить$/); await click(/Создать комплекты/);

    // Native first-run/add-template flow: the visible action opens the OS picker,
    // then the selected DOCX files are analysed and presented for confirmation.
    const priorTemplateAnalyses = calls.filter((call) => call.command === 'analyze_template_file').length;
    await click(/Добавить шаблоны/);
    const dialog = await screen.findByRole('dialog', { name: 'Добавление шаблонов' });
    await waitFor(() => expect(calls.some((c) => c.command === 'pick_template_files')).toBe(true));
    await waitFor(() => expect(calls.filter((c) => c.command === 'analyze_template_file')).toHaveLength(priorTemplateAnalyses + 2));
    fireEvent.click(await within(dialog).findByRole('button', { name: 'Создать кнопки (2)' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Добавление шаблонов' })).toBeNull());
    await screen.findByRole('button', { name: 'Договор' });
    expect(parsePayload(calls, 'prepare_template_setup')).toMatchObject({ req: { candidates: [
      { template_path: '/app-data/user-templates/contract.docx' },
      { template_path: '/app-data/user-templates/act.docx' },
    ] } });

    // Text fallback remains explicitly reachable without cancelling the native picker.
    await click(/Создать из текста/);
    const manualDialog = await screen.findByRole('dialog', { name: 'Добавление шаблонов' });
    fireEvent.change(within(manualDialog).getByPlaceholderText('Вставьте текст документа'), { target: { value: 'Договор № {{document.number}}' } });
    fireEvent.click(within(manualDialog).getByRole('button', { name: 'Проверить шаблон' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'analyze_template')).toBe(true));
    fireEvent.click(within(manualDialog).getByRole('button', { name: 'Отмена' }));

    // HTTPS/site/API intake -> parse_web_source
    fireEvent.change(screen.getByLabelText('Адрес источника'), { target: { value: 'https://example.com/doc' } });
    await click(/^Загрузить$/);
    await waitFor(() => expect(parsePayload(calls, 'parse_web_source')).toMatchObject({ req: { url: 'https://example.com/doc' } }));

    // Folder automation remains available without cluttering the primary workflow.
    fireEvent.click(screen.getByText('Автоматическая обработка папки'));
    const automation = screen.getByText('Автоматическая обработка папки').closest('.automationCard');
    expect(automation).toBeTruthy();
    fireEvent.click(within(automation as HTMLElement).getByRole('button', { name: 'Выбрать' }));
    await waitFor(() => expect(parsePayload(calls, 'pick_folder')).toMatchObject({ req: { initial_path: expect.any(String) } }));
    fireEvent.change(within(automation as HTMLElement).getByPlaceholderText('Путь к файлу'), { target: { value: 'C:/Созданные документы/Источник.docx' } });
    fireEvent.click(within(automation as HTMLElement).getByRole('button', { name: 'Создать комплект' }));
    await waitFor(() => expect(parsePayload(calls, 'run_created_documents_intake')).toMatchObject({ req: { source_path: 'C:/Созданные документы/Источник.docx', output_root: expect.any(String), folder_parts: ['DocumentNumber', 'DocumentDate'] } }));
    await screen.findByRole('heading', { name: /Создано документов:/ });
    await click(/Открыть папку с документами/);
    fireEvent.click(screen.getByLabelText('Дополнительные форматы'));
    await click(/^Создать PDF$/);
    await waitFor(() => expect(calls.some((c) => c.command === 'export_files_to_pdf')).toBe(true));
    await click(/Создать пакет обмена/);
    await waitFor(() => expect(calls.some((c) => c.command === 'create_kedo_package')).toBe(true));
    await click(/^Печать$/);
    await waitFor(() => expect(calls.some((call) => call.command === 'open_in_file_manager'
      && (call.payload as { req?: { path?: string } } | undefined)?.req?.path === 'C:/Созданные документы/Иванов')).toBe(true));
    await waitFor(() => expect(parsePayload(calls, 'print_files')).toMatchObject({ req: { jobs: [{ path: 'C:/Созданные документы/Иванов/Договор.docx', copies: 3 }] } }));

    // Explicit recognition refresh remains available in advanced tools.
    await click(/Обновить распознанные данные/);
    await waitFor(() => expect(parsePayload(calls, 'semantic_extract')).toMatchObject({ req: { source_text: expect.any(String), default_year: expect.any(Number) } }));

    // button management preserves the template while changing only the registry
    await click(/Переименовать/);
    const renameDialog = await screen.findByRole('dialog', { name: 'Переименовать документ' });
    fireEvent.change(within(renameDialog).getByLabelText('Новое название кнопки *'), { target: { value: 'Договор новый' } });
    fireEvent.click(within(renameDialog).getByRole('button', { name: 'Переименовать' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'rename_document_button')).toBe(true));
    await click(/Убрать из набора/);
    const removeDialog = await screen.findByRole('dialog', { name: 'Убрать документ из набора?' });
    fireEvent.click(within(removeDialog).getByRole('button', { name: 'Убрать из набора' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'remove_document_button')).toBe(true));

    // theme: preset B applies dark bg + persists
    fireEvent.click(screen.getByRole('button', { name: 'Тема оформления' }));
    fireEvent.click(await screen.findByRole('button', { name: /Тёмный сфокусированный/ }));
    await waitFor(() => expect(getVar('--bg')).toBe('#0B0F14'));
    expect(JSON.parse(localStorage.getItem('dokkomplekt.theme.v1') || '{}').preset).toBe('B');

    // theme: custom accent colour applies + persists
    fireEvent.change(screen.getByLabelText('Акцент'), { target: { value: '#ff0000' } });
    await waitFor(() => expect(getVar('--accent')).toBe('#ff0000'));
    expect(JSON.parse(localStorage.getItem('dokkomplekt.theme.v1') || '{}')).toMatchObject({ preset: 'custom', accent: '#ff0000' });

    // Every user-facing command is reached. Profile-only legacy diary planning
    // and focused approval/registry flows remain covered by dedicated tests, not fake clicks in this already broad scenario.
    const reached = new Set(calls.map((c) => c.command));
    const internalOrProfileOnly = new Set(['icd10_suggest', 'get_default_output_root', 'ensure_output_root', 'get_diary_plan', 'route_intake', 'retry_case_run', 'rollback_template_version', 'install_component', 'refresh_component_catalog', 'remove_component', 'get_print_triage', 'approve_document_template', 'import_business_registry', 'lookup_business_registry', 'apply_business_registry_record', 'export_one_c_counterparties', 'import_learning_example_file', 'replace_clause_blocks', 'learn_template_from_examples_command', 'apply_template_learning_map', 'register_learned_template', 'check_template_regression', 'confirm_bundle_exception_and_retry', 'upsert_organization_knowledge', 'delete_organization_knowledge', 'apply_organization_knowledge', 'select_process_blueprint', 'render_docx']);
    const expected = rustCommandNames.filter((command) => !internalOrProfileOnly.has(command));
    expect([...reached].sort()).toEqual([...expected].sort());
  }, 20_000);

  it('fail-closed блокирует работу при ошибке чтения сохранённого набора и даёт безопасный повтор', async () => {
    const calls: Call[] = [];
    installMock(calls, { firstRunFailures: 1 });
    render(<App />);

    const alert = await screen.findByRole('alert', { name: 'Не удалось загрузить рабочий набор' });
    expect(within(alert).getByText('Рабочий набор не загружен')).toBeTruthy();
    const createButtons = screen.getByRole('button', { name: 'Создать свои кнопки' }) as HTMLButtonElement;
    expect(createButtons.disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Выбрать исходный файл' }) as HTMLButtonElement).disabled).toBe(true);

    const retry = within(alert).getByRole('button', { name: 'Повторить загрузку' }) as HTMLButtonElement;
    await waitFor(() => expect(retry.disabled).toBe(false));
    fireEvent.click(retry);
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    await waitFor(() => expect(screen.queryByRole('alert', { name: 'Не удалось загрузить рабочий набор' })).toBeNull());
    expect((screen.getByRole('button', { name: 'Выбрать исходный файл' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('замена источника и новый комплект не оставляют имя, сканер или готовые файлы предыдущего дела', async () => {
    const calls: Call[] = [];
    installMock(calls);
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    const oldSource = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Старый пациент.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    fireEvent.drop(document.querySelector('.sourceStage') as Element, { dataTransfer: { files: [oldSource] } });
    await screen.findByText('Старый пациент.docx');

    await click(/Проверить и создать \(2\)/);
    let preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await screen.findByRole('status', { name: 'Комплект готов' });

    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Новый пациент, счёт № 148' } });
    await click(/Использовать текст/);
    await waitFor(() => expect(screen.queryByText('Старый пациент.docx')).toBeNull());
    expect(screen.queryByRole('status', { name: 'Комплект готов' })).toBeNull();

    fireEvent.click(screen.getByText('Расширенные инструменты'));
    expect((screen.getByRole('button', { name: 'Показать значение в Word' }) as HTMLButtonElement).disabled).toBe(true);

    await click(/Проверить и создать \(2\)/);
    preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await screen.findByRole('status', { name: 'Комплект готов' });
    await click(/Новый комплект/);
    await waitFor(() => expect(calls.filter((call) => call.command === 'reset_case').length).toBeGreaterThan(0));
    expect(screen.queryByRole('status', { name: 'Комплект готов' })).toBeNull();
    expect(screen.getByRole('heading', { name: 'Добавьте исходный файл' })).toBeTruthy();
  });

  it('новая подтверждённая генерация убирает старую зелёную карточку, если текущий render падает', async () => {
    const calls: Call[] = [];
    installMock(calls, { renderFailureOnCall: 2 });
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Счёт № 148' } });
    await click(/Использовать текст/);

    await click(/Проверить и создать \(2\)/);
    let preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await screen.findByRole('status', { name: 'Комплект готов' });

    await click(/Проверить и создать \(2\)/);
    preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    expect(screen.getByRole('status', { name: 'Комплект готов' })).toBeTruthy();

    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await within(preflight).findByText(/simulated render failure/);
    expect(screen.queryByRole('status', { name: 'Комплект готов' })).toBeNull();
    expect(within(preflight).getByText('Документы не созданы')).toBeTruthy();
    expect(calls.filter((call) => call.command === 'render_docx_batch')).toHaveLength(2);
  });

  it('явное продолжение без обязательного значения передаётся в Rust и не блокирует генерацию', async () => {
    const calls: Call[] = [];
    installMock(calls);
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Счёт № 148' } });
    await click(/Использовать текст/);
    await screen.findByDisplayValue('7701234567');

    fireEvent.click(screen.getByRole('button', { name: 'Продолжить без этого значения' }));
    expect((screen.getByDisplayValue('7701234567') as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByRole('button', { name: 'Вернуться к заполнению' }).getAttribute('aria-pressed')).toBe('true');

    await click(/Проверить и создать \(2\)/);
    const preflight = await screen.findByRole('dialog', { name: 'Проверка перед созданием' });
    expect(within(preflight).getByRole('button', { name: 'Вернуться к заполнению' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(within(preflight).getByRole('button', { name: 'Создать документы' }));
    await waitFor(() => expect(parsePayload(calls, 'apply_popup_batch')).toMatchObject({
      req: {
        document_ids: ['acc_1', 'doc_2'],
        answers: [{ field_id: 'org.inn', value: '', continue_without_value: true }],
      },
    }));
    await waitFor(() => expect(calls.some((call) => call.command === 'render_docx_batch')).toBe(true));
  });

  it('новый источник заменяет старый комплект точным review-предложением без автогенерации', async () => {
    const calls: Call[] = [];
    installMock(calls, { bundleMode: 'review' });
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    // Simulate an explicit selection left from the previous case.
    fireEvent.click(screen.getByRole('button', { name: 'Выбрать всё' }));
    expect((screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole('checkbox', { name: 'Добавить Сопроводительное письмо в комплект' }) as HTMLInputElement).checked).toBe(true);

    fireEvent.click(screen.getByText('Другой способ добавить источник'));
    fireEvent.change(screen.getByPlaceholderText('Вставьте текст источника'), { target: { value: 'Новый неоднозначный источник' } });
    await click(/Использовать текст/);

    // Rust proposes only acc_1. The old doc_2 selection must not survive.
    await waitFor(() => expect((screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }) as HTMLInputElement).checked).toBe(true));
    expect((screen.getByRole('checkbox', { name: 'Добавить Сопроводительное письмо в комплект' }) as HTMLInputElement).checked).toBe(false);
    expect(screen.getByRole('button', { name: /Проверить и создать \(1\)/ })).toBeTruthy();
    expect(screen.getByText(/Предложен комплект: Счёт на оплату/)).toBeTruthy();
    expect(calls.some((call) => call.command === 'render_docx_batch')).toBe(false);
  });

  it('не предлагает загрузку, когда OCR уже найден в системе', async () => {
    const calls: Call[] = [];
    installMock(calls, { componentState: 'system' });
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    const image = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'system-scan.png', { type: 'image/png' });
    const zone = screen.getByText(/Перетащите документ в эту область/).closest('.sourceStage');
    fireEvent.drop(zone as Element, { dataTransfer: { files: [image] } });
    await waitFor(() => expect(calls.some(call => call.command === 'parse_source_file')).toBe(true));
    expect(calls.some(call => call.command === 'install_component')).toBe(false);
    expect(screen.queryByRole('dialog', { name: /Установить компонент/ })).toBeNull();
  });

  it('для отсутствующего OCR предлагает подписанную загрузку и только затем разбирает скан', async () => {
    const calls: Call[] = [];
    installMock(calls, { componentInstalled: false });
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    const image = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'scan.png', { type: 'image/png' });
    const zone = screen.getByText(/Перетащите документ в эту область/).closest('.sourceStage');
    fireEvent.drop(zone as Element, { dataTransfer: { files: [image] } });
    const installDialog = await screen.findByRole('dialog', { name: 'Установить компонент «OCR»?' });
    fireEvent.click(within(installDialog).getByRole('button', { name: 'Скачать и установить' }));
    await waitFor(() => expect(calls.some(call => call.command === 'install_component')).toBe(true));
    await waitFor(() => expect(calls.some(call => call.command === 'parse_source_file')).toBe(true));
    expect(calls.findIndex(call => call.command === 'install_component')).toBeLessThan(calls.findIndex(call => call.command === 'parse_source_file'));
  });

});

function parsePayload(calls: Call[], command: string): Record<string, unknown> | undefined {
  return calls.find((c) => c.command === command)?.payload;
}

function getVar(name: string): string {
  return document.documentElement.style.getPropertyValue(name).trim();
}
