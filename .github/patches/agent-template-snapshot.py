from pathlib import Path

# 1) Root module registration.
main_path = Path('src-tauri/src/main.rs')
main = main_path.read_text(encoding='utf-8')
marker = 'mod semantic_runtime;\nmod threshold_calibration;\nmod universal_intake;\n'
replacement = 'mod semantic_runtime;\nmod template_snapshot;\nmod threshold_calibration;\nmod universal_intake;\n'
if main.count(marker) != 1:
    raise SystemExit('main module marker mismatch')
main = main.replace(marker, replacement, 1)
main_path.write_text(main, encoding='utf-8')

# 2) Expose the owned source snapshot type crate-wide for the template wrapper.
intake_path = Path('src-tauri/src/universal_intake.rs')
intake = intake_path.read_text(encoding='utf-8')
marker = 'pub use source_snapshot::{capture_stable_source, current_source_matches};\n'
replacement = 'pub use source_snapshot::{capture_stable_source, current_source_matches};\npub(crate) use source_snapshot::StableSourceSnapshot;\n'
if intake.count(marker) != 1:
    raise SystemExit('source snapshot export marker mismatch')
intake = intake.replace(marker, replacement, 1)
intake_path.write_text(intake, encoding='utf-8')

# 3) Automation runtime: capture every pack template once, fingerprint/text/render the snapshot,
# and fail closed if any live template changes before durable publication.
runtime_path = Path('src-tauri/src/subsystems/automation_runtime.rs')
runtime = runtime_path.read_text(encoding='utf-8')

old = '''fn automation_plan_fingerprint(
    app: &tauri::AppHandle,
    pack: &DocumentPack,
    req: &CreatedDocumentsIntakeRequest,
) -> Result<String, String> {
'''
new = '''fn automation_plan_fingerprint(
    app: &tauri::AppHandle,
    pack: &DocumentPack,
    template_snapshots: &BTreeMap<String, template_snapshot::TemplateSnapshot>,
    req: &CreatedDocumentsIntakeRequest,
) -> Result<String, String> {
'''
if runtime.count(old) != 1:
    raise SystemExit('automation fingerprint signature mismatch')
runtime = runtime.replace(old, new, 1)

old = '''    for document in documents {
        let template_path = resolve_user_path(app, &document.template_path)?;
        let (_, _, template_sha256) = file_content_signature(&template_path).map_err(|error| {
            format!(
                "Не удалось вычислить fingerprint шаблона «{}»: {error}",
                document.button_label
            )
        })?;
        templates.push(serde_json::json!({
            "document": document,
            "template_sha256": template_sha256,
        }));
    }
'''
new = '''    for document in documents {
        let snapshot = template_snapshots.get(&document.id).ok_or_else(|| {
            format!("Не найден snapshot шаблона «{}».", document.button_label)
        })?;
        templates.push(serde_json::json!({
            "document": document,
            "template_sha256": snapshot.sha256(),
        }));
    }
'''
if runtime.count(old) != 1:
    raise SystemExit('automation fingerprint loop mismatch')
runtime = runtime.replace(old, new, 1)

