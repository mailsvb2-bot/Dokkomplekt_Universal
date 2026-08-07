from pathlib import Path


def replace_between(text: str, start: str, end: str, replacement: str) -> str:
    i = text.index(start)
    j = text.index(end, i)
    return text[:i] + replacement + text[j:]


# --- storage: make desktop snapshot + template-version publication one SQLite transaction ---
storage_path = Path("crates/dokkomplekt-storage/src/lib.rs")
storage = storage_path.read_text(encoding="utf-8")

record_block = '''#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemplateVersionRecord {
    pub version_id: String,
    pub document_id: String,
    pub version_number: u32,
    pub template_path: String,
    pub template_sha256: String,
    pub note: String,
    pub status: String,
    pub created_at: String,
}
'''
if storage.count(record_block) != 1:
    raise SystemExit("TemplateVersionRecord marker mismatch")
storage = storage.replace(
    record_block,
    record_block
    + '''\n#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateVersionDraft {
    pub document_id: String,
    pub template_path: String,
    pub template_sha256: String,
    pub note: String,
}
''',
    1,
)

atomic_method = '''    /// Atomically publishes the complete desktop snapshot together with all
    /// template-version records that make the candidate pack auditable.
    ///
    /// Archive files are prepared by the caller before this transaction. SQLite is
    /// the publication boundary: after a crash, callers can observe either the old
    /// pack/version set or the new pack/version set, never a mixture of both.
    pub fn save_desktop_snapshot_with_template_versions<T: serde::Serialize + ?Sized>(
        &mut self,
        case_id: &str,
        pack_id: &str,
        case: &SemanticCase,
        pack: &DocumentPack,
        state_key: &str,
        state_value: &T,
        versions: &[TemplateVersionDraft],
    ) -> StorageResult<Vec<TemplateVersionRecord>> {
        let case_json = serde_json::to_string_pretty(case)?;
        let case_stored = self.encode_sensitive(&case_json)?;
        let pack_json = serde_json::to_string_pretty(pack)?;
        let pack_stored = self.encode_sensitive(&pack_json)?;
        let state_json = serde_json::to_string(state_value)?;
        let state_stored = self.encode_sensitive(&state_json)?;

        let mut prepared = Vec::with_capacity(versions.len());
        for draft in versions {
            if draft.document_id.trim().is_empty() {
                return Err(StorageError::Crypto("document_id cannot be empty".into()));
            }
            if draft.template_sha256.len() != 64
                || !draft
                    .template_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(StorageError::Crypto(
                    "template_sha256 must be lowercase SHA-256".into(),
                ));
            }
            prepared.push((
                draft.clone(),
                random_record_id("tpl")?,
                self.encode_sensitive(&draft.template_path)?,
                self.encode_sensitive(&draft.note)?,
            ));
        }

        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO semantic_cases(case_id, json) VALUES (?1, ?2) ON CONFLICT(case_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![case_id, case_stored],
        )?;
        tx.execute(
            "INSERT INTO document_packs(pack_id, json) VALUES (?1, ?2) ON CONFLICT(pack_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![pack_id, pack_stored],
        )?;
        tx.execute(
            "INSERT INTO app_state(state_key, json) VALUES (?1, ?2) ON CONFLICT(state_key) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![state_key, state_stored],
        )?;

        let mut published_ids = Vec::with_capacity(prepared.len());
        for (draft, version_id, encrypted_path, encrypted_note) in prepared {
            let current: Option<(String, String)> = tx
                .query_row(
                    "SELECT version_id,template_sha256 FROM template_versions WHERE document_id=?1 AND status='published' ORDER BY version_number DESC LIMIT 1",
                    params![draft.document_id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((current_id, current_sha256)) = current {
                if current_sha256 == draft.template_sha256 {
                    published_ids.push(current_id);
                    continue;
                }
            }
            let next: i64 = tx.query_row(
                "SELECT COALESCE(MAX(version_number),0)+1 FROM template_versions WHERE document_id=?1",
                params![draft.document_id.as_str()],
                |row| row.get(0),
            )?;
            tx.execute(
                "UPDATE template_versions SET status='archived' WHERE document_id=?1 AND status='published'",
                params![draft.document_id.as_str()],
            )?;
            tx.execute(
                "INSERT INTO template_versions(version_id,document_id,version_number,template_path,template_sha256,note,status) VALUES (?1,?2,?3,?4,?5,?6,'published')",
                params![
                    version_id.as_str(),
                    draft.document_id.as_str(),
                    next,
                    encrypted_path,
                    draft.template_sha256.as_str(),
                    encrypted_note,
                ],
            )?;
            published_ids.push(version_id);
        }
        tx.commit()?;

        published_ids
            .into_iter()
            .map(|version_id| {
                self.template_version_by_id(&version_id)?.ok_or_else(|| {
                    StorageError::Crypto("atomically published template version disappeared".into())
                })
            })
            .collect()
    }

'''
marker = "    pub fn register_template_version(\n"
if storage.count(marker) != 1:
    raise SystemExit("register_template_version marker mismatch")
