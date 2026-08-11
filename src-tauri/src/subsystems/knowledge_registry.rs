const ORGANIZATION_KNOWLEDGE_STATE_KEY: &str = "organization_knowledge_registry_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OrganizationKnowledgeRecord {
    record_id: String,
    category: String,
    label: String,
    #[serde(default)]
    fields: BTreeMap<String, String>,
    #[serde(default)]
    valid_from: Option<String>,
    #[serde(default)]
    valid_until: Option<String>,
    #[serde(default = "default_true")]
    active: bool,
    #[serde(default)]
    note: String,
    updated_at: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct ListOrganizationKnowledgeRequest {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    include_inactive: bool,
}

#[derive(Debug, Deserialize)]
struct UpsertOrganizationKnowledgeRequest {
    record_id: String,
    category: String,
    label: String,
    #[serde(default)]
    fields: BTreeMap<String, String>,
    #[serde(default)]
    valid_from: Option<String>,
    #[serde(default)]
    valid_until: Option<String>,
    #[serde(default = "default_true")]
    active: bool,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Deserialize)]
struct DeleteOrganizationKnowledgeRequest {
    record_id: String,
}

#[derive(Debug, Deserialize)]
struct ApplyOrganizationKnowledgeRequest {
    record_id: String,
}

fn validate_knowledge_category(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "organization" | "employee" | "position" | "signatory" | "department"
        | "counter" | "print_form" | "authority" | "template_rule" => Ok(normalized),
        _ => Err("Категория знаний должна быть organization, employee, position, signatory, department, counter, print_form, authority или template_rule.".into()),
    }
}

fn validate_knowledge_date(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    let Some(value) = value.map(|item| item.trim().to_string()).filter(|item| !item.is_empty()) else {
        return Ok(None);
    };
    let valid = value.len() == 10
        && value.as_bytes().get(4) == Some(&b'-')
        && value.as_bytes().get(7) == Some(&b'-')
        && value
            .chars()
            .enumerate()
            .all(|(index, ch)| matches!(index, 4 | 7) || ch.is_ascii_digit());
    if !valid {
        return Err(format!("{field} должен иметь формат ГГГГ-ММ-ДД."));
    }
    Ok(Some(value))
}

fn load_organization_knowledge(
    repo: &LocalRepository,
) -> Result<Vec<OrganizationKnowledgeRecord>, String> {
    repo.load_state_value::<Vec<OrganizationKnowledgeRecord>>(ORGANIZATION_KNOWLEDGE_STATE_KEY)
        .map_err(|error| error.to_string())
        .map(Option::unwrap_or_default)
}

fn save_organization_knowledge(
    repo: &LocalRepository,
    records: &[OrganizationKnowledgeRecord],
) -> Result<(), String> {
    repo.save_state_value(ORGANIZATION_KNOWLEDGE_STATE_KEY, &records)
        .map_err(|error| error.to_string())
}

fn record_is_current(record: &OrganizationKnowledgeRecord, today: &str) -> bool {
    record.active
        && record.valid_from.as_deref().is_none_or(|date| date <= today)
        && record.valid_until.as_deref().is_none_or(|date| date >= today)
}

#[tauri::command]
fn list_organization_knowledge(
    req: ListOrganizationKnowledgeRequest,
    app: tauri::AppHandle,
) -> Result<Vec<OrganizationKnowledgeRecord>, String> {
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let mut records = load_organization_knowledge(&repo)?;
    if let Some(category) = req.category.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        let category = validate_knowledge_category(category)?;
        records.retain(|record| record.category == category);
    }
    if !req.include_inactive {
        let today = OffsetDateTime::now_utc().date().to_string();
        records.retain(|record| record_is_current(record, &today));
    }
    records.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });
    Ok(records)
}

