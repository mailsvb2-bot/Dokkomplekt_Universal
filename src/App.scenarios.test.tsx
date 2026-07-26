import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { App } from './App';
import { __resetInvokeForTests, __setInvokeForTests, rustCommandNames } from './lib/api';

type Call = { command: string; payload?: Record<string, unknown> };

const accDoc = { id: 'acc_1', button_label: 'Счёт на оплату', template_path: 'a.docx', category: 'Accounting', role_id: 'invoice', required_fields: [], placeholders: ['org.inn'], is_static_copy: false };
const medDoc = { id: 'med_1', button_label: 'Первичный осмотр', template_path: 'b.docx', category: 'Medical', role_id: 'exam', required_fields: [], placeholders: [], is_static_copy: false };
const pack = { pack_id: 'default', name: 'Пакет', documents: [accDoc, medDoc] };
const caseDto = { values: { 'org.inn': { field_id: 'org.inn', value: '7701234567', source: 'parser', confidence: 0.9 } } };
const workflow = { document_id: 'acc_1', prompts: [{ field_id: 'org.inn', title: 'ИНН', required: true, current_value: '7701234567', validation_hint: null }], blocked: false, block_reasons: [] };

function installMock(calls: Call[], options: { componentInstalled?: boolean; componentState?: 'downloaded' | 'bundled' | 'system' | 'missing' } = {}) {
  const componentState = options.componentState ?? ((options.componentInstalled ?? true) ? 'downloaded' : 'missing');
  const componentInstalled = componentState === 'downloaded';
  const componentAvailable = componentState !== 'missing';
  let clauseBlocks: Array<{ block_id: string; title: string; content: string; updated_at: string }> = [];
  __setInvokeForTests(async (command, payload) => {
    calls.push({ command, payload });
    switch (command) {
      case 'first_run_state':
      case 'load_state':
        return { pack, has_user_buttons: true, message: 'ok' } as never;
      case 'parse_source':
        return { semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] } } as never;
      case 'parse_source_file':
        return { source_text: 'Счёт № 148', source_path: '/app-data/scanner-sources/source.docx', semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] } } as never;
      case 'get_intake_capabilities':
        return [{ format: 'PDF', extensions: ['pdf'], available: true, built_in: true, engine: 'pdftotext/OCR', details: 'готово' }] as never;
      case 'get_reference_data_status': return { installed: false, cached: false, restart_required: false, source: 'bundled', published_at: null, complete_years: [2025, 2026], listed_years: [2025, 2026, 2027], message: 'bundled' } as never;
      case 'update_reference_data': return { installed: true, cached: true, restart_required: false, source: 'signed-feed', published_at: '2026-07-18', complete_years: [2025, 2026, 2027], listed_years: [2025, 2026, 2027], message: 'updated' } as never;
      case 'import_reference_data': return { installed: true, cached: true, restart_required: false, source: 'imported', published_at: '2026-07-18', complete_years: [2025, 2026, 2027], listed_years: [2025, 2026, 2027], message: 'imported' } as never;
      case 'get_sidecar_status':
        return [{ tool: 'tesseract', available: true, bundled: true, state: 'bundled', component_id: 'ocr', resolved_path: 'tools/tesseract.exe', purpose: 'OCR' }] as never;
      case 'get_component_statuses':
      case 'refresh_component_catalog':
        return [
          { id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42 * 1024 * 1024, size_label: '42 МБ', unlocks: ['tesseract'], state: componentState, installed: componentInstalled, available: componentAvailable, catalog_available: true, message: 'ok' },
          { id: 'office', label: 'Office', description: '', target: 'windows-x86_64', size_bytes: 210 * 1024 * 1024, size_label: '210 МБ', unlocks: ['soffice'], state: 'downloaded', installed: true, available: true, catalog_available: true, message: 'ok' },
          { id: 'semantic', label: 'Semantic', description: '', target: 'windows-x86_64', size_bytes: 980 * 1024 * 1024, size_label: '980 МБ', unlocks: ['llama_cpp'], state: 'downloaded', installed: true, available: true, catalog_available: true, message: 'ok' },
        ] as never;
      case 'install_component':
      case 'remove_component':
        return { id: 'ocr', label: 'OCR', description: '', target: 'windows-x86_64', size_bytes: 42, size_label: '42 МБ', unlocks: ['tesseract'], state: command === 'install_component' ? 'downloaded' : 'missing', installed: command === 'install_component', available: command === 'install_component', catalog_available: true, message: 'ok' } as never;
      case 'parse_web_source':
        return { source_text: 'Счёт № 148 из HTTPS', semantic_case: caseDto, report: { recognized_title: 'Счёт на оплату', warnings: [] }, final_url: 'https://example.com/doc', content_type: 'text/html' } as never;
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
      case 'render_docx_batch':
        return { output_folder: 'output/148_2026-02-01', created_files: ['output/148_2026-02-01/Счёт на оплату.docx'], created_documents: [{ document_id: 'acc_1', label: 'Счёт на оплату', path: 'output/148_2026-02-01/Счёт на оплату.docx' }] } as never;
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
      case 'icd10_suggest':
        return [{ code: 'F32.1', title: 'Депрессивный эпизод' }] as never;
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
      case 'save_learned_scanner_rule':
      case 'list_learned_scanner_rules':
      case 'delete_learned_scanner_rule': return [] as never;
      case 'update_document_template': return pack as never;
      case 'list_template_versions': return [] as never;
      case 'rollback_template_version': return pack as never;
      case 'get_output_plan':
        return { root_folder: 'output', patient_folder: 'output/Готовые', files: ['output/Готовые/Счёт.docx'], warnings: [] } as never;
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
      case 'prepare_template_setup':
        return [{ document_id: 'tpl', template_path: 't.docx', detected_title: 'Договор', suggested_button_label: 'Договор', editable_button_label: 'Договор', role_id: 'generic', is_static_copy: false, analysis: {}, popup_fields: [] }] as never;
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
  afterEach(() => { __resetInvokeForTests(); vi.restoreAllMocks(); });

  it('каждый пользовательский сценарий вызывает соответствующую Rust-команду', async () => {
    const calls: Call[] = [];
    installMock(calls);
    render(<App />);

    // first_run_state populates documents + profile tabs
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    expect(screen.getByRole('button', { name: 'Бухгалтерия' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Медицина' })).toBeTruthy();

    // A new set must explicitly clear case-specific values before another person/contract.
    await click(/Новый комплект/);
    await waitFor(() => expect(calls.some((c) => c.command === 'reset_case')).toBe(true));

    // Each document keeps its own print-copy count (including 0 = do not print).
    fireEvent.change(screen.getByLabelText('Количество копий для Счёт на оплату'), { target: { value: '3' } });
    fireEvent.change(screen.getByLabelText('Количество копий для Первичный осмотр'), { target: { value: '10' } });
    expect(JSON.parse(localStorage.getItem('dokkomplekt.print-copies.v1') || '{}')).toMatchObject({ acc_1: 3, med_1: 10 });

    // profile filter: switch to Медицина hides Accounting doc
    fireEvent.click(screen.getByRole('button', { name: 'Медицина' }));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Счёт на оплату' })).toBeNull());
    fireEvent.click(screen.getByRole('button', { name: 'Все' }));
    await screen.findByRole('button', { name: 'Счёт на оплату' });

    // parse source text
    await click(/Разобрать текст/);
    await waitFor(() => expect(calls.some((c) => c.command === 'parse_source')).toBe(true));
    expect(parsePayload(calls, 'parse_source')).toMatchObject({ req: { default_year: expect.any(Number) } });

    // direct DOCX source import -> parse_source_file
    const sourceFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Источник.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    const sourceDropZone = screen.getByText(/Перетащите Word\/PDF\/фото\/таблицу\/письмо\/архив/).closest('.fileDropZone');
    expect(sourceDropZone).toBeTruthy();
    fireEvent.drop(sourceDropZone as Element, { dataTransfer: { files: [sourceFile] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'parse_source_file')).toBe(true));

    // Guided Word scanner: the program opens the document, reads the mouse selection,
    // suggests the semantic destination, remembers it and closes Word itself.
    await click(/Открыть Word и показать значение мышкой/);
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
    await click(/Открыть Word и показать значение мышкой/);
    const guidedCancel = await screen.findByRole('dialog', { name: 'Простой сканер мышью' });
    fireEvent.click(within(guidedCancel).getByRole('button', { name: /Отмена — всё закрыть/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'close_word_scanner')).toBe(true));

    // In-app cursor scanner: select a fragment with the mouse and bind it to a semantic field.
    const sourceArea = document.querySelector('.fileDropZone textarea.source') as HTMLTextAreaElement;
    sourceArea.focus();
    sourceArea.setSelectionRange(7, 10);
    fireEvent.select(sourceArea);
    fireEvent.change(screen.getByLabelText('Поле для выделенного фрагмента'), { target: { value: 'document.number' } });
    await click(/Назначить выделение полю/);
    await waitFor(() => expect(calls.some((call) => {
      if (call.command !== 'apply_scanner') return false;
      const request = (call.payload as { req?: { marks?: Array<{ field_id?: string; selected_text?: string }> } } | undefined)?.req;
      return request?.marks?.some((mark) => mark.field_id === 'document.number' && mark.selected_text === '148') ?? false;
    })).toBe(true));

    // select document -> workflow plan + actual template text, fields render
    fireEvent.click(screen.getByRole('button', { name: 'Счёт на оплату' }));
    await screen.findByDisplayValue('7701234567');
    await waitFor(() => expect(calls.some((c) => c.command === 'get_document_template_text')).toBe(true));

    // Existing template is marked through the same guided Word workflow.
    await click(/Разметить шаблон мышью/);
    const guidedTemplate = await screen.findByRole('dialog', { name: 'Простой сканер мышью' });
    fireEvent.click(within(guidedTemplate).getByRole('button', { name: /Я показал значение/ }));
    await waitFor(() => expect(within(guidedTemplate).getAllByText(/Номер счёта/).length).toBeGreaterThan(0));
    fireEvent.click(within(guidedTemplate).getByRole('button', { name: /Да, всё правильно/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'apply_word_scanner_selection')).toBe(true));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_document_template')).toBe(true));

    // pin field -> set_field
    fireEvent.click(screen.getByRole('button', { name: 'Закрепить значение' }));
    await waitFor(() => expect(parsePayload(calls, 'set_field')).toMatchObject({ req: { field_id: 'org.inn', value: '7701234567' } }));

    // save fields -> apply_popup
    await click(/Сохранить поля/);
    await waitFor(() => expect(parsePayload(calls, 'apply_popup')).toMatchObject({ req: { document_id: 'acc_1', answers: [{ field_id: 'org.inn', value: '7701234567' }] } }));

    // specialist can configure the document-specific popup without changing the template.
    await click(/Настроить вопросы/);
    const popupDesigner = await screen.findByRole('dialog', { name: 'Конструктор уточняющих вопросов' });
    fireEvent.click(within(popupDesigner).getByRole('button', { name: /Сохранить вопросы/ }));
    await waitFor(() => expect(parsePayload(calls, 'update_document_popup_fields')).toMatchObject({ req: { document_id: 'acc_1' } }));

    // preview -> render_preview, text shown
    await click(/Предпросмотр/);
    await screen.findByText('СЧЁТ-ПРЕВЬЮ');

    // generate docx -> render_docx
    await click(/Сформировать DOCX/);
    const singlePrompt = await screen.findByRole('dialog', { name: 'Уточнить данные документа' });
    fireEvent.click(within(singlePrompt).getByRole('button', { name: /Применить и создать/ }));
    await waitFor(() => expect(parsePayload(calls, 'render_docx')).toMatchObject({ req: { document_id: 'acc_1' } }));

    // multi-document batch: selection is separate from opening a document
    fireEvent.click(screen.getByRole('checkbox', { name: 'Добавить Счёт на оплату в комплект' }));
    await click(/Сформировать комплект \(1\)/);
    const batchPrompt = await screen.findByRole('dialog', { name: /Уточнить данные комплекта/ });
    fireEvent.click(within(batchPrompt).getByRole('button', { name: /Применить и создать/ }));
    await waitFor(() => expect(parsePayload(calls, 'render_docx_batch')).toMatchObject({
      req: { document_ids: ['acc_1'], output_root: expect.any(String), folder_parts: ['DocumentNumber', 'DocumentDate'], strict: true },
    }));

    // dictionary search -> icd10_suggest, chip shown
    fireEvent.change(screen.getByPlaceholderText(/код или значение/), { target: { value: 'F32' } });
    await click('Найти');
    await screen.findByRole('button', { name: /F32\.1 — Депрессивный эпизод/ });
    expect(parsePayload(calls, 'icd10_suggest')).toMatchObject({ query: 'F32' });

    // utility scenarios use real user inputs, not demo constants
    await click(/Служебные сценарии/);
    await screen.findByText('Конфиденциальность и хранение');
    const semanticCard = screen.getByText('Локальная SemanticModel').closest('.utilityCard');
    expect(semanticCard).toBeTruthy();
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('checkbox', { name: /включить локальное понимание/ }));
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: 'Сохранить' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'update_semantic_model_config')).toBe(true));
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: /Проверить соединение/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'test_semantic_model')).toBe(true));
    vi.spyOn(window, 'prompt').mockReturnValueOnce('corpus-test.json');
    fireEvent.click(within(semanticCard as HTMLElement).getByRole('button', { name: /Экспортировать корпус/ }));
    await waitFor(() => expect(parsePayload(calls, 'export_corpus')).toMatchObject({ req: { output_path: 'corpus-test.json' } }));
    const thresholdCard = screen.getByText('Доказанная автопечать').closest('.utilityCard');
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
    vi.spyOn(window, 'prompt').mockReturnValueOnce('Дата подтверждена по оригиналу');
    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: 'Закрыть' }));
    await waitFor(() => expect(calls.some((c) => c.command === 'resolve_automation_exception')).toBe(true));
    vi.spyOn(window, 'confirm').mockReturnValueOnce(true);
    fireEvent.click(within(exceptionCard as HTMLElement).getByRole('button', { name: /Подтвердить всё/ }));
    await waitFor(() => expect(calls.some((c) => c.command === 'confirm_risk_exception_and_retry')).toBe(true));
    fireEvent.change(screen.getByPlaceholderText('дата начала'), { target: { value: '01.02.2026' } });
    fireEvent.change(screen.getByPlaceholderText('дата окончания'), { target: { value: '03.02.2026' } });
    await click(/Рассчитать/);
    fireEvent.change(screen.getByPlaceholderText(/поле, например/), { target: { value: 'document.number' } });
    fireEvent.change(screen.getByPlaceholderText('выделенный текст'), { target: { value: '148' } });
    await click(/Применить разметку/);
    fireEvent.change(screen.getByPlaceholderText('корневая папка'), { target: { value: 'C:/output' } });
    await click(/Проверить путь/);
    await click(/Сохранить сессию/);
    await click(/Загрузить сессию/);
    await click(/Проверить доступ/);
    await click(/Проверить обновления/);
    await waitFor(() => expect(calls.some((c) => c.command === 'check_for_updates')).toBe(true));
    await click(/Фоновый агент/);
    await click(/Отключить агент/);
    fireEvent.change(screen.getByPlaceholderText(/подписанную лицензию/), { target: { value: 'LIC-123' } });
    await click(/Активировать лицензию/);
    await waitFor(() => expect(parsePayload(calls, 'verify_rust_license_text')).toMatchObject({ req: { license_text: 'LIC-123' } }));
    expect((parsePayload(calls, 'verify_rust_license_text') as { req?: Record<string, unknown> })?.req).not.toHaveProperty('public_key_b64');

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
    fireEvent.change(screen.getByPlaceholderText(/ФИО;contract\.number/),{target:{value:'subject.name;contract.number\nИванов;Д-1'}}); await click(/^Проверить$/); await click(/Создать комплекты/);

    // add-document dialog -> analyze_template, analyze_template_file, prepare + confirm
    await click(/Добавить документ/);
    const dialog = screen.getByRole('dialog', { name: 'Настройка шаблона' });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Анализировать' }));
    // Настоящий выбор файла: байты DOCX уходят в Rust через import_template_file.
    const docxFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Договор.docx', { type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' });
    const docmFile = new File([new Uint8Array([0x50, 0x4b, 0x03, 0x04])], 'Акт.docm', { type: 'application/vnd.ms-word.document.macroEnabled.12' });
    fireEvent.change(within(dialog).getByTestId('template-file-input'), { target: { files: [docxFile, docmFile] } });
    await waitFor(() => expect(calls.some((c) => c.command === 'import_template_file')).toBe(true));
    await waitFor(() => expect(calls.some((c) => c.command === 'analyze_template_file')).toBe(true));
    expect(calls.filter((call) => call.command === 'analyze_template_file').some((call) => JSON.stringify(call.payload).includes('/app-data/user-templates/tpl.docx'))).toBe(true);
    fireEvent.click(await within(dialog).findByRole('button', { name: 'Создать кнопки (2)' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Настройка шаблона' })).toBeNull());
    await screen.findByRole('button', { name: 'Договор' });
    expect(parsePayload(calls, 'prepare_template_setup')).toMatchObject({ req: { candidates: [
      { template_path: '/app-data/user-templates/tpl.docx' },
      { template_path: '/app-data/user-templates/tpl.docx' },
    ] } });

    // HTTPS/site/API intake -> parse_web_source
    fireEvent.change(screen.getByLabelText('HTTPS-источник'), { target: { value: 'https://example.com/doc' } });
    await click(/Загрузить HTTPS/);
    await waitFor(() => expect(parsePayload(calls, 'parse_web_source')).toMatchObject({ req: { url: 'https://example.com/doc' } }));

    // zero-touch «Созданные документы» -> run_created_documents_intake
    fireEvent.change(screen.getByLabelText('Исходный документ'), { target: { value: 'C:/Созданные документы/Первичный.docx' } });
    await click(/Обработать источник/);
    await waitFor(() => expect(parsePayload(calls, 'run_created_documents_intake')).toMatchObject({ req: { source_path: 'C:/Созданные документы/Первичный.docx', output_root: expect.any(String), folder_parts: ['DocumentNumber', 'DocumentDate'] } }));
    await screen.findByText(/Комплект создан:/);
    await click(/Открыть папку/);
    await click(/^Создать PDF$/);
    await waitFor(() => expect(calls.some((c) => c.command === 'export_files_to_pdf')).toBe(true));
    await click(/КЭДО-пакет/);
    await waitFor(() => expect(calls.some((c) => c.command === 'create_kedo_package')).toBe(true));
    await click(/Распечатать выбранное количество/);
    await waitFor(() => expect(parsePayload(calls, 'open_in_file_manager')).toMatchObject({ req: { path: 'C:/Созданные документы/Иванов' } }));
    await waitFor(() => expect(parsePayload(calls, 'print_files')).toMatchObject({ req: { jobs: [{ path: 'C:/Созданные документы/Иванов/Договор.docx', copies: 3 }] } }));

    // semantic understanding -> semantic_extract
    await click(/Извлечь поля/);
    await waitFor(() => expect(parsePayload(calls, 'semantic_extract')).toMatchObject({ req: { source_text: expect.any(String), default_year: expect.any(Number) } }));
    await screen.findByText(/Извлечено полей:/);

    // button management preserves the template while changing only the registry
    vi.spyOn(window, 'prompt').mockReturnValue('Договор новый');
    await click(/Переименовать/);
    await waitFor(() => expect(calls.some((c) => c.command === 'rename_document_button')).toBe(true));
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    await click(/Убрать кнопку/);
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
    const internalOrProfileOnly = new Set(['get_diary_plan', 'get_learned_kit_decision', 'route_intake', 'retry_case_run', 'delete_learned_scanner_rule', 'rollback_template_version', 'install_component', 'refresh_component_catalog', 'remove_component', 'get_print_triage', 'list_template_approvals', 'approve_document_template', 'revoke_document_template_approval', 'import_business_registry', 'lookup_business_registry', 'apply_business_registry_record', 'export_one_c_counterparties', 'import_learning_example_file', 'learn_template_from_examples_command', 'apply_template_learning_map', 'register_learned_template', 'check_template_regression', 'confirm_bundle_exception_and_retry', 'upsert_organization_knowledge', 'delete_organization_knowledge', 'apply_organization_knowledge', 'select_process_blueprint']);
    const expected = rustCommandNames.filter((command) => !internalOrProfileOnly.has(command));
    expect([...reached].sort()).toEqual([...expected].sort());
  }, 20_000);

  it('не предлагает загрузку, когда OCR уже найден в системе', async () => {
    const calls: Call[] = [];
    installMock(calls, { componentState: 'system' });
    const confirm = vi.spyOn(globalThis, 'confirm');
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    const image = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'system-scan.png', { type: 'image/png' });
    const zone = screen.getByText(/Перетащите Word\/PDF\/фото\/таблицу\/письмо\/архив/).closest('.fileDropZone');
    fireEvent.drop(zone as Element, { dataTransfer: { files: [image] } });
    await waitFor(() => expect(calls.some(call => call.command === 'parse_source_file')).toBe(true));
    expect(calls.some(call => call.command === 'install_component')).toBe(false);
    expect(confirm).not.toHaveBeenCalled();
  });

  it('для отсутствующего OCR предлагает подписанную загрузку и только затем разбирает скан', async () => {
    const calls: Call[] = [];
    installMock(calls, { componentInstalled: false });
    vi.spyOn(globalThis, 'confirm').mockReturnValue(true);
    render(<App />);
    await screen.findByRole('button', { name: 'Счёт на оплату' });
    const image = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'scan.png', { type: 'image/png' });
    const zone = screen.getByText(/Перетащите Word\/PDF\/фото\/таблицу\/письмо\/архив/).closest('.fileDropZone');
    fireEvent.drop(zone as Element, { dataTransfer: { files: [image] } });
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
