/// Build the publication identity for an explicit UI generation without
/// persisting user/source values. The automatic watcher already has a
/// content-addressed processing job; manual generation needs the same
/// fail-closed binding at the PublicationGate boundary.
fn manual_identity_sha256<T: Serialize>(namespace: &str, value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn manual_publication_plan_binding<T: Serialize>(
    state: &AppState,
    resolved_case: &SemanticCase,
    processing_payload: &T,
) -> Result<generation_publication::PublicationPlanBinding, String> {
    let provenance = state
        .source_provenance
        .lock()
        .map_err(|_| "source provenance state lock failed")?
        .clone();
    let source_sha256 = match provenance {
        Some(value) if is_sha256_hex(&value.source_sha256) => {
            value.source_sha256.to_ascii_lowercase()
        }
        Some(_) => {
            return Err(
                "Источник manual generation не содержит проверяемый SHA-256; публикация заблокирована."
                    .into(),
            )
        }
        None => manual_identity_sha256(
            "dokkomplekt-manual-source-set-v1",
            resolved_case,
        )?,
    };
    let resolved_case_sha256 = manual_identity_sha256(
        "dokkomplekt-manual-resolved-case-v1",
        resolved_case,
    )?;
    let processing_fingerprint = manual_identity_sha256(
        "dokkomplekt-manual-processing-fingerprint-v1",
        processing_payload,
    )?;
    let processing_job_sha256 = manual_identity_sha256(
        "dokkomplekt-manual-processing-job-v1",
        &serde_json::json!({
            "source_sha256": &source_sha256,
            "resolved_case_sha256": &resolved_case_sha256,
            "processing_fingerprint": &processing_fingerprint,
        }),
    )?;
    Ok(generation_publication::PublicationPlanBinding {
        processing_job_sha256,
        source_sha256,
        processing_fingerprint,
    })
}
