const SELECTED_PROCESS_BLUEPRINT_STATE_KEY: &str = "selected_process_blueprint_v1";
const PROCESS_BLUEPRINTS_JSON: &str = include_str!("../../../content-packs/process-blueprints.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProcessBlueprint {
    process_id: String,
    domain: String,
    locale: String,
    title: String,
    description: String,
    template_slots: Vec<String>,
    high_risk_fields: Vec<String>,
    validators: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProcessBlueprintCatalog {
    schema: u32,
    usage_mode: String,
    notice: String,
    processes: Vec<ProcessBlueprint>,
}

#[derive(Debug, Serialize)]
struct ProcessBlueprintState {
    selected_process_id: Option<String>,
    processes: Vec<ProcessBlueprint>,
    notice: String,
}

#[derive(Debug, Deserialize)]
struct SelectProcessBlueprintRequest {
    process_id: String,
}

fn process_blueprint_catalog() -> Result<ProcessBlueprintCatalog, String> {
    let catalog = serde_json::from_str::<ProcessBlueprintCatalog>(PROCESS_BLUEPRINTS_JSON)
        .map_err(|error| format!("Каталог процессов повреждён: {error}"))?;
    if catalog.schema != 1 || catalog.usage_mode != "workflow_blueprints_only" {
        return Err("Каталог процессов имеет неподдерживаемую схему.".into());
    }
    if catalog.processes.is_empty() {
        return Err("Каталог процессов пуст.".into());
    }
    Ok(catalog)
}

#[tauri::command]
fn get_process_blueprints(app: tauri::AppHandle) -> Result<ProcessBlueprintState, String> {
    let catalog = process_blueprint_catalog()?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    let selected_process_id = repo
        .load_state_value::<String>(SELECTED_PROCESS_BLUEPRINT_STATE_KEY)
        .map_err(|error| error.to_string())?;
    Ok(ProcessBlueprintState {
        selected_process_id,
        processes: catalog.processes,
        notice: catalog.notice,
    })
}

#[tauri::command]
fn select_process_blueprint(
    req: SelectProcessBlueprintRequest,
    app: tauri::AppHandle,
) -> Result<ProcessBlueprintState, String> {
    let process_id = req.process_id.trim();
    let catalog = process_blueprint_catalog()?;
    let selected = catalog
        .processes
        .iter()
        .find(|process| process.process_id == process_id)
        .ok_or_else(|| "Выбранный процесс отсутствует в каталоге.".to_string())?;
    let repo = repository_for(&default_state_db_path(&app)?)?;
    repo.save_state_value(SELECTED_PROCESS_BLUEPRINT_STATE_KEY, &selected.process_id)
        .map_err(|error| error.to_string())?;
    append_audit_event(
        &app,
        "process_blueprint_selected",
        &format!("{:x}", Sha256::digest(selected.process_id.as_bytes())),
        &serde_json::json!({
            "process_id": selected.process_id,
            "domain": selected.domain,
            "template_slots": selected.template_slots,
            "blueprint_only_no_certified_forms": true,
        }),
    )?;
    get_process_blueprints(app)
}
