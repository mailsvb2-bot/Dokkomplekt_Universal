/// Revalidates all live zero-touch inputs immediately before publication.
///
/// The immutable source/template snapshots are what generation reads. These
/// checks make the live source, live templates and distributed fallback lease
/// a single fail-closed publication boundary: if any one moved, the staged
/// result is discarded instead of being exposed as current.
fn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {
    match universal_intake::current_source_matches(source, source_sha256) {
        Ok(true) => Ok(()),
        Ok(false) => Err(
            "Исходный файл изменился во время обработки. Устаревший комплект не опубликован; новая версия будет обработана отдельно."
                .into(),
        ),
        Err(error) => Err(format!(
            "Не удалось повторно проверить исходный файл перед публикацией: {error}"
        )),
    }
}

fn ensure_generation_inputs_current(
    source: &Path,
    source_sha256: &str,
    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,
    processing_guard: Option<&ProcessingGuard>,
) -> Result<(), String> {
    ensure_source_snapshot_current(source, source_sha256)?;
    template_snapshot::ensure_all_current(template_snapshots)?;
    if let Some(guard) = processing_guard {
        guard.ensure_current()?;
    }
    Ok(())
}