storage = storage.replace(marker, atomic_method + marker, 1)

test_marker = '''    #[test]
    fn template_registry_versions_and_archives_previous_publication() {
'''
if storage.count(test_marker) != 1:
    raise SystemExit("template test marker mismatch")
new_tests = '''    #[test]
    fn desktop_snapshot_and_template_versions_publish_as_one_transaction() {
        let path = temp_db("template-atomic-publish");
        let mut repo = LocalRepository::open_with_key(&path, [21u8; 32]).unwrap();
        let case = SemanticCase::default();
        let old_pack = DocumentPack {
            pack_id: "default".into(),
            pack_name: "old".into(),
            documents: Vec::new(),
        };
        repo.save_case_and_pack_atomic("current", "default", &case, &old_pack)
            .unwrap();
        let candidate = DocumentPack {
            pack_id: "default".into(),
            pack_name: "candidate".into(),
            documents: Vec::new(),
        };
        let draft = TemplateVersionDraft {
            document_id: "invoice".into(),
            template_path: "C:/archive/invoice.docx".into(),
            template_sha256: "a".repeat(64),
            note: "atomic publish".into(),
        };
        let versions = repo
            .save_desktop_snapshot_with_template_versions(
                "current",
                "default",
                &case,
                &candidate,
                "license_document",
                &Option::<String>::None,
                &[draft],
            )
            .unwrap();
        assert_eq!(repo.load_pack("default").unwrap(), Some(candidate));
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].status, "published");
        assert_eq!(repo.list_template_versions("invoice").unwrap().len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_template_version_cannot_publish_candidate_pack() {
        let path = temp_db("template-atomic-reject");
        let mut repo = LocalRepository::open_with_key(&path, [22u8; 32]).unwrap();
        let case = SemanticCase::default();
        let old_pack = DocumentPack {
            pack_id: "default".into(),
            pack_name: "old".into(),
            documents: Vec::new(),
        };
        repo.save_case_and_pack_atomic("current", "default", &case, &old_pack)
            .unwrap();
        let candidate = DocumentPack {
            pack_id: "default".into(),
            pack_name: "must-not-publish".into(),
            documents: Vec::new(),
        };
        let invalid = TemplateVersionDraft {
            document_id: "invoice".into(),
            template_path: "C:/archive/invoice.docx".into(),
            template_sha256: "NOT-A-SHA".into(),
            note: "invalid".into(),
        };
        assert!(repo
            .save_desktop_snapshot_with_template_versions(
                "current",
                "default",
                &case,
                &candidate,
                "license_document",
                &Option::<String>::None,
                &[invalid],
            )
            .is_err());
        assert_eq!(repo.load_pack("default").unwrap(), Some(old_pack));
        assert!(repo.list_template_versions("invoice").unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }

'''
storage = storage.replace(test_marker, new_tests + test_marker, 1)
storage_path.write_text(storage, encoding="utf-8")


# --- AppState: serialize persistence operations so candidate publication cannot deadlock/race ---
main_path = Path("src-tauri/src/main.rs")
main = main_path.read_text(encoding="utf-8")
main = main.replace(
    "    CaseRunRecord, ClauseBlockRecord, CounterValue, LocalRepository, TemplateVersionRecord,\n    UsageReservation,\n",
    "    CaseRunRecord, ClauseBlockRecord, CounterValue, LocalRepository, TemplateVersionDraft,\n    TemplateVersionRecord, UsageReservation,\n",
    1,
)
main = main.replace(
    "    semantic_runtime: Mutex<Option<semantic_runtime::ManagedSemanticRuntime>>,\n    persistence_blocked: AtomicBool,\n",
    "    semantic_runtime: Mutex<Option<semantic_runtime::ManagedSemanticRuntime>>,\n    persistence_gate: Mutex<()>,\n    persistence_blocked: AtomicBool,\n",
    1,
)
main = main.replace(
    "            semantic_runtime: Mutex::new(None),\n            persistence_blocked: AtomicBool::new(false),\n",
    "            semantic_runtime: Mutex::new(None),\n            persistence_gate: Mutex::new(()),\n            persistence_blocked: AtomicBool::new(false),\n",
    1,
)
for fn_name in ["persist_state_to", "persist_default_state"]:
    needle = f"fn {fn_name}"
    start = main.index(needle)
    ensure = main.index("    ensure_persistence_available(state)?;", start)
    insertion = "    let _persistence_guard = state\n        .persistence_gate\n        .lock()\n        .map_err(|_| \"persistence gate lock failed\")?;\n"
    pos = ensure + len("    ensure_persistence_available(state)?;\n")
    if main[pos:pos + len(insertion)] != insertion:
        main = main[:pos] + insertion + main[pos:]
