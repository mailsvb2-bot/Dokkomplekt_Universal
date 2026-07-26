const LEGACY_BUSINESS_REGISTRY_STATE_KEY: &str = "business_registry_cache_v1";
const BUSINESS_REGISTRY_INDEX_STATE_KEY: &str = "business_registry_index_v2";
const BUSINESS_REGISTRY_RECORD_PREFIX: &str = "business_registry_record_v2:";
static BUSINESS_REGISTRY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BusinessRegistryRecord {
    inn: String,
    name: String,
    #[serde(default)]
    kpp: Option<String>,
    #[serde(default)]
    ogrn: Option<String>,
    #[serde(default)]
    legal_address: Option<String>,
    #[serde(default)]
    director: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    source: String,
    #[serde(default)]
    source_updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct BusinessRegistryImportResult {
    total_records: usize,
    imported_records: usize,
    replaced: bool,
}

#[derive(Debug, Deserialize)]
struct ImportBusinessRegistryRequest {
    records: Vec<BusinessRegistryRecord>,
    #[serde(default)]
    replace: bool,
}

#[derive(Debug, Deserialize)]
struct LookupBusinessRegistryRequest {
    inn: String,
}

#[derive(Debug, Deserialize)]
struct ApplyBusinessRegistryRequest {
    inn: String,
    target: String,
}

#[derive(Debug, Deserialize)]
struct ExportOneCRequest {
    output_path: String,
    #[serde(default)]
    inns: Vec<String>,
}

fn lock_business_registry() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    BUSINESS_REGISTRY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "business registry lock failed".to_string())
}

fn normalize_registry_record(
    mut record: BusinessRegistryRecord,
) -> Result<BusinessRegistryRecord, String> {
    record.inn = record.inn.chars().filter(char::is_ascii_digit).collect();
    validate_field_value("org.inn", &record.inn)?;
    record.name = record.name.trim().to_string();
    if record.name.is_empty() || record.name.chars().count() > 500 {
        return Err("Наименование контрагента пусто или слишком длинное.".into());
    }
    record.kpp = record
        .kpp
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = record.kpp.as_deref() {
        validate_field_value("org.kpp", value)?;
    }
    record.ogrn = record
        .ogrn
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = record.ogrn.as_deref() {
        validate_field_value("org.ogrn", value)?;
    }
    record.legal_address = record
        .legal_address
        .take()
        .map(|value| value.trim().chars().take(1000).collect())
        .filter(|value: &String| !value.is_empty());
    record.director = record
        .director
        .take()
        .map(|value| value.trim().chars().take(300).collect())
        .filter(|value: &String| !value.is_empty());
    record.status = record
        .status
        .take()
        .map(|value| value.trim().chars().take(100).collect())
        .filter(|value: &String| !value.is_empty());
    record.source = record.source.trim().chars().take(160).collect();
    record.source_updated_at = record
        .source_updated_at
        .take()
        .map(|value| value.trim().chars().take(80).collect())
        .filter(|value: &String| !value.is_empty());
    Ok(record)
}

fn registry_record_key(inn: &str) -> String {
    let digest = Sha256::digest(inn.as_bytes());
    format!("{BUSINESS_REGISTRY_RECORD_PREFIX}{digest:x}")
}

fn load_registry_index(repo: &LocalRepository) -> Result<BTreeSet<String>, String> {
    repo.load_state_value::<Vec<String>>(BUSINESS_REGISTRY_INDEX_STATE_KEY)
        .map_err(|error| error.to_string())
        .map(|value| value.unwrap_or_default().into_iter().collect())
}

fn save_registry_index(repo: &LocalRepository, index: &BTreeSet<String>) -> Result<(), String> {
    repo.save_state_value(
        BUSINESS_REGISTRY_INDEX_STATE_KEY,
        &index.iter().cloned().collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())
}

