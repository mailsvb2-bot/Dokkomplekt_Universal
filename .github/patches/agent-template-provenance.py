from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one marker, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


doc = Path("src-tauri/src/subsystems/document_commands.rs")
main = Path("src-tauri/src/main.rs")
docs = Path("docs/CRASH_CONSISTENCY.md")

# 1. Published archive bytes are authoritative across launches. Verify the stored
# record before rebinding a loaded pack, and keep the pure path mutation small/testable.
marker = '''#[derive(Debug, Deserialize)]
struct RegisterLearnedTemplateRequest {
'''
helper = r'''fn verify_published_template_version_file(
    path: &Path,
    record: &TemplateVersionRecord,
) -> Result<(), String> {
    let (_, _, actual_sha256) = file_content_signature(path)?;
    if actual_sha256 != record.template_sha256 {
        return Err(format!(
            "Опубликованная версия шаблона {} повреждена или изменена: ожидался SHA-256 {}, получен {}.",
            record.version_number, record.template_sha256, actual_sha256
        ));
    }
    Ok(())
}

fn bind_document_to_published_template(
    document: &mut DocumentTemplateSpec,
    record: &TemplateVersionRecord,
) -> bool {
    if document.template_path == record.template_path {
        return false;
    }
    document.template_path = record.template_path.clone();
    true
}

fn bind_loaded_pack_to_published_template_versions(
    app: &tauri::AppHandle,
    repo: &LocalRepository,
    pack: &mut DocumentPack,
) -> Result<usize, String> {
    let mut rebound = 0usize;
    for document in &mut pack.documents {
        let Some(record) = repo
            .list_template_versions(&document.id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|version| version.status == "published")
        else {
            continue;
        };
        let archived_path = resolve_user_path(app, &record.template_path)?;
        verify_published_template_version_file(&archived_path, &record)?;
        rebound += usize::from(bind_document_to_published_template(document, &record));
    }
    Ok(rebound)
}

#[derive(Debug, Deserialize)]
struct RegisterLearnedTemplateRequest {
'''
replace_once(doc, marker, helper)

# 2. Newly registered templates publish the immutable archive path into the active pack.
replace_once(
    doc,
    '''    let draft = prepare_template_version_draft(
        &app,
        document_id,
        template_snapshot.path(),
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    template_snapshot.ensure_current()?;
''',
    '''    let draft = prepare_template_version_draft(
        &app,
        document_id,
        template_snapshot.path(),
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    document.template_path = draft.template_path.clone();
    template_snapshot.ensure_current()?;
''',
)

# 3. Bulk first-run confirmation also binds every active document to its archived bytes.
replace_once(
    doc,
    '''    let incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
''',
    '''    let mut incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
''',
)
replace_once(
    doc,
    '''    template_snapshot::ensure_all_current(&template_snapshots)?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &drafts, |pack| {
        merge_document_pack(pack, incoming);
''',
    '''    template_snapshot::ensure_all_current(&template_snapshots)?;
    for draft in &drafts {
        let document = incoming
            .documents
            .iter_mut()
            .find(|document| document.id == draft.document_id)
            .ok_or_else(|| format!("Не найден документ {} для привязки опубликованной версии.", draft.document_id))?;
        document.template_path = draft.template_path.clone();
    }
    let (result, _) = publish_pack_with_template_versions(&app, &state, &drafts, |pack| {
        merge_document_pack(pack, incoming);
''',
)

# 4. Regression comparison must verify the previous immutable archive, and public
# checks snapshot the candidate so the report is about one stable candidate revision.
old_check = r'''#[tauri::command]
fn check_template_regression(
    req: CheckTemplateRegressionRequest,
    app: tauri::AppHandle,
) -> Result<Option<TemplateRegressionReport>, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let Some(previous) = repo
        .list_template_versions(req.document_id.trim())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|version| version.status == "published")
    else {
        return Ok(None);
    };
    let previous_path = resolve_user_path(&app, &previous.template_path)?;
    let candidate_path = resolve_user_path(&app, &req.candidate_template_path)?;
    compare_docx_structures(&previous_path, &candidate_path)
        .map(Some)
        .map_err(|error| error.to_string())
}
'''
new_check = r'''fn compare_candidate_to_published_template(
    app: &tauri::AppHandle,
    document_id: &str,
    candidate_path: &Path,
) -> Result<Option<TemplateRegressionReport>, String> {
    let repo = repository_for(&default_state_db_path(app)?)?;
    let Some(previous) = repo
        .list_template_versions(document_id.trim())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|version| version.status == "published")
    else {
        return Ok(None);
    };
    let previous_path = resolve_user_path(app, &previous.template_path)?;
    verify_published_template_version_file(&previous_path, &previous)?;
    compare_docx_structures(&previous_path, candidate_path)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn check_template_regression(
    req: CheckTemplateRegressionRequest,
    app: tauri::AppHandle,
) -> Result<Option<TemplateRegressionReport>, String> {
    let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.candidate_template_path,
        "кандидат новой версии шаблона",
    )?;
    let result = compare_candidate_to_published_template(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
    )?;
    candidate_snapshot.ensure_current()?;
    Ok(result)
}
'''
replace_once(doc, old_check, new_check)