main_path.write_text(main, encoding="utf-8")


# --- document commands: archive first, then commit pack + version rows in one DB transaction ---
commands_path = Path("src-tauri/src/subsystems/document_commands.rs")
commands = commands_path.read_text(encoding="utf-8")

helper_marker = "#[derive(Debug, Deserialize)]\nstruct RegisterLearnedTemplateRequest {"
helper = '''fn publish_pack_with_template_versions<F>(
    app: &tauri::AppHandle,
    state: &AppState,
    drafts: &[TemplateVersionDraft],
    mutate: F,
) -> Result<(DocumentPack, Vec<TemplateVersionRecord>), String>
where
    F: FnOnce(&mut DocumentPack) -> Result<(), String>,
{
    ensure_persistence_available(state)?;
    let _persistence_guard = state
        .persistence_gate
        .lock()
        .map_err(|_| "persistence gate lock failed")?;
    let case = state
        .semantic_case
        .lock()
        .map_err(|_| "state lock failed")?
        .clone();
    let license = state
        .license_document
        .lock()
        .map_err(|_| "license state lock failed")?
        .clone();
    let mut pack_guard = state.pack.lock().map_err(|_| "state lock failed")?;
    let mut candidate = pack_guard.clone();
    mutate(&mut candidate)?;
    let path = default_state_db_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut repo = repository_for(&path)?;
    let versions = repo
        .save_desktop_snapshot_with_template_versions(
            "current",
            "default",
            &case,
            &candidate,
            "license_document",
            &license,
            drafts,
        )
        .map_err(|error| error.to_string())?;
    *pack_guard = candidate.clone();
    Ok((candidate, versions))
}

'''
if commands.count(helper_marker) != 1:
    raise SystemExit("learned-template helper marker mismatch")
commands = commands.replace(helper_marker, helper + helper_marker, 1)

learned_and_confirm = '''#[tauri::command]
fn register_learned_template(
    req: RegisterLearnedTemplateRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let document_id = req.document_id.trim();
    let button_label = req.button_label.trim();
    if document_id.is_empty()
        || !document_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("Укажите безопасный идентификатор документа.".into());
    }
    if button_label.is_empty() {
        return Err("Укажите название кнопки.".into());
    }
    let path = resolve_user_path(&app, &req.template_path)?;
    let text = extract_docx_text(&path).map_err(|error| error.to_string())?;
    let mut document = dokkomplekt_core::create_button_from_template_text(
        &text,
        document_id,
        &path.display().to_string(),
        Some(button_label),
    );
    if document.is_static_copy || document.placeholders.is_empty() {
        return Err("Обученная копия не содержит подтверждённых {{field.id}} и не может стать рабочей кнопкой.".into());
    }
    document.popup_fields = normalize_popup_fields(&document.popup_fields);
    let (_, _, template_sha256) = file_content_signature(&path)?;
    let draft = prepare_template_version_draft(
        &app,
        document_id,
        &path,
        &template_sha256,
        "Публикация шаблона после подтверждённого Template Intelligence Wizard.",
    )?;
    let (result, _) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        pack.documents.retain(|item| item.id != document_id);
        if pack
            .documents
            .iter()
            .any(|item| item.button_label.eq_ignore_ascii_case(button_label))
        {
            return Err("Кнопка с таким названием уже существует.".into());
        }
        pack.documents.push(document);
        pack.documents
            .sort_by(|left, right| left.button_label.cmp(&right.button_label));
        Ok(())
    })?;
    append_audit_event(
        &app,
        "learned_template_registered",
        &template_sha256,
        &serde_json::json!({
            "document_id": document_id,
            "button_label": button_label,
            "template_path": path.display().to_string(),
            "explicit_confirmation": true,
        }),
    )?;
    Ok(result)
}

#[derive(Debug, Deserialize)]
struct ConfirmTemplatesRequest {
    rows: Vec<TemplateConfirmationRow>,
}

#[tauri::command]
fn confirm_template_setup(
    req: ConfirmTemplatesRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    if req.rows.is_empty() {
        return Err("Выберите хотя бы один шаблон Word.".into());
    }
    if req
        .rows
        .iter()
        .any(|row| row.editable_button_label.trim().is_empty())
    {
        return Err("У каждого шаблона должно быть название кнопки.".into());
    }
    let incoming = create_pack_from_confirmations("incoming", "Новые шаблоны", &req.rows).pack;
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
        merge_document_pack(pack, incoming);
        Ok(())
    })?;
    Ok(result)
}

'''
commands = replace_between(
    commands,
    "#[tauri::command]\nfn register_learned_template(",
    "#[derive(Debug, Deserialize)]\nstruct RenameDocumentButtonRequest",
    learned_and_confirm,
)