old = '''fn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {
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

fn perform_created_documents_intake(
'''
new = '''fn ensure_source_snapshot_current(source: &Path, source_sha256: &str) -> Result<(), String> {
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
) -> Result<(), String> {
    ensure_source_snapshot_current(source, source_sha256)?;
    template_snapshot::ensure_all_current(template_snapshots)
}

fn perform_created_documents_intake(
'''
if runtime.count(old) != 1:
    raise SystemExit('generation input helper marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''    let processed_markers = workspace_hygiene::processed_marker_candidates(&source);
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let processing_fingerprint = automation_plan_fingerprint(app, &pack, &req)?;
'''
new = '''    let processed_markers = workspace_hygiene::processed_marker_candidates(&source);
    let pack = state.pack.lock().map_err(|_| "state lock failed")?.clone();
    let template_snapshots = pack
        .documents
        .iter()
        .map(|document| {
            template_snapshot::TemplateSnapshot::capture(
                app,
                &document.template_path,
                &document.button_label,
            )
            .map(|snapshot| (document.id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let processing_fingerprint =
        automation_plan_fingerprint(app, &pack, &template_snapshots, &req)?;
'''
if runtime.count(old) != 1:
    raise SystemExit('automation snapshot capture marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''        let template_path = resolve_user_path(app, &doc.template_path)?;
        let template_text = extract_docx_text(&template_path)
            .map_err(|e| format!("Шаблон «{}» не читается: {e}", doc.button_label))?;
'''
new = '''        let template_snapshot = template_snapshots
            .get(&doc.id)
            .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", doc.button_label))?;
        let template_text = extract_docx_text(template_snapshot.path())
            .map_err(|e| format!("Шаблон «{}» не читается: {e}", doc.button_label))?;
'''
if runtime.count(old) != 1:
    raise SystemExit('configured template read marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''                    let out_path = stage.join(&out.file_name);
                    let template_path = resolve_user_path(app, &doc.template_path)?;
                    let template_text = configured
'''
new = '''                    let out_path = stage.join(&out.file_name);
                    let template_snapshot = template_snapshots
                        .get(&doc.id)
                        .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", doc.button_label))?;
                    let template_text = configured
'''
if runtime.count(old) != 1:
    raise SystemExit('automation render template path marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''                        &out.document_id,
                        &template_path,
                        &template_text,
'''
new = '''                        &out.document_id,
                        template_snapshot.path(),
                        &template_text,
'''
if runtime.count(old) != 1:
    raise SystemExit('resume template path marker mismatch')
runtime = runtime.replace(old, new, 1)

old = '''                            app,
                            &template_path,
                            &out_path,
'''
new = '''                            app,
                            template_snapshot.path(),
                            &out_path,
'''
if runtime.count(old) != 1:
    raise SystemExit('automation render path marker mismatch')
runtime = runtime.replace(old, new, 1)

old_call = 'ensure_source_snapshot_current(&source, &source_sha256)'
new_call = 'ensure_generation_inputs_current(&source, &source_sha256, &template_snapshots)'
if runtime.count(old_call) != 2:
    raise SystemExit(f'expected 2 source publication checks, found {runtime.count(old_call)}')
runtime = runtime.replace(old_call, new_call)
runtime_path.write_text(runtime, encoding='utf-8')

# 4) Manual render paths + registration use the same immutable bytes for read/hash/render.
commands_path = Path('src-tauri/src/subsystems/document_commands.rs')
commands = commands_path.read_text(encoding='utf-8')

# Learned-template registration.
old = '''    let path = resolve_user_path(&app, &req.template_path)?;
    let text = extract_docx_text(&path).map_err(|error| error.to_string())?;
    let mut document = dokkomplekt_core::create_button_from_template_text(
        &text,
        document_id,
        &path.display().to_string(),
        Some(button_label),
    );
'''
new = '''    let template_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.template_path,
        button_label,
    )?;
    let text = extract_docx_text(template_snapshot.path()).map_err(|error| error.to_string())?;
    let live_template_path = template_snapshot.live_path().display().to_string();
    let mut document = dokkomplekt_core::create_button_from_template_text(
        &text,
        document_id,
        &live_template_path,
        Some(button_label),
    );
'''
if commands.count(old) != 1:
    raise SystemExit('register learned template read marker mismatch')
commands = commands.replace(old, new, 1)

old = '''    let (_, _, template_sha256) = file_content_signature(&path)?;
    let draft = prepare_template_version_draft(
        &app,
        document_id,
        &path,
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
'''
new = '''    let template_sha256 = template_snapshot.sha256().to_string();
    let draft = prepare_template_version_draft(
        &app,
        document_id,
        template_snapshot.path(),
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    template_snapshot.ensure_current()?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
'''
if commands.count(old) != 1:
    raise SystemExit('register learned template draft marker mismatch')
commands = commands.replace(old, new, 1)

old = '''            "template_path": path.display().to_string(),
'''
new = '''            "template_path": template_snapshot.live_path().display().to_string(),
'''
if commands.count(old) < 1:
    raise SystemExit('register audit path marker missing')
commands = commands.replace(old, new, 1)

# Bulk first-time template confirmation: snapshot each row and archive/hash exactly that copy.
old = '''    let incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
    let mut drafts = Vec::with_capacity(req.rows.len());
    for row in &req.rows {
        let path = resolve_user_path(&app, &row.template_path)?;
        let (_, _, template_sha256) = file_content_signature(&path)?;
        drafts.push(prepare_template_version_draft(
            &app,
            &row.document_id,
            &path,
            &template_sha256,
            "Первичная публикация пользовательского шаблона.",
        )?);
    }
    let (result, _) = publish_pack_with_template_versions(&app, &state, &drafts, |pack| {
'''
new = '''    let incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
    let template_snapshots = req
        .rows
        .iter()
        .map(|row| {
            template_snapshot::TemplateSnapshot::capture(
                &app,
                &row.template_path,
                &row.editable_button_label,
            )
            .map(|snapshot| (row.document_id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut drafts = Vec::with_capacity(req.rows.len());
    for row in &req.rows {
        let snapshot = template_snapshots
            .get(&row.document_id)
            .ok_or_else(|| format!("Не найден snapshot шаблона {}.", row.document_id))?;
        drafts.push(prepare_template_version_draft(
            &app,
            &row.document_id,
            snapshot.path(),
            snapshot.sha256(),
            "Первичная публикация пользовательского шаблона.",
        )?);
    }
    template_snapshot::ensure_all_current(&template_snapshots)?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &drafts, |pack| {
'''
if commands.count(old) != 1:
    raise SystemExit('confirm template setup marker mismatch')
commands = commands.replace(old, new, 1)

# Single manual render.
old = '''    let template_text = template_text_for_document(&app, &doc)?;
    // Both paths are anchored: an installed app must not depend on the process CWD.
    let template_path = resolve_user_path(&app, &doc.template_path)?;
    let desired_output = resolve_user_path(&app, &req.output_path)?;
'''
new = '''    let template_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &doc.template_path,
        &doc.button_label,
    )?;
    let template_text = extract_docx_text(template_snapshot.path()).map_err(|e| e.to_string())?;
    // Both paths are anchored: an installed app must not depend on the process CWD.
    let desired_output = resolve_user_path(&app, &req.output_path)?;
'''
if commands.count(old) != 1:
    raise SystemExit('single render snapshot marker mismatch')
commands = commands.replace(old, new, 1)

old = '''        &app,
        &template_path,
        &reservation.path,
'''
new = '''        &app,
        template_snapshot.path(),
        &reservation.path,
'''
if commands.count(old) != 1:
    raise SystemExit('single render path marker mismatch')
commands = commands.replace(old, new, 1)

old = '''    let output_path = match reservation.commit() {
'''
new = '''    if let Err(error) = template_snapshot.ensure_current() {
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let output_path = match reservation.commit() {
'''
if commands.count(old) != 1:
    raise SystemExit('single render precommit marker mismatch')
commands = commands.replace(old, new, 1)

old = '''    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_file(&output_path);
'''
new = '''    if let Err(error) = template_snapshot.ensure_current() {
        let _ = std::fs::remove_file(&output_path);
        rollback_counter_reservations(&app, &hydrated.counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_file(&output_path);
'''
if commands.count(old) != 1:
    raise SystemExit('single render postcommit marker mismatch')
commands = commands.replace(old, new, 1)

# Batch manual render: capture all templates before quota reservation.
old = '''    let base_case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let output_root = resolve_user_path(&app, &req.output_root)?;
'''
new = '''    let template_snapshots = documents
        .iter()
        .map(|document| {
            template_snapshot::TemplateSnapshot::capture(
                &app,
                &document.template_path,
                &document.button_label,
            )
            .map(|snapshot| (document.id.clone(), snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let base_case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let output_root = resolve_user_path(&app, &req.output_root)?;
'''
if commands.count(old) != 1:
    raise SystemExit('batch snapshot capture marker mismatch')
commands = commands.replace(old, new, 1)

old = '''        for document in &documents {
            let template_path = resolve_user_path(&app, &document.template_path)?;
            let template_text = extract_docx_text(&template_path).map_err(|e| e.to_string())?;
'''
new = '''        for document in &documents {
            let template_snapshot = template_snapshots
                .get(&document.id)
                .ok_or_else(|| format!("Не найден snapshot шаблона «{}».", document.button_label))?;
            let template_text = extract_docx_text(template_snapshot.path()).map_err(|e| e.to_string())?;
'''
if commands.count(old) != 1:
    raise SystemExit('batch template read marker mismatch')
commands = commands.replace(old, new, 1)

old = '''            let extension = template_path
                .extension()
'''
new = '''            let extension = template_snapshot
                .path()
                .extension()
'''
if commands.count(old) != 1:
    raise SystemExit('batch extension marker mismatch')
commands = commands.replace(old, new, 1)

old = '''                &app,
                &template_path,
                &reservation.path,
'''
new = '''                &app,
                template_snapshot.path(),
                &reservation.path,
'''
if commands.count(old) != 1:
    raise SystemExit('batch render path marker mismatch')
commands = commands.replace(old, new, 1)

old = '''    let output_folder = match publish_stage_to_unique_directory(&stage, &desired_output_folder) {
'''
new = '''    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    let output_folder = match publish_stage_to_unique_directory(&stage, &desired_output_folder) {
'''
if commands.count(old) != 1:
    raise SystemExit('batch prepublish marker mismatch')
commands = commands.replace(old, new, 1)

old = '''    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_dir_all(&output_folder);
'''
new = '''    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
        let _ = std::fs::remove_dir_all(&output_folder);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(error);
    }
    if let Err(error) = commit_generation_access(&app, &permit) {
        let _ = std::fs::remove_dir_all(&output_folder);
'''
# There is exactly one matching block in render_docx_batch after the earlier single-render block was changed.
if commands.count(old) != 1:
    raise SystemExit(f'batch postpublish marker mismatch: {commands.count(old)}')
commands = commands.replace(old, new, 1)
commands_path.write_text(commands, encoding='utf-8')

# 5) Crash-consistency documentation.
doc_path = Path('docs/CRASH_CONSISTENCY.md')
doc = doc_path.read_text(encoding='utf-8')
marker = '## Generated documents\n'
section = '''## Template input stability\n\nEvery generation run captures each configured template into a private immutable snapshot before planning. The same bytes drive template SHA-256 fingerprints, placeholder extraction, resume fingerprints and DOCX rendering. Live template paths are revalidated before publication and again before commercial commit; replacement of a template during a run aborts stale output and rolls back explicit reservations. Template registration and bulk first-run confirmation likewise analyze, hash and version-copy one captured snapshot rather than reopening a mutable live path between phases.\n\n'''
if doc.count(marker) != 1:
    raise SystemExit('crash-consistency documentation marker mismatch')
doc = doc.replace(marker, section + marker, 1)
doc_path.write_text(doc, encoding='utf-8')