# 5. Update path had an independent A->B race after #70: regression, extraction,
# hashing and archive copy reopened the live path. Capture once and bind the resulting
# active document to the archived draft path before the atomic DB publication.
old_update = r'''#[tauri::command]
fn update_document_template(
    req: UpdateDocumentTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let path = resolve_user_path(&app, &req.template_path)?;
    let regression_report = check_template_regression(
        CheckTemplateRegressionRequest {
            document_id: req.document_id.clone(),
            candidate_template_path: req.template_path.clone(),
        },
        app.clone(),
    )?;
    if !req.acknowledge_regressions {
        if let Some(report) = regression_report.as_ref().filter(|report| report.critical) {
            return Err(format!(
                "Обновление заблокировано Template Regression Gate: {}",
                report
                    .issues
                    .iter()
                    .filter(|issue| matches!(&issue.severity, dokkomplekt_docx::TemplateRegressionSeverity::Critical))
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    let text = extract_docx_text(&path).map_err(|error| error.to_string())?;
    let mut updated = dokkomplekt_core::create_button_from_template_text(
        &text,
        &req.document_id,
        &path.display().to_string(),
        None,
    );
    if updated.is_static_copy {
        return Err("Размеченная копия не содержит ни одного поля {{field.id}}.".into());
    }
    let (_, _, template_sha256) = file_content_signature(&path)?;
    let draft = prepare_template_version_draft(
        &app,
        &req.document_id,
        &path,
        &template_sha256,
        "Шаблон опубликован после проверенной разметки.",
    )?;
    let (result, versions) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        let existing = pack
            .documents
            .iter_mut()
            .find(|document| document.id == req.document_id)
            .ok_or_else(|| "Документ для обновления не найден.".to_string())?;
        updated.button_label = existing.button_label.clone();
        updated.category = existing.category.clone();
        updated.role_id = existing.role_id.clone();
        updated.popup_fields = existing.popup_fields.clone();
        updated.popup_configured = existing.popup_configured;
        updated
            .required_fields
            .extend(existing.required_fields.iter().cloned());
        updated.required_fields.extend(
            existing
                .popup_fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.field_id.clone()),
        );
        updated.required_fields.sort();
        updated.required_fields.dedup();
        *existing = updated;
        Ok(())
    })?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| "Атомарная публикация не вернула версию шаблона.".to_string())?;
    let _ = append_audit_event(
        &app,
        "template_version_published",
        &template_sha256,
        &serde_json::json!({
            "document_id": req.document_id,
            "version_id": version.version_id,
            "version_number": version.version_number,
            "regression_report": &regression_report,
            "regressions_acknowledged": req.acknowledge_regressions,
        }),
    );
    Ok(result)
}
'''
new_update = r'''#[tauri::command]
fn update_document_template(
    req: UpdateDocumentTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(
        &app,
        &req.template_path,
        "новая версия шаблона",
    )?;
    let regression_report = compare_candidate_to_published_template(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
    )?;
    if !req.acknowledge_regressions {
        if let Some(report) = regression_report.as_ref().filter(|report| report.critical) {
            return Err(format!(
                "Обновление заблокировано Template Regression Gate: {}",
                report
                    .issues
                    .iter()
                    .filter(|issue| matches!(&issue.severity, dokkomplekt_docx::TemplateRegressionSeverity::Critical))
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    let text = extract_docx_text(candidate_snapshot.path()).map_err(|error| error.to_string())?;
    let mut updated = dokkomplekt_core::create_button_from_template_text(
        &text,
        &req.document_id,
        &candidate_snapshot.path().display().to_string(),
        None,
    );
    if updated.is_static_copy {
        return Err("Размеченная копия не содержит ни одного поля {{field.id}}.".into());
    }
    let template_sha256 = candidate_snapshot.sha256().to_string();
    let draft = prepare_template_version_draft(
        &app,
        &req.document_id,
        candidate_snapshot.path(),
        &template_sha256,
        "Шаблон опубликован после проверенной разметки.",
    )?;
    updated.template_path = draft.template_path.clone();
    candidate_snapshot.ensure_current()?;
    let (result, versions) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        let existing = pack
            .documents
            .iter_mut()
            .find(|document| document.id == req.document_id)
            .ok_or_else(|| "Документ для обновления не найден.".to_string())?;
        updated.button_label = existing.button_label.clone();
        updated.category = existing.category.clone();
        updated.role_id = existing.role_id.clone();
        updated.popup_fields = existing.popup_fields.clone();
        updated.popup_configured = existing.popup_configured;
        updated
            .required_fields
            .extend(existing.required_fields.iter().cloned());
        updated.required_fields.extend(
            existing
                .popup_fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.field_id.clone()),
        );
        updated.required_fields.sort();
        updated.required_fields.dedup();
        *existing = updated;
        Ok(())
    })?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| "Атомарная публикация не вернула версию шаблона.".to_string())?;
    let _ = append_audit_event(
        &app,
        "template_version_published",
        &template_sha256,
        &serde_json::json!({
            "document_id": req.document_id,
            "version_id": version.version_id,
            "version_number": version.version_number,
            "regression_report": &regression_report,
            "regressions_acknowledged": req.acknowledge_regressions,
        }),
    );
    Ok(result)
}
'''
replace_once(doc, old_update, new_update)