update_fn = '''#[tauri::command]
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
commands = replace_between(
    commands,
    "#[tauri::command]\nfn update_document_template(",
    "fn archive_template_version_source(",
    update_fn,
)

prepare_helper = '''fn prepare_template_version_draft(
    app: &tauri::AppHandle,
    document_id: &str,
    source: &Path,
    template_sha256: &str,
    note: &str,
) -> Result<TemplateVersionDraft, String> {
    let archived_path =
        archive_template_version_source(app, document_id, source, template_sha256)?;
    Ok(TemplateVersionDraft {
        document_id: document_id.to_string(),
        template_path: archived_path.display().to_string(),
        template_sha256: template_sha256.to_string(),
        note: note.to_string(),
    })
}

'''
commands = replace_between(
    commands,
    "fn register_template_snapshot(",
    "#[derive(Debug, Deserialize)]\nstruct ListTemplateVersionsRequest",
    prepare_helper,
)

rollback_fn = '''#[tauri::command]
fn rollback_template_version(
    req: RollbackTemplateVersionRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<DocumentPack, String> {
    let record = repository_for(&default_state_db_path(&app)?)?
        .template_version_by_id(req.version_id.trim())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Версия шаблона не найдена.".to_string())?;
    let path = resolve_user_path(&app, &record.template_path)?;
    let (_, _, actual_sha256) = file_content_signature(&path)?;
    if actual_sha256 != record.template_sha256 {
        return Err("Архивная версия шаблона была изменена после публикации; rollback заблокирован по SHA-256.".into());
    }
    let text = extract_docx_text(&path).map_err(|error| error.to_string())?;
    let mut restored = dokkomplekt_core::create_button_from_template_text(
        &text,
        &record.document_id,
        &path.display().to_string(),
        None,
    );
    if restored.is_static_copy {
        return Err("Архивная версия больше не содержит размеченных полей.".into());
    }
    let rollback_note = format!("Rollback к версии {}.", record.version_number);
    let draft = prepare_template_version_draft(
        &app,
        &record.document_id,
        &path,
        &record.template_sha256,
        &rollback_note,
    )?;
    let (result, versions) = publish_pack_with_template_versions(&app, &state, &[draft], |pack| {
        let existing = pack
            .documents
            .iter_mut()
            .find(|document| document.id.as_str() == record.document_id.as_str())
            .ok_or_else(|| "Документ версии отсутствует в текущем комплекте.".to_string())?;
        restored.button_label = existing.button_label.clone();
        restored.category = existing.category.clone();
        restored.role_id = existing.role_id.clone();
        restored.popup_fields = existing.popup_fields.clone();
        restored.popup_configured = existing.popup_configured;
        restored
            .required_fields
            .extend(existing.required_fields.iter().cloned());
        restored.required_fields.sort();
        restored.required_fields.dedup();
        *existing = restored;
        Ok(())
    })?;
    let published = versions
        .into_iter()
        .next()
        .ok_or_else(|| "Атомарный rollback не вернул опубликованную версию.".to_string())?;
    append_audit_event(
        &app,
        "template_version_rollback",
        &record.template_sha256,
        &serde_json::json!({
            "document_id": record.document_id,
            "from_version_id": record.version_id,
            "published_version_id": published.version_id,
        }),
    )?;
    Ok(result)
}

'''
commands = replace_between(
    commands,
    "#[tauri::command]\nfn rollback_template_version(",
    "#[derive(Debug, Deserialize)]\nstruct DiaryPlanRequest",
    rollback_fn,
)

commands_path.write_text(commands, encoding="utf-8")