fn migrate_legacy_business_registry(repo: &LocalRepository) -> Result<(), String> {
    let Some(records) = repo
        .load_state_value::<Vec<BusinessRegistryRecord>>(LEGACY_BUSINESS_REGISTRY_STATE_KEY)
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let mut index = load_registry_index(repo)?;
    for record in records {
        let record = normalize_registry_record(record)?;
        repo.save_state_value(&registry_record_key(&record.inn), &record)
            .map_err(|error| error.to_string())?;
        index.insert(record.inn);
    }
    save_registry_index(repo, &index)?;
    repo.delete_state_value(LEGACY_BUSINESS_REGISTRY_STATE_KEY)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_business_registry_record(
    repo: &LocalRepository,
    inn: &str,
) -> Result<Option<BusinessRegistryRecord>, String> {
    repo.load_state_value::<BusinessRegistryRecord>(&registry_record_key(inn))
        .map_err(|error| error.to_string())
}

fn load_all_business_registry_records(
    repo: &LocalRepository,
) -> Result<Vec<BusinessRegistryRecord>, String> {
    let mut records = Vec::new();
    for inn in load_registry_index(repo)? {
        if let Some(record) = load_business_registry_record(repo, &inn)? {
            records.push(record);
        }
    }
    Ok(records)
}

#[tauri::command]
fn import_business_registry(
    req: ImportBusinessRegistryRequest,
    app: tauri::AppHandle,
) -> Result<BusinessRegistryImportResult, String> {
    if req.records.is_empty() {
        return Err("Справочник не содержит записей.".into());
    }
    if req.records.len() > 100_000 {
        return Err("За один импорт допускается не более 100 000 записей.".into());
    }
    let _registry_guard = lock_business_registry()?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    migrate_legacy_business_registry(&repo)?;

    let normalized = req
        .records
        .into_iter()
        .map(normalize_registry_record)
        .collect::<Result<Vec<_>, _>>()?;
    let imported_records = normalized.len();
    let incoming_inns = normalized
        .iter()
        .map(|record| record.inn.clone())
        .collect::<BTreeSet<_>>();
    let previous_index = load_registry_index(&repo)?;
    if req.replace {
        for inn in previous_index.difference(&incoming_inns) {
            repo.delete_state_value(&registry_record_key(inn))
                .map_err(|error| error.to_string())?;
        }
    }
    let mut next_index = if req.replace {
        BTreeSet::new()
    } else {
        previous_index
    };
    for record in normalized {
        repo.save_state_value(&registry_record_key(&record.inn), &record)
            .map_err(|error| error.to_string())?;
        next_index.insert(record.inn);
    }
    save_registry_index(&repo, &next_index)?;
    let result = BusinessRegistryImportResult {
        total_records: next_index.len(),
        imported_records,
        replaced: req.replace,
    };
    append_audit_event(
        &app,
        "business_registry_imported",
        "",
        &serde_json::json!({
            "total_records": result.total_records,
            "imported_records": result.imported_records,
            "replace": result.replaced,
            "storage": "encrypted_per_record_hash_index_v2",
        }),
    )?;
    Ok(result)
}

#[tauri::command]
fn lookup_business_registry(
    req: LookupBusinessRegistryRequest,
    app: tauri::AppHandle,
) -> Result<Option<BusinessRegistryRecord>, String> {
    let inn = req.inn.chars().filter(char::is_ascii_digit).collect::<String>();
    validate_field_value("org.inn", &inn)?;
    let _registry_guard = lock_business_registry()?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    migrate_legacy_business_registry(&repo)?;
    load_business_registry_record(&repo, &inn)
}

#[tauri::command]
fn apply_business_registry_record(
    req: ApplyBusinessRegistryRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SemanticCase, String> {
    let inn = req.inn.chars().filter(char::is_ascii_digit).collect::<String>();
    validate_field_value("org.inn", &inn)?;
    let record = {
        let _registry_guard = lock_business_registry()?;
        let repo = repository_for(&default_state_db_path(&app)?)?;
        migrate_legacy_business_registry(&repo)?;
        load_business_registry_record(&repo, &inn)?.ok_or_else(|| {
            "Контрагент с таким ИНН отсутствует в локальном проверенном справочнике."
                .to_string()
        })?
    };
    let prefix = match req.target.as_str() {
        "organization" | "org" => "org",
        "counterparty" => "counterparty",
        _ => return Err("target должен быть organization или counterparty.".into()),
    };
    let mut case = state.semantic_case.lock().map_err(|_| "state lock failed")?;
    let mut set = |suffix: &str, value: Option<&str>| {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            set_user_value(&mut case, format!("{prefix}.{suffix}"), value);
        }
    };
    set("name", Some(&record.name));
    set("inn", Some(&record.inn));
    set("kpp", record.kpp.as_deref());
    set("ogrn", record.ogrn.as_deref());
    set("address", record.legal_address.as_deref());
    set("director", record.director.as_deref());
    let updated = case.clone();
    drop(case);
    let object_hash = format!("{:x}", Sha256::digest(record.inn.as_bytes()));
    append_audit_event(
        &app,
        "business_registry_record_applied",
        &object_hash,
        &serde_json::json!({
            "target": prefix,
            "fields_confirmed_by_user_action": true,
            "source": record.source,
            "source_updated_at": record.source_updated_at,
        }),
    )?;
    Ok(updated)
}

#[tauri::command]
fn export_one_c_counterparties(
    req: ExportOneCRequest,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let requested = req
        .inns
        .iter()
        .map(|inn| inn.chars().filter(char::is_ascii_digit).collect::<String>())
        .filter(|inn| !inn.is_empty())
        .collect::<BTreeSet<_>>();
    for inn in &requested {
        validate_field_value("org.inn", inn)?;
    }
    let records = {
        let _registry_guard = lock_business_registry()?;
        let repo = repository_for(&default_state_db_path(&app)?)?;
        migrate_legacy_business_registry(&repo)?;
        if requested.is_empty() {
            load_all_business_registry_records(&repo)?
        } else {
            let mut selected = Vec::new();
            for inn in &requested {
                if let Some(record) = load_business_registry_record(&repo, inn)? {
                    selected.push(record);
                }
            }
            selected
        }
    };
    if records.is_empty() {
        return Err("Нет контрагентов для экспорта.".into());
    }
    let output = resolve_user_path(&app, &req.output_path)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let payload = serde_json::json!({
        "schema": "dokkomplekt.1c-counterparty-exchange.v1",
        "generated_at": OffsetDateTime::now_utc().to_string(),
        "records": records,
    });
    let temporary = output.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if output.exists() {
        std::fs::remove_file(&output).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temporary, &output).map_err(|error| error.to_string())?;
    append_audit_event(
        &app,
        "one_c_counterparties_exported",
        "",
        &serde_json::json!({
            "record_count": payload["records"].as_array().map(Vec::len).unwrap_or_default(),
            "format": "dokkomplekt.1c-counterparty-exchange.v1",
        }),
    )?;
    Ok(output.display().to_string())
}
