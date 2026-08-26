from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text('utf-8')


def tail_between(text: str, start: str, end: str) -> str:
    begin = text.index(start)
    finish = text.index(end, begin)
    return text[begin:finish]


def test_published_outputs_are_never_deleted_or_refunded_after_boundary() -> None:
    document = read('src-tauri/src/subsystems/document_commands.rs')
    single = tail_between(document, '    let mut publication_warnings = Vec::new();', '#[derive(Debug, Deserialize)]\nstruct RenderDocxBatchRequest')
    assert 'rollback_generation_access' not in single
    assert 'rollback_counter_reservations' not in single
    assert 'remove_file(&output_path)' not in single
    assert 'generation_publication::finalize_published_generation' in single

    batch = tail_between(document, '    let mut warnings = Vec::new();', '#[derive(Debug, Deserialize)]\nstruct ScannerRequest')
    assert 'rollback_generation_access' not in batch
    assert 'rollback_counter_reservations' not in batch
    assert 'remove_dir_all(&output_folder)' not in batch
    assert 'generation_publication::finalize_published_generation' in batch

    mail = read('src-tauri/src/subsystems/automation_mail_merge.rs')
    published = mail[mail.index('    let mut warnings = Vec::new();'):]
    assert 'rollback_generation_access' not in published
    assert 'rollback_counter_reservations' not in published
    assert 'remove_dir_all(&published)' not in published
    assert 'generation_publication::finalize_published_generation' in published

    automation = read('src-tauri/src/subsystems/automation_runtime.rs')
    after_publish = tail_between(automation, '// The filesystem publication is the irreversible business boundary.', 'let audit_details = serde_json::json!')
    assert 'case_run.mark_business_terminal();' in after_publish
    assert after_publish.index('case_run.mark_business_terminal();') < after_publish.index('ensure_generation_inputs_current(')
    assert 'rollback_generation_access' not in after_publish
    assert 'rollback_counter_reservations' not in after_publish
    assert 'remove_dir_all(&patient_dir)' not in after_publish
    assert 'generation_publication::finalize_published_generation' in after_publish


def test_prepublish_failures_still_refund_and_cleanup() -> None:
    document = read('src-tauri/src/subsystems/document_commands.rs')
    pre_single = tail_between(document, 'let permit = reserve_generation_access', 'let output_path = match reservation.commit()')
    assert 'rollback_generation_access' in pre_single
    assert 'rollback_counter_reservations' in pre_single

    automation = read('src-tauri/src/subsystems/automation_runtime.rs')
    before_publish = tail_between(automation, 'let names = match render_result', 'let patient_dir = match publish_stage_to_unique_directory')
    assert 'rollback_generation_access' in before_publish
    assert 'rollback_counter_reservations' in before_publish


def test_published_usage_finalize_is_idempotent_and_storage_authoritative() -> None:
    storage = read('crates/dokkomplekt-storage/src/lib.rs')
    assert 'pub fn finalize_published_usage(&mut self, reservation_id: &str)' in storage
    block = tail_between(storage, 'pub fn finalize_published_usage', 'pub fn rollback_usage')
    assert '"reserved"' in block
    assert '"committed" | "committed_after_crash"' in block
    assert '"rolled_back"' in block
    assert 'published usage reservation was already rolled back' in block
    assert 'published_usage_finalization_is_idempotent_and_never_refunds' in storage


def test_publication_receipt_is_non_pii_and_reconciled_before_stale_fallback() -> None:
    publication = read('src-tauri/src/generation_publication.rs')
    receipt_struct = tail_between(publication, 'struct PublicationReceipt', 'struct PublicationReconciliationReport')
    assert 'reservation_id' in receipt_struct
    assert 'output_sha256' in receipt_struct
    assert 'output_path' not in receipt_struct
    assert 'source_path' not in receipt_struct
    assert 'patient' not in receipt_struct.lower()
    assert 'reconcile_publication_receipts(&app_data, repo)' in publication
    startup = tail_between(publication, 'pub(crate) fn recover_startup_generation_state', '#[cfg(test)]')
    assert startup.index('reconcile_publication_receipts') < startup.index('recover_stale_usage_reservations')
    assert 'recover_interrupted_case_runs' in startup


def test_publication_warnings_cross_rust_typescript_and_ui_boundaries() -> None:
    document = read('src-tauri/src/subsystems/document_commands.rs')
    mail = read('src-tauri/src/subsystems/automation_mail_merge.rs')
    types = read('src/lib/types.ts')
    validation = read('src/lib/runtimeValidation.ts')
    app = read('src/App.tsx')
    tools = read('src/components/AdvancedToolsPanel.tsx')
    assert 'warnings: Vec<String>' in document
    assert 'warnings: Vec<String>' in mail
    assert 'warnings?: string[];' in types
    assert "optionalStringArray(command, root.warnings, 'warnings');" in validation
    assert 'res.warnings?.length' in app
    assert 'result.warnings?.length' in tools


