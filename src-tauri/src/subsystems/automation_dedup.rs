fn automatic_generation_already_processed(
    app: &tauri::AppHandle,
    source: &Path,
    app_data: &Path,
    source_sha256: &str,
    processing_fingerprint: &str,
    processing_job_sha256: &str,
    force_reissue: bool,
) -> Result<bool, String> {
    // force_reissue is the only deliberate bypass. Normal automation is fail-closed:
    // inability to read or validate any durable guard is an error, never "not processed".
    if force_reissue {
        return Ok(false);
    }
    let completed_in_history = repository_for(&default_state_db_path(app)?)?
        .completed_case_exists_for_source_and_plan(source_sha256, processing_fingerprint)
        .map_err(|error| error.to_string())?;
    let completed_in_shared_queue =
        shared_completion_receipt_matches(source, processing_job_sha256)?;
    let completed_in_local_receipts = generation_publication::local_completion_receipt_matches(
        app_data,
        processing_job_sha256,
        source_sha256,
        processing_fingerprint,
    )?;
    let completed_in_emergency_marker =
        generation_publication::plan_bound_emergency_completion_exists(
            source,
            processing_job_sha256,
        )?;
    let completed_in_publication_guard =
        generation_publication::plan_bound_publication_guard_exists(
            app_data,
            processing_job_sha256,
            source_sha256,
            processing_fingerprint,
        )?;
    Ok(completed_in_history
        || completed_in_shared_queue
        || completed_in_local_receipts
        || completed_in_emergency_marker
        || completed_in_publication_guard)
}
