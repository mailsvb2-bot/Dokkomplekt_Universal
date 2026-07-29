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

// Shared fail-closed helpers live in this included module so all Tauri command
// subsystems use one implementation without duplicating unsafe file handling.
fn validate_safe_template_bytes(bytes: &[u8]) -> dokkomplekt_docx::DocxResult<()> {
    dokkomplekt_docx::validate_safe_template_bytes(bytes)
}

pub(crate) fn commit_atomic_temp_file(temp: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "У файла назначения нет родительской папки.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    if let Ok(metadata) = std::fs::symlink_metadata(destination) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Небезопасный файл назначения не заменён: {}",
                destination.display()
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source = temp
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let target = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let moved = unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved == 0 {
            return Err(format!(
                "Не удалось атомарно заменить {}: {}",
                destination.display(),
                std::io::Error::last_os_error()
            ));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::rename(temp, destination).map_err(|error| {
            format!(
                "Не удалось атомарно заменить {}: {error}",
                destination.display()
            )
        })?;
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

pub(crate) fn atomic_write_file(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let parent = destination
        .parent()
        .ok_or_else(|| "У файла назначения нет родительской папки.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("dokkomplekt"),
        Uuid::new_v4()
    ));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    if let Err(error) = output.write_all(bytes).and_then(|_| output.sync_all()) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    drop(output);
    let result = commit_atomic_temp_file(&temporary, destination);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