def test_postpublish_template_change_is_warning_not_rollback() -> None:
    document = read('src-tauri/src/subsystems/document_commands.rs')
    single = tail_between(document, '    let mut publication_warnings = Vec::new();', '#[derive(Debug, Deserialize)]\nstruct RenderDocxBatchRequest')
    assert 'published_template_changed_after_boundary' in single
    assert 'result.warnings.extend(publication_warnings)' in single
    batch = tail_between(document, '    let mut warnings = Vec::new();', '#[derive(Debug, Deserialize)]\nstruct ScannerRequest')
    assert 'published_templates_changed_after_boundary' in batch
    mail = read('src-tauri/src/subsystems/automation_mail_merge.rs')
    assert 'published_mail_merge_templates_changed_after_boundary' in mail

def test_durable_journal_is_prepared_before_every_filesystem_publication_boundary() -> None:
    publication = read('src-tauri/src/generation_publication.rs')
    assert 'PublicationPhase::Prepared' in publication
    assert 'PublicationPhase::Published' in publication
    assert 'pub(crate) fn prepare_publication' in publication
    assert 'pub(crate) fn confirm_publication' in publication
    assert 'pub(crate) fn plan_bound_publication_guard_exists' in publication

    document = read('src-tauri/src/subsystems/document_commands.rs')
    single_start = document.index('fn render_docx(')
    single_end = document.index('struct RenderDocxBatchRequest', single_start)
    single = document[single_start:single_end]
    assert single.index('generation_publication::prepare_publication') < single.index('reservation.commit()')
    assert single.index('reservation.commit()') < single.index('generation_publication::confirm_publication')

    batch_start = document.index('fn render_docx_batch(')
    batch_end = document.index('struct ScannerRequest', batch_start)
    batch = document[batch_start:batch_end]
    assert batch.index('generation_publication::prepare_publication') < batch.index(
        'publish_stage_to_unique_directory'
    )
    assert batch.index('publish_stage_to_unique_directory') < batch.index(
        'generation_publication::confirm_publication'
    )

    mail = read('src-tauri/src/subsystems/automation_mail_merge.rs')
    assert mail.index('generation_publication::prepare_publication') < mail.index(
        'publish_stage_to_unique_directory'
    )
    assert mail.index('publish_stage_to_unique_directory') < mail.index(
        'generation_publication::confirm_publication'
    )

    automation = read('src-tauri/src/subsystems/automation_runtime.rs')
    auto_publish = automation.index('let publication_binding = generation_publication::PublicationPlanBinding')
    auto_confirm = automation.index('generation_publication::confirm_publication', auto_publish)
    assert automation.index('generation_publication::prepare_publication', auto_publish) < automation.index(
        'publish_stage_to_unique_directory', auto_publish
    ) < auto_confirm
    assert 'completed_in_publication_guard' in automation
    assert 'complete_publication_receipt' in automation



def test_unverified_batch_readback_is_quarantined_without_refunding_business_state() -> None:
    publication = read('src-tauri/src/subsystems/publication_collision.rs')
    recovery = tail_between(
        publication,
        'fn recover_unverified_batch_publication',
        '#[cfg(test)]',
    )
    assert 'rollback_unverified_publication' in recovery
    assert '.dokkomplekt-failed' in publication
    assert 'commit_generation_access' in recovery
    assert 'rollback_generation_access' not in recovery
    assert 'rollback_counter_reservations' not in recovery
    assert '"usage_refunded": false' in recovery
    assert '"counters_refunded": false' in recovery
    assert 'abort_prepared_publication' in recovery

    document = read('src-tauri/src/subsystems/document_commands.rs')
    start = document.index('fn render_docx_batch(')
    end = document.index('struct ScannerRequest', start)
    batch = document[start:end]
    assert batch.index('let verification =') < batch.index(
        'generation_publication::confirm_publication'
    )
    assert 'recover_unverified_batch_publication' in batch
    failure_start = batch.index('Err(error) => {', batch.index('let created_files = match verification'))
    failure_end = batch.index('let mut warnings = Vec::new();', failure_start)
    failure = batch[failure_start:failure_end]
    assert 'rollback_generation_access' not in failure
    assert 'rollback_counter_reservations' not in failure


def test_failed_replacement_restores_previous_folder_and_quarantines_new_files() -> None:
    publication = read('src-tauri/src/subsystems/publication_collision.rs')
    assert 'fn failed_new_version_is_quarantined_outside_user_visible_folder()' in publication
    assert 'fn failed_replacement_restores_previous_directory_from_backup()' in publication
    assert 'std::fs::rename(output_folder, &failed)' in publication
    assert 'std::fs::rename(backup, output_folder)' in publication