#[tauri::command]
fn upsert_organization_knowledge(
    req: UpsertOrganizationKnowledgeRequest,
    app: tauri::AppHandle,
) -> Result<Vec<OrganizationKnowledgeRecord>, String> {
    let record_id = req.record_id.trim().to_string();
    if record_id.is_empty()
        || record_id.len() > 120
        || !record_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("record_id должен быть непустым безопасным идентификатором до 120 символов.".into());
    }
    let category = validate_knowledge_category(&req.category)?;
    let label = req.label.trim().to_string();
    if label.is_empty() || label.len() > 240 || label.chars().any(char::is_control) {
        return Err("Укажите понятное название записи до 240 символов.".into());
    }
    if req.fields.is_empty() {
        return Err("Запись знаний должна содержать хотя бы одно смысловое поле.".into());
    }
    let mut fields = BTreeMap::new();
    for (field_id, value) in req.fields {
        let field_id = field_id.trim().to_string();
        let value = value.trim().to_string();
        if !is_valid_field_id(&field_id) {
            return Err(format!("Некорректный идентификатор поля: {field_id}"));
        }
        if value.is_empty() || value.len() > 20_000 || value.chars().any(char::is_control) {
            return Err(format!("Некорректное значение поля: {field_id}"));
        }
        validate_field_value(&field_id, &value)?;
        fields.insert(field_id, value);
    }
    let valid_from = validate_knowledge_date(req.valid_from, "valid_from")?;
    let valid_until = validate_knowledge_date(req.valid_until, "valid_until")?;
    if valid_from.as_ref().zip(valid_until.as_ref()).is_some_and(|(from, until)| from > until) {
        return Err("valid_from не может быть позже valid_until.".into());
    }
    let record = OrganizationKnowledgeRecord {
        record_id: record_id.clone(),
        category,
        label,
        fields,
        valid_from,
        valid_until,
        active: req.active,
        note: req.note.trim().chars().take(2_000).collect(),
        updated_at: OffsetDateTime::now_utc().to_string(),
    };
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let mut records = load_organization_knowledge(&repo)?;
    records.retain(|item| item.record_id != record_id);
    records.push(record.clone());
    save_organization_knowledge(&repo, &records)?;
    append_audit_event(
        &app,
        "organization_knowledge_updated",
        &format!("{:x}", Sha256::digest(record_id.as_bytes())),
        &serde_json::json!({
            "record_id": record.record_id,
            "category": record.category,
            "field_ids": record.fields.keys().collect::<Vec<_>>(),
            "active": record.active,
            "valid_from": record.valid_from,
            "valid_until": record.valid_until,
        }),
    )?;
    list_organization_knowledge(
        ListOrganizationKnowledgeRequest {
            category: None,
            include_inactive: true,
        },
        app,
    )
}

#[tauri::command]
fn delete_organization_knowledge(
    req: DeleteOrganizationKnowledgeRequest,
    app: tauri::AppHandle,
) -> Result<Vec<OrganizationKnowledgeRecord>, String> {
    let record_id = req.record_id.trim();
    if record_id.is_empty() {
        return Err("Не указан record_id.".into());
    }
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let mut records = load_organization_knowledge(&repo)?;
    let previous_len = records.len();
    records.retain(|item| item.record_id != record_id);
    if records.len() == previous_len {
        return Err("Запись знаний не найдена.".into());
    }
    save_organization_knowledge(&repo, &records)?;
    append_audit_event(
        &app,
        "organization_knowledge_deleted",
        &format!("{:x}", Sha256::digest(record_id.as_bytes())),
        &serde_json::json!({"record_id": record_id}),
    )?;
    Ok(records)
}

#[tauri::command]
fn apply_organization_knowledge(
    req: ApplyOrganizationKnowledgeRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SemanticCase, String> {
    let record_id = req.record_id.trim();
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let records = load_organization_knowledge(&repo)?;
    let record = records
        .into_iter()
        .find(|item| item.record_id == record_id)
        .ok_or_else(|| "Запись знаний не найдена.".to_string())?;
    let today = OffsetDateTime::now_utc().date().to_string();
    if !record_is_current(&record, &today) {
        return Err("Запись неактивна или находится вне срока действия; применение заблокировано.".into());
    }
    let updated = transact_default_state(&app, &state, |snapshot| {
        for (field_id, value) in &record.fields {
            set_user_value(&mut snapshot.semantic_case, field_id.clone(), value);
        }
        Ok((snapshot.semantic_case.clone(), true))
    })?;
    append_audit_event(
        &app,
        "organization_knowledge_applied",
        &format!("{:x}", Sha256::digest(record_id.as_bytes())),
        &serde_json::json!({
            "record_id": record.record_id,
            "category": record.category,
            "field_ids": record.fields.keys().collect::<Vec<_>>(),
            "fields_confirmed_by_user_action": true,
        }),
    )?;
    Ok(updated)
}