# 6. Rollback already uses archived bytes; share the same SHA invariant.
replace_once(
    doc,
    '''    let path = resolve_user_path(&app, &record.template_path)?;
    let (_, _, actual_sha256) = file_content_signature(&path)?;
    if actual_sha256 != record.template_sha256 {
        return Err("Архивная версия шаблона была изменена после публикации; rollback заблокирован по SHA-256.".into());
    }
''',
    '''    let path = resolve_user_path(&app, &record.template_path)?;
    verify_published_template_version_file(&path, &record)?;
''',
)

# 7. Rebind legacy packs before they become live. This preserves the existing
# all-or-nothing restore guarantee and persists the migration only for the real
# default commercial state database, never for a user-selected backup file.
replace_once(
    doc,
    '''fn load_state_from(
    db_path: &Path,
    state: &AppState,
    load_commercial_state: bool,
) -> Result<(), String> {
''',
    '''fn load_state_from(
    app: &tauri::AppHandle,
    db_path: &Path,
    state: &AppState,
    load_commercial_state: bool,
) -> Result<(), String> {
''',
)
replace_once(
    doc,
    '''    let loaded_case = repo.load_case("current").map_err(|error| error.to_string())?;
    let loaded_pack = repo.load_pack("default").map_err(|error| error.to_string())?;
    let loaded_license = if load_commercial_state {
''',
    '''    let loaded_case = repo.load_case("current").map_err(|error| error.to_string())?;
    let mut loaded_pack = repo.load_pack("default").map_err(|error| error.to_string())?;
    let loaded_license = if load_commercial_state {
''',
)
replace_once(
    doc,
    '''    if let Some(Some(document)) = loaded_license.as_ref() {
        verify_license_document_now(document, &trusted_license_key()?)
            .map_err(|error| format!("Сохранённая лицензия недействительна: {error}"))?;
    }

    let mut case_guard = state
''',
    '''    if let Some(Some(document)) = loaded_license.as_ref() {
        verify_license_document_now(document, &trusted_license_key()?)
            .map_err(|error| format!("Сохранённая лицензия недействительна: {error}"))?;
    }
    if let Some(pack) = loaded_pack.as_mut() {
        let rebound = bind_loaded_pack_to_published_template_versions(app, &repo, pack)?;
        if rebound > 0 && load_commercial_state {
            repo.save_pack(pack).map_err(|error| error.to_string())?;
        }
    }

    let mut case_guard = state
''',
)
replace_once(
    doc,
    '''    load_state_from(&db_path, &state, false)?;
''',
    '''    load_state_from(&app, &db_path, &state, false)?;
''',
)
replace_once(
    main,
    '''                    if let Err(error) = load_state_from(&db_path, &state, true) {
''',
    '''                    if let Err(error) = load_state_from(&handle, &db_path, &state, true) {
''',
)

