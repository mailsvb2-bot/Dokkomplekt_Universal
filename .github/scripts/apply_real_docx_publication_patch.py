from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
commands_path = ROOT / "src-tauri/src/subsystems/document_commands.rs"
commands = commands_path.read_text(encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


commands = replace_once(
    commands,
    '''    let output_root = resolve_user_path(&app, &req.output_root)?;
    std::fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&output_root, Duration::from_secs(24 * 60 * 60))?;
''',
    '''    let output_root = resolve_user_path(&app, &req.output_root)?;
    // Render beside the final output root instead of creating the user-visible
    // output directory before the batch has actually succeeded. Publication
    // creates the visible root only at the atomic visibility boundary below.
    let stage_parent = output_root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_root.clone());
    std::fs::create_dir_all(&stage_parent).map_err(|error| error.to_string())?;
    cleanup_stale_stage_directories(&stage_parent, Duration::from_secs(24 * 60 * 60))?;
''',
    "delay visible root creation",
)

commands = replace_once(
    commands,
    '''    let privacy = load_privacy_preferences(&app)?;
    let stage = output_root.join(format!(
        ".dokkomplekt-manual-stage-{}-{}",
''',
    '''    let privacy = load_privacy_preferences(&app)?;
    let stage = stage_parent.join(format!(
        ".dokkomplekt-manual-stage-{}-{}",
''',
    "move stage beside output root",
)

commands = replace_once(
    commands,
    '''    let mut counter_reservations = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
''',
    '''    let mut counter_reservations = Vec::new();
    let mut render_warnings = Vec::new();
    let rendered = (|| -> Result<Vec<PathBuf>, String> {
''',
    "add render warnings",
)

commands = replace_once(
    commands,
    '''        if privacy.write_trust_report {
            let provenance = state
                .source_provenance
                .lock()
                .map_err(|_| "source provenance state lock failed")?
                .clone()
                .ok_or_else(|| {
                    "Для проверяемого отчёта сначала загрузите файл, вставьте текст или получите HTTPS-источник.".to_string()
                })?;
            write_trust_report(
                &stage,
                &report_case,
                TrustReportContext {
                    source_name: &provenance.source_name,
                    source_sha256: &provenance.source_sha256,
                    generated_names: &generated_names,
                    used_field_ids: &used_field_ids,
                    include_values: privacy.include_values_in_trust_report,
                    source_warnings: &[],
                },
            )?;
        }
        Ok(paths)
''',
    '''        if privacy.write_trust_report {
            match state.source_provenance.lock() {
                Ok(guard) => match guard.clone() {
                    Some(provenance) => {
                        if let Err(error) = write_trust_report(
                            &stage,
                            &report_case,
                            TrustReportContext {
                                source_name: &provenance.source_name,
                                source_sha256: &provenance.source_sha256,
                                generated_names: &generated_names,
                                used_field_ids: &used_field_ids,
                                include_values: privacy.include_values_in_trust_report,
                                source_warnings: &[],
                            },
                        ) {
                            render_warnings.push(format!(
                                "DOCX созданы; локальный отчёт проверяемости не создан: {error}"
                            ));
                        }
                    }
                    None => render_warnings.push(
                        "DOCX созданы; локальный отчёт проверяемости пропущен: источник не имеет доступного provenance после восстановления состояния.".into(),
                    ),
                },
                Err(_) => render_warnings.push(
                    "DOCX созданы; локальный отчёт проверяемости пропущен: состояние provenance временно недоступно.".into(),
                ),
            }
        }
        Ok(paths)
''',
    "make trust report non-fatal",
)

commands = replace_once(
    commands,
    '''    let staged_paths = match rendered {
        Ok(paths) => paths,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
''',
    '''    let staged_paths = match rendered {
        Ok(paths) => paths,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if staged_paths.len() != documents.len() {
        let _ = std::fs::remove_dir_all(&stage);
        rollback_counter_reservations(&app, &counter_reservations);
        rollback_generation_access(&app, &state, &permit);
        return Err(format!(
            "Комплект не опубликован: запрошено {} документ(ов), физически подготовлено {}.",
            documents.len(),
            staged_paths.len()
        ));
    }
    for path in &staged_paths {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&stage);
                rollback_counter_reservations(&app, &counter_reservations);
                rollback_generation_access(&app, &state, &permit);
                return Err(format!(
                    "Комплект не опубликован: подготовленный DOCX исчез до публикации ({}): {error}",
                    path.display()
                ));
            }
        };
        if !metadata.is_file() || metadata.len() == 0 {
            let _ = std::fs::remove_dir_all(&stage);
            rollback_counter_reservations(&app, &counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(format!(
                "Комплект не опубликован: подготовленный DOCX пуст или не является файлом: {}",
                path.display()
            ));
        }
    }
    if let Err(error) = template_snapshot::ensure_all_current(&template_snapshots) {
''',
    "verify staged files",
)

commands = replace_once(
    commands,
    '''    let mut warnings = Vec::new();
    if let Some(backup) = backup_folder.as_ref() {
''',
    '''    let created_files = staged_paths
        .iter()
        .map(|path| {
            path.file_name()
                .ok_or_else(|| format!("Созданный файл не имеет имени: {}", path.display()))
                .map(|name| output_folder.join(name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if created_files.len() != documents.len() {
        return Err(format!(
            "КРИТИЧЕСКАЯ ОШИБКА публикации: ожидалось {} DOCX, получено {} в {}.",
            documents.len(),
            created_files.len(),
            output_folder.display()
        ));
    }
    for path in &created_files {
        let metadata = std::fs::metadata(path).map_err(|error| {
            format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: файл отсутствует на диске после публикации ({}): {error}",
                path.display()
            )
        })?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: опубликованный DOCX пуст или не является файлом: {}",
                path.display()
            ));
        }
        extract_docx_text(path).map_err(|error| {
            format!(
                "КРИТИЧЕСКАЯ ОШИБКА публикации: опубликованный DOCX не читается как Word-документ ({}): {error}",
                path.display()
            )
        })?;
    }
    let created_file_strings = created_files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    warnings.extend(render_warnings);
    if let Some(backup) = backup_folder.as_ref() {
''',
    "verify published files",
)

commands = replace_once(
    commands,
    '''    let created_files = staged_paths
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| output_folder.join(name).display().to_string())
        .collect::<Vec<_>>();
    let created_documents = documents
        .iter()
        .zip(created_files.iter())
        .map(|(document, path)| CreatedDocumentOutputDto {
            document_id: document.id.clone(),
            label: document.button_label.clone(),
            path: path.clone(),
        })
        .collect();
    Ok(RenderDocxBatchResponse {
        output_folder: output_folder.display().to_string(),
        created_files,
''',
    '''    let created_documents = documents
        .iter()
        .zip(created_file_strings.iter())
        .map(|(document, path)| CreatedDocumentOutputDto {
            document_id: document.id.clone(),
            label: document.button_label.clone(),
            path: path.clone(),
        })
        .collect();
    if let Err(error) = open_in_file_manager(
        OpenPathRequest {
            path: output_folder.display().to_string(),
        },
        app.clone(),
    ) {
        warnings.push(format!(
            "Комплект создан и проверен, но открыть итоговую папку автоматически не удалось: {error}"
        ));
    }
    Ok(RenderDocxBatchResponse {
        output_folder: output_folder.display().to_string(),
        created_files: created_file_strings,
''',
    "return verified paths and open exact folder",
)

commands_path.write_text(commands, encoding="utf-8")

hardening = ROOT / "tests/test_runtime_user_scenario_hardening.py"
text = hardening.read_text(encoding="utf-8")
text = replace_once(
    text,
    '    assert "Для проверяемого отчёта сначала загрузите файл" in commands\n',
    '    assert "match state.source_provenance.lock()" in commands\n'
    '    assert "DOCX созданы; локальный отчёт проверяемости пропущен" in commands\n'
    '    assert "Для проверяемого отчёта сначала загрузите файл" not in commands\n',
    "update provenance regression contract",
)
hardening.write_text(text, encoding="utf-8")

physical = ROOT / "tests/test_manual_generation_physical_publication_contract.py"
physical.write_text(
    '''from pathlib import Path\n\nROOT = Path(__file__).resolve().parents[1]\nCOMMANDS = (ROOT / "src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")\n\n\ndef body() -> str:\n    start = COMMANDS.index("fn render_docx_batch(")\n    end = COMMANDS.index("#[derive(Debug, Deserialize)]\\nstruct ScannerRequest", start)\n    return COMMANDS[start:end]\n\n\ndef test_visible_output_root_is_not_created_before_success():\n    batch = body()\n    assert "let stage_parent = output_root" in batch\n    assert "let stage = stage_parent.join(format!(" in batch\n    assert "std::fs::create_dir_all(&output_root)" not in batch\n\n\ndef test_one_physical_file_is_required_for_every_requested_document():\n    batch = body()\n    assert "if staged_paths.len() != documents.len()" in batch\n    assert "if created_files.len() != documents.len()" in batch\n    assert "std::fs::metadata(path)" in batch\n    assert "extract_docx_text(path)" in batch\n    assert "КРИТИЧЕСКАЯ ОШИБКА публикации" in batch\n\n\ndef test_trust_report_is_ancillary_and_cannot_delete_docx():\n    batch = body()\n    report_start = batch.index("if privacy.write_trust_report")\n    report_end = batch.index("Ok(paths)", report_start)\n    report = batch[report_start:report_end]\n    assert "write_trust_report(" in report\n    assert "render_warnings.push" in report\n    assert "Для проверяемого отчёта сначала загрузите файл" not in report\n\n\ndef test_success_opens_exact_verified_publication_folder():\n    batch = body()\n    assert batch.index("extract_docx_text(path)") < batch.index("open_in_file_manager(")\n    assert "path: output_folder.display().to_string()" in batch\n''',
    encoding="utf-8",
)

print("publication patch applied")
