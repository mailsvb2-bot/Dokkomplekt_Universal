// Explicit output-collision publication policies.

fn planned_replacement_backup_path(desired: &Path, reservation_id: &str) -> PathBuf {
    let parent = desired.parent().unwrap_or_else(|| Path::new("."));
    let stem = sanitize_path_component(
        desired
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Комплект"),
    );
    let digest = hex::encode(Sha256::digest(reservation_id.as_bytes()));
    parent
        .join(".dokkomplekt-backups")
        .join(format!("{stem}.backup-{}", &digest[..24]))
}

fn publish_stage_replacing_with_backup(
    stage: &Path,
    desired: &Path,
    backup: &Path,
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
    let Some(lock) = try_acquire_publication_lock(&lock_path)? else {
        return Err(
            "Безопасная замена уже выполняется другим живым процессом; повторите операцию после её завершения."
                .into(),
        );
    };

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
        if backup.parent() != Some(backup_root.as_path()) {
            return Err("Путь резервной копии безопасной замены вышел за допустимый backup-каталог.".into());
        }
        if backup.exists() {
            return Err("Плановая резервная копия безопасной замены уже существует; публикация остановлена.".into());
        }

        std::fs::rename(desired, backup).map_err(|error| {
            format!(
                "Не удалось сначала сохранить существующий комплект в резервную копию: {error}"
            )
        })?;
        match std::fs::rename(stage, desired) {
            Ok(()) => Ok((desired.to_path_buf(), Some(backup.to_path_buf()))),
            Err(publish_error) => {
                match std::fs::rename(backup, desired) {
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
    result
}

fn rollback_unverified_publication(
    output_folder: &Path,
    backup_folder: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    let parent = output_folder.parent().unwrap_or_else(|| Path::new("."));
    let mut quarantined = None;
    if output_folder.exists() {
        let failed_root = parent.join(".dokkomplekt-failed");
        std::fs::create_dir_all(&failed_root)
            .map_err(|error| format!("Не удалось создать карантин для непроверенного комплекта: {error}"))?;
        let stem = sanitize_path_component(
            output_folder
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Комплект"),
        );
        let failed = failed_root.join(format!(
            "{stem}.failed-{}-{}",
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Uuid::new_v4()
        ));
        std::fs::rename(output_folder, &failed).map_err(|error| {
            format!(
                "Не удалось убрать непроверенный комплект из пользовательской папки: {error}"
            )
        })?;
        quarantined = Some(failed);
    }

    if let Some(backup) = backup_folder {
        if !backup.is_dir() {
            return Err(format!(
                "Резервная копия предыдущего комплекта недоступна для восстановления: {}",
                backup.display()
            ));
        }
        if output_folder.exists() {
            return Err(format!(
                "Нельзя восстановить предыдущий комплект: пользовательский путь всё ещё занят: {}",
                output_folder.display()
            ));
        }
        std::fs::rename(backup, output_folder).map_err(|error| {
            format!(
                "Непроверенный комплект убран, но предыдущую версию не удалось восстановить из {}: {error}",
                backup.display()
            )
        })?;
    }
    Ok(quarantined)
}


fn recover_unverified_batch_publication(
    app: &tauri::AppHandle,
    permit: &GenerationPermit,
    output_folder: &Path,
    backup_folder: Option<&Path>,
    verification_error: String,
) -> String {
    // Files can be restored for the user after a failed read-back, but generated
    // artifacts have already crossed the accounting/counter boundary. Never
    // refund usage or counter reservations after this point: the quarantined
    // documents still exist and their numbers must not be issued again.
    let rollback = rollback_unverified_publication(output_folder, backup_folder);
    let accounting = commit_generation_access(app, permit);
    let receipt_cleanup = if accounting.is_ok() {
        Some(generation_publication::abort_prepared_publication(app, permit))
    } else {
        None
    };
    let quarantine_path = rollback
        .as_ref()
        .ok()
        .and_then(|value| value.as_ref())
        .map(|path| path.display().to_string());
    let rollback_error = rollback.as_ref().err().cloned();
    let accounting_error = accounting.as_ref().err().cloned();
    let receipt_cleanup_error = receipt_cleanup
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .cloned();
    let _ = append_audit_event(
        app,
        "unverified_publication_quarantined",
        "",
        &serde_json::json!({
            "verification_error": verification_error,
            "quarantine_path": quarantine_path,
            "filesystem_rollback_error": rollback_error,
            "accounting_error": accounting_error,
            "receipt_cleanup_error": receipt_cleanup_error,
            "usage_refunded": false,
            "counters_refunded": false,
        }),
    );

    let rollback_note = match rollback.as_ref() {
        Ok(Some(path)) => format!(
            "Непроверенный комплект убран из пользовательской папки в карантин {}. Предыдущая версия восстановлена, если существовала.",
            path.display()
        ),
        Ok(None) => "Непроверенный пользовательский комплект удалять не пришлось; предыдущая версия восстановлена, если существовала.".to_string(),
        Err(error) => format!("КРИТИЧЕСКАЯ ОШИБКА файлового восстановления: {error}"),
    };
    let accounting_note = match accounting.as_ref() {
        Ok(()) => "Расход генерации и зарезервированные номера сохранены, чтобы исключить бесплатный или повторный выпуск.".to_string(),
        Err(error) => format!(
            "Учёт расхода не удалось дофинализировать ({error}); защищённая pre-publication квитанция сохранена для восстановления, возврат лимита не выполнялся."
        ),
    };
    let receipt_note = receipt_cleanup
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(|error| format!(
            " Pre-publication квитанцию после фиксации расхода удалить не удалось ({error}); она оставлена как дополнительный recovery guard."
        ))
        .unwrap_or_default();
    format!(
        "{verification_error} Публикация не признана успешной. {rollback_note} {accounting_note}{receipt_note}"
    )
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

        let planned_backup = planned_replacement_backup_path(&desired, "test-reservation");
        let (published, backup) =
            publish_stage_replacing_with_backup(&stage, &desired, &planned_backup).unwrap();
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

        let planned_backup = planned_replacement_backup_path(&desired, "test-reservation-new");
        let (published, backup) =
            publish_stage_replacing_with_backup(&stage, &desired, &planned_backup).unwrap();
        assert_eq!(published, desired);
        assert!(backup.is_none());
        assert_eq!(std::fs::read_to_string(desired.join("new.txt")).unwrap(), "new");
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn failed_new_version_is_quarantined_outside_user_visible_folder() {
        let root = temp_root("rollback-unverified-version");
        let desired = root.join("Комплект");
        std::fs::create_dir_all(&desired).unwrap();
        std::fs::write(desired.join("broken.docx"), b"broken").unwrap();

        let quarantined = rollback_unverified_publication(&desired, None)
            .unwrap()
            .expect("failed publication must be quarantined");
        assert!(!desired.exists());
        assert!(quarantined.is_dir());
        assert_eq!(std::fs::read(quarantined.join("broken.docx")).unwrap(), b"broken");
        assert_eq!(quarantined.parent().unwrap(), root.join(".dokkomplekt-failed"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_replacement_restores_previous_directory_from_backup() {
        let root = temp_root("rollback-unverified-replace");
        let desired = root.join("Комплект");
        let backup = root.join(".dokkomplekt-backups").join("Комплект.backup-test");
        std::fs::create_dir_all(&desired).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(desired.join("new-broken.docx"), b"broken").unwrap();
        std::fs::write(backup.join("old-good.docx"), b"old").unwrap();

        let quarantined = rollback_unverified_publication(&desired, Some(&backup))
            .unwrap()
            .expect("failed replacement must be quarantined");
        assert!(desired.is_dir());
        assert_eq!(std::fs::read(desired.join("old-good.docx")).unwrap(), b"old");
        assert!(!backup.exists());
        assert_eq!(std::fs::read(quarantined.join("new-broken.docx")).unwrap(), b"broken");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stale_dead_process_publication_lock_is_reclaimed() {
        let root = temp_root("stale-publication-lock");
        std::fs::create_dir_all(&root).unwrap();
        let lock_path = root.join("publication.lock");
        std::fs::write(
            &lock_path,
            format!("host={}\npid=4294967294\n", processing_lock_host_id()),
        )
        .unwrap();

        let lock = try_acquire_publication_lock(&lock_path)
            .unwrap()
            .expect("dead-process lock must be reclaimed");
        assert_eq!(publication_lock_pid(&lock_path), Some(std::process::id()));
        drop(lock);
        assert!(!lock_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn live_process_publication_lock_is_not_stolen() {
        let root = temp_root("live-publication-lock");
        std::fs::create_dir_all(&root).unwrap();
        let lock_path = root.join("publication.lock");
        std::fs::write(
            &lock_path,
            format!(
                "host={}\npid={}\n",
                processing_lock_host_id(),
                std::process::id()
            ),
        )
        .unwrap();

        assert!(try_acquire_publication_lock(&lock_path).unwrap().is_none());
        assert!(lock_path.exists());
        std::fs::remove_file(&lock_path).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn fresh_foreign_host_publication_lock_is_not_stolen() {
        let root = temp_root("foreign-publication-lock");
        std::fs::create_dir_all(&root).unwrap();
        let lock_path = root.join("publication.lock");
        std::fs::write(&lock_path, "host=other-machine\npid=4294967294\n").unwrap();

        assert!(try_acquire_publication_lock(&lock_path).unwrap().is_none());
        assert!(lock_path.exists());
        std::fs::remove_file(&lock_path).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dropping_old_lock_does_not_delete_replacement_lock() {
        let root = temp_root("publication-lock-token");
        std::fs::create_dir_all(&root).unwrap();
        let lock_path = root.join("publication.lock");
        let lock = try_acquire_publication_lock(&lock_path)
            .unwrap()
            .expect("lock must be acquired");
        std::fs::remove_file(&lock_path).unwrap();
        std::fs::write(
            &lock_path,
            format!(
                "host={}\npid={}\ntoken=replacement\n",
                processing_lock_host_id(),
                std::process::id()
            ),
        )
        .unwrap();

        drop(lock);
        assert!(lock_path.exists());
        std::fs::remove_file(&lock_path).unwrap();
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
    fn publishing_batch_creates_desktop_root_patient_subfolder_and_all_real_docx() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-create-output-root-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let desktop = root.join("Desktop");
        let stage = desktop.join(".dokkomplekt-manual-stage-test");
        let output_root = canonical_default_output_root_under(&desktop);
        std::fs::create_dir_all(&stage).unwrap();

        let mut case = SemanticCase::default();
        dokkomplekt_core::set_user_value(&mut case, "subject.name", "Иванов Иван Иванович");
        dokkomplekt_core::set_user_value(&mut case, "medical.admission_date", "10.05.2026");
        dokkomplekt_core::set_user_value(&mut case, "medical.discharge_date", "13.05.2026");
        let output_plan = dokkomplekt_core::plan_output_paths(
            &output_root,
            &case,
            &[
                dokkomplekt_core::FolderNamePart::FullSubjectName,
                dokkomplekt_core::FolderNamePart::AdmissionAndDischargeDates,
            ],
            &["Первичный осмотр".into(), "Выписной эпикриз".into()],
        );
        let desired = output_plan.patient_folder;
        assert_eq!(
            desired,
            output_root.join("Иванов Иван Иванович 10.05.2026 - 13.05.2026")
        );

        let primary = stage.join("Первичный осмотр.docx");
        let discharge = stage.join("Выписной эпикриз.docx");
        create_docx_from_text(
            &primary,
            "Первичный осмотр\nИванов Иван Иванович\n10.05.2026",
        )
        .unwrap();
        create_docx_from_text(
            &discharge,
            "Выписной эпикриз\nИванов Иван Иванович\n13.05.2026",
        )
        .unwrap();
        assert!(!output_root.exists());

        let published = publish_stage_to_unique_directory(&stage, &desired).unwrap();

        assert_eq!(published, desired);
        assert!(output_root.is_dir());
        assert!(published.is_dir());
        let staged = vec![primary, discharge];
        let verified = verify_published_batch_files(&published, &staged, 2).unwrap();
        let expected_primary = published.join("Первичный осмотр.docx");
        let expected_discharge = published.join("Выписной эпикриз.docx");
        assert_eq!(
            verified,
            vec![
                expected_primary.display().to_string(),
                expected_discharge.display().to_string(),
            ]
        );
        assert!(extract_docx_text(&expected_primary)
            .unwrap()
            .contains("10.05.2026"));
        assert!(extract_docx_text(&expected_discharge)
            .unwrap()
            .contains("13.05.2026"));
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
