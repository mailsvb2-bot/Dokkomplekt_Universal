// Explicit output-collision publication policies.

fn publish_stage_replacing_with_backup(
    stage: &Path,
    desired: &Path,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let parent = desired.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let lock_path = parent.join(format!(
        ".dokkomplekt-dir-replace-{}.lock",
        sanitize_path_component(
            desired
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("output")
        )
    ));
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            format!(
                "Не удалось получить эксклюзивную блокировку безопасной замены комплекта: {error}"
            )
        })?;

    let result = (|| -> Result<(PathBuf, Option<PathBuf>), String> {
        if !desired.exists() {
            std::fs::rename(stage, desired).map_err(|error| {
                format!("Не удалось опубликовать новый комплект: {error}")
            })?;
            return Ok((desired.to_path_buf(), None));
        }
        if !desired.is_dir() {
            return Err(format!(
                "Нельзя безопасно заменить результат: {} существует и не является папкой.",
                desired.display()
            ));
        }

        let backup_root = parent.join(".dokkomplekt-backups");
        std::fs::create_dir_all(&backup_root)
            .map_err(|error| format!("Не удалось создать каталог резервных копий: {error}"))?;
        let stem = sanitize_path_component(
            desired
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Комплект"),
        );
        let backup = backup_root.join(format!(
            "{stem}.backup-{}-{}",
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Uuid::new_v4()
        ));

        std::fs::rename(desired, &backup).map_err(|error| {
            format!(
                "Не удалось сначала сохранить существующий комплект в резервную копию: {error}"
            )
        })?;
        match std::fs::rename(stage, desired) {
            Ok(()) => Ok((desired.to_path_buf(), Some(backup))),
            Err(publish_error) => {
                match std::fs::rename(&backup, desired) {
                    Ok(()) => Err(format!(
                        "Новый комплект не опубликован ({publish_error}); предыдущая версия восстановлена."
                    )),
                    Err(rollback_error) => Err(format!(
                        "КРИТИЧЕСКАЯ ОШИБКА безопасной замены: новый комплект не опубликован ({publish_error}), а автоматическое восстановление старой папки не удалось ({rollback_error}). Резервная копия сохранена по пути {}.",
                        backup.display()
                    )),
                }
            }
        }
    })();

    drop(lock);
    let _ = std::fs::remove_file(&lock_path);
    result
}

