from pathlib import Path

main_path = Path("src-tauri/src/main.rs")
main = main_path.read_text(encoding="utf-8")
include_marker = 'include!("subsystems/update_runtime.rs");\n'
include_replacement = (
    'include!("subsystems/update_runtime.rs");\n'
    'include!("subsystems/automation_consistency.rs");\n'
)
if main.count(include_marker) != 1:
    raise SystemExit(f"main include marker mismatch: {main.count(include_marker)}")
main_path.write_text(main.replace(include_marker, include_replacement, 1), encoding="utf-8")

runtime_path = Path("src-tauri/src/subsystems/automation_runtime.rs")
runtime = runtime_path.read_text(encoding="utf-8")
block = '''fn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {
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

'''
if runtime.count(block) != 1:
    raise SystemExit(f"automation consistency block mismatch: {runtime.count(block)}")
runtime_path.write_text(runtime.replace(block, "", 1), encoding="utf-8")

consistency_path = Path("src-tauri/src/subsystems/automation_consistency.rs")
if consistency_path.exists():
    raise SystemExit("automation_consistency.rs already exists")
consistency_path.write_text(
    '''/// Revalidates all live zero-touch inputs immediately before publication.\n///\n/// The immutable source/template snapshots are what generation reads. These\n/// checks make the live source, live templates and distributed fallback lease\n/// a single fail-closed publication boundary: if any one moved, the staged\n/// result is discarded instead of being exposed as current.\nfn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {\n    match universal_intake::current_source_matches(source, source_sha256) {\n        Ok(true) => Ok(()),\n        Ok(false) => Err(\n            "Исходный файл изменился во время обработки. Устаревший комплект не опубликован; новая версия будет обработана отдельно."\n                .into(),\n        ),\n        Err(error) => Err(format!(\n            "Не удалось повторно проверить исходный файл перед публикацией: {error}"\n        )),\n    }\n}\n\nfn ensure_generation_inputs_current(\n    source: &Path,\n    source_sha256: &str,\n    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,\n    processing_guard: Option<&ProcessingGuard>,\n) -> Result<(), String> {\n    ensure_source_snapshot_current(source, source_sha256)?;\n    template_snapshot::ensure_all_current(template_snapshots)?;\n    if let Some(guard) = processing_guard {\n        guard.ensure_current()?;\n    }\n    Ok(())\n}\n''',
    encoding="utf-8",
)