# 8. Add focused Rust regressions for the persistent binding invariant.
insert_before = '''#[derive(Debug, Deserialize)]
struct DiaryPlanRequest {
'''
tests = r'''#[cfg(test)]
mod published_template_binding_tests {
    use super::*;

    fn record(path: &Path, sha256: &str) -> TemplateVersionRecord {
        TemplateVersionRecord {
            version_id: "version-1".into(),
            document_id: "invoice".into(),
            version_number: 1,
            template_path: path.display().to_string(),
            template_sha256: sha256.into(),
            note: "test".into(),
            status: "published".into(),
            created_at: "2026-08-08T00:00:00Z".into(),
        }
    }

    fn document(path: &str) -> DocumentTemplateSpec {
        DocumentTemplateSpec {
            id: "invoice".into(),
            button_label: "Счёт".into(),
            template_path: path.into(),
            category: DomainKind::Generic,
            role_id: "generic".into(),
            required_fields: Vec::new(),
            placeholders: vec!["invoice.number".into()],
            is_static_copy: false,
            popup_fields: Vec::new(),
            popup_configured: false,
        }
    }

    #[test]
    fn active_document_binding_replaces_mutable_live_path_with_published_archive() {
        let archive = PathBuf::from("C:/app-data/template-versions/invoice/hash.docx");
        let version = record(&archive, &"a".repeat(64));
        let mut document = document("C:/Users/user/Documents/invoice.docx");
        assert!(bind_document_to_published_template(&mut document, &version));
        assert_eq!(document.template_path, version.template_path);
        assert!(!bind_document_to_published_template(&mut document, &version));
    }

    #[test]
    fn published_template_sha_verification_rejects_archive_mutation() {
        let root = std::env::temp_dir().join(format!(
            "dkk-published-template-binding-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("template.docx");
        std::fs::write(&archive, b"published-template-v1").unwrap();
        let sha256 = hex::encode(Sha256::digest(b"published-template-v1"));
        let version = record(&archive, &sha256);
        verify_published_template_version_file(&archive, &version).unwrap();
        std::fs::write(&archive, b"tampered-template-v2").unwrap();
        assert!(verify_published_template_version_file(&archive, &version).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[derive(Debug, Deserialize)]
struct DiaryPlanRequest {
'''
replace_once(doc, insert_before, tests)

# 9. Document the cross-run invariant.
doc_text = docs.read_text(encoding="utf-8").rstrip()
section = r'''

## Published template identity across launches

A confirmed template is no longer executed from the mutable user-selected source path. Registration, bulk confirmation, update and rollback publish content-addressed bytes under `app_data/template-versions/<document_id>/` and bind the active `DocumentTemplateSpec.template_path` to that archived version. Editing or replacing the original DOCX after publication therefore cannot silently change later generations.

Existing installations are migrated during state restore before the loaded pack becomes live: every document with a published version is rebound to the latest published archive only after the archive bytes match the stored SHA-256. A missing or modified published archive fails closed instead of falling back to the mutable original. User-selected backup databases are rebound in memory but are not rewritten as a side effect of inspection/recovery.

Template updates use one immutable candidate snapshot for regression comparison, placeholder extraction, SHA-256 and archive copy, then revalidate the live candidate before atomic publication. This closes the remaining update-specific A→B race that could otherwise approve one revision and publish another.
'''
if "## Published template identity across launches" not in doc_text:
    docs.write_text(doc_text + section + "\n", encoding="utf-8")
else:
    docs.write_text(doc_text + "\n", encoding="utf-8")

contract = Path("tests/test_published_template_provenance_contract.py")
contract.write_text(
    '''from pathlib import Path\n\n\ndef test_active_templates_bind_to_published_archives_and_update_uses_one_snapshot():\n    source = Path("src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")\n    main = Path("src-tauri/src/main.rs").read_text(encoding="utf-8")\n\n    assert "bind_loaded_pack_to_published_template_versions" in source\n    assert "verify_published_template_version_file(&archived_path, &record)?" in source\n    assert "document.template_path = draft.template_path.clone();" in source\n    assert "let mut incoming = create_pack_from_confirmations" in source\n    assert "document.template_path = draft.template_path.clone();" in source\n    assert "let candidate_snapshot = template_snapshot::TemplateSnapshot::capture(" in source\n    assert "compare_candidate_to_published_template(" in source\n    assert "candidate_snapshot.path()," in source\n    assert "let template_sha256 = candidate_snapshot.sha256().to_string();" in source\n    assert "updated.template_path = draft.template_path.clone();" in source\n    assert "candidate_snapshot.ensure_current()?;" in source\n    assert "load_state_from(\\n    app: &tauri::AppHandle," in source\n    assert "bind_loaded_pack_to_published_template_versions(app, &repo, pack)?" in source\n    assert "load_state_from(&app, &db_path, &state, false)?;" in source\n    assert "load_state_from(&handle, &db_path, &state, true)" in main\n\n\ndef test_mutable_live_path_is_not_reintroduced_by_registration_update_or_rollback():\n    source = Path("src-tauri/src/subsystems/document_commands.rs").read_text(encoding="utf-8")\n    assert "updated.template_path = draft.template_path.clone();" in source\n    assert "verify_published_template_version_file(&path, &record)?;" in source\n    assert "compare_docx_structures(&previous_path, candidate_path)" in source\n''',
    encoding="utf-8",
)