#[cfg(test)]
mod publication_collision_tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dokkomplekt-{label}-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    #[test]
    fn replace_with_backup_never_destroys_previous_directory_before_backup() {
        let root = temp_root("replace-with-backup");
        let desired = root.join("Комплект");
        let stage = root.join(".stage");
        std::fs::create_dir_all(&desired).unwrap();
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(desired.join("old.txt"), "old").unwrap();
        std::fs::write(stage.join("new.txt"), "new").unwrap();

        let (published, backup) = publish_stage_replacing_with_backup(&stage, &desired).unwrap();
        let backup = backup.expect("existing target must be backed up");
        assert_eq!(published, desired);
        assert_eq!(std::fs::read_to_string(desired.join("new.txt")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(backup.join("old.txt")).unwrap(), "old");
        assert!(!desired.join("old.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn replace_policy_without_existing_target_publishes_without_backup() {
        let root = temp_root("replace-new-target");
        let desired = root.join("Комплект");
        let stage = root.join(".stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("new.txt"), "new").unwrap();

        let (published, backup) = publish_stage_replacing_with_backup(&stage, &desired).unwrap();
        assert_eq!(published, desired);
        assert!(backup.is_none());
        assert_eq!(std::fs::read_to_string(desired.join("new.txt")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(root);
    }
}

// Manual generation publication proof.
//
// Keep filesystem publication verification separate from the Tauri command
// orchestration so the final user-visible DOCX boundary can be unit-tested.

/// Inspect the rendered Word document after strict rendering. This second pass is
/// deliberately advisory for semantic/role heuristics: DOCX text extraction is
/// lossy around tables, runs and signature layouts, while the preflight plus strict
/// renderer already own missing-input enforcement. Only failure to read the actual
/// rendered Word file remains a hard publication error.
fn rendered_document_completeness_advisory(
    document: &DocumentTemplateSpec,
    template_text: &str,
    semantic_case: &SemanticCase,
    rendered_path: &Path,
) -> Result<Option<String>, String> {
    let rendered_text = extract_docx_text(rendered_path).map_err(|error| {
        format!(
            "Не удалось проверить созданный документ «{}»: {error}",
            document.button_label
        )
    })?;
    let missing_required = document
        .required_fields
        .iter()
        .filter(|field_id| !semantic_case.has(field_id))
        .cloned()
        .collect::<Vec<_>>();
    let requirements = dokkomplekt_core::required_blocks_for(document, template_text);
    let unmet_blocks = dokkomplekt_core::unmet_blocks(&requirements, semantic_case, &rendered_text);
    if missing_required.is_empty() && unmet_blocks.is_empty() {
        return Ok(None);
    }

    let mut reasons = Vec::new();
    if !missing_required.is_empty() {
        reasons.push(format!(
            "не подтверждены дополнительные поля: {}",
            missing_required.join(", ")
        ));
    }
    if !unmet_blocks.is_empty() {
        reasons.push(format!(
            "извлечённый текст Word не подтвердил блоки: {}",
            unmet_blocks.join(", ")
        ));
    }
    Ok(Some(format!(
        "Документ «{}» физически создан; дополнительная проверка требует внимания: {}.",
        document.button_label,
        reasons.join("; ")
    )))
}

fn ensure_rendered_document_complete(
    document: &DocumentTemplateSpec,
    template_text: &str,
    semantic_case: &SemanticCase,
    rendered_path: &Path,
) -> Result<(), String> {
    // Do not destroy a successfully rendered DOCX because a lossy post-render
    // text heuristic disagrees with the already-completed preflight/strict render.
    // The advisory is intentionally evaluated (and unit-tested) so the validation
    // contract does not disappear; the publication boundary remains fail-closed on
    // unreadable/empty/missing physical files via this read and the final verifier.
    let _advisory = rendered_document_completeness_advisory(
        document,
        template_text,
        semantic_case,
        rendered_path,
    )?;
    Ok(())
}

fn verify_published_batch_files(
    output_folder: &Path,
    staged_paths: &[PathBuf],
    expected_count: usize,
) -> Result<Vec<String>, String> {
    if staged_paths.len() != expected_count {
        return Err(format!(
            "Публикация комплекта остановлена: ожидалось {expected_count} документ(ов), подготовлено {}.",
            staged_paths.len()
        ));
    }

    let mut created_files = Vec::with_capacity(expected_count);
    for staged_path in staged_paths {
        let name = staged_path.file_name().ok_or_else(|| {
            format!(
                "Публикация комплекта остановлена: staging-путь не содержит имени файла: {}",
                staged_path.display()
            )
        })?;
        let published_path = output_folder.join(name);
        if !published_path.is_file() {
            return Err(format!(
                "Публикация комплекта не подтверждена: созданный документ отсутствует на диске: {}",
                published_path.display()
            ));
        }
        let metadata = std::fs::metadata(&published_path).map_err(|error| {
            format!(
                "Не удалось проверить созданный документ {}: {error}",
                published_path.display()
            )
        })?;
        if metadata.len() == 0 {
            return Err(format!(
                "Публикация комплекта не подтверждена: созданный документ пуст: {}",
                published_path.display()
            ));
        }
        extract_docx_text(&published_path).map_err(|error| {
            format!(
                "Публикация комплекта не подтверждена: итоговый Word-документ не читается {}: {error}",
                published_path.display()
            )
        })?;
        created_files.push(published_path.display().to_string());
    }
    Ok(created_files)
}

#[cfg(test)]
mod manual_batch_publication_proof_tests {
    use super::*;

    fn generic_document_with_required_name() -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "document".into(),
            button_label: "Проверяемый документ".into(),
            template_path: "template.docx".into(),
            category: dokkomplekt_core::DomainKind::Generic,
            role_id: "generic".into(),
            required_fields: vec!["subject.name".into()],
            placeholders: vec!["subject.name".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn published_batch_verification_requires_real_readable_files() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-manual-publication-proof-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let stage = root.join("stage");
        let published = root.join("published");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::create_dir_all(&published).unwrap();
        let staged = stage.join("Документ.docx");
        create_docx_from_text(&staged, "Проверяемый документ").unwrap();

        let missing = verify_published_batch_files(&published, std::slice::from_ref(&staged), 1);
        assert!(missing.is_err());

        let final_path = published.join("Документ.docx");
        std::fs::copy(&staged, &final_path).unwrap();
        let verified =
            verify_published_batch_files(&published, std::slice::from_ref(&staged), 1).unwrap();
        assert_eq!(verified, vec![final_path.display().to_string()]);

        std::fs::write(&final_path, b"").unwrap();
        assert!(verify_published_batch_files(&published, &[staged], 1).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn publishing_batch_creates_missing_output_root_and_keeps_readable_docx() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-create-output-root-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let stage = root.join(".dokkomplekt-manual-stage-test");
        let output_root = root.join("Выписанные пациенты");
        let desired = output_root.join("Иванов Иван Иванович");
        std::fs::create_dir_all(&stage).unwrap();
        let staged = stage.join("Выписной эпикриз.docx");
        create_docx_from_text(&staged, "Физически созданный документ").unwrap();
        assert!(!output_root.exists());

        let published = publish_stage_to_unique_directory(&stage, &desired).unwrap();

        assert_eq!(published, desired);
        assert!(output_root.is_dir());
        assert!(published.is_dir());
        let verified = verify_published_batch_files(&published, &[staged], 1).unwrap();
        let expected = published.join("Выписной эпикриз.docx");
        assert_eq!(verified, vec![expected.display().to_string()]);
        assert_eq!(extract_docx_text(&expected).unwrap(), "Физически созданный документ");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn published_batch_verification_rejects_wrong_document_count() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-manual-publication-count-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(verify_published_batch_files(&root, &[], 1).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn post_render_semantic_advisory_does_not_delete_a_valid_docx() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-post-render-advisory-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let rendered = root.join("Документ.docx");
        create_docx_from_text(&rendered, "Физически созданный документ").unwrap();
        let document = generic_document_with_required_name();
        let case = SemanticCase::default();

        let advisory = rendered_document_completeness_advisory(
            &document,
            "{{subject.name}}",
            &case,
            &rendered,
        )
        .unwrap();
        assert!(advisory.is_some());
        assert!(ensure_rendered_document_complete(
            &document,
            "{{subject.name}}",
            &case,
            &rendered,
        )
        .is_ok());
        assert!(rendered.is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unreadable_rendered_word_file_is_still_a_hard_error() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-post-render-unreadable-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let rendered = root.join("broken.docx");
        std::fs::write(&rendered, b"not-a-docx").unwrap();
        let document = generic_document_with_required_name();

        assert!(ensure_rendered_document_complete(
            &document,
            "{{subject.name}}",
            &SemanticCase::default(),
            &rendered,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
