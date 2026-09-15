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
    #[cfg(target_os = "linux")]
    if let Some(window) = app.get_webview_window("main") {
        match window.set_title("Dokkomplekt Universal") {
            Ok(()) => eprintln!("Dokkomplekt native frontend IPC ready"),
            Err(error) => eprintln!("Dokkomplekt native frontend IPC signal failed: {error}"),
        }
    }
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

#[cfg(target_os = "windows")]
fn windows_verbatim_wide_path(path: &Path) -> Result<Vec<u16>, String> {
    use std::os::windows::ffi::OsStrExt as _;

    let absolute = std::path::absolute(path).map_err(|error| {
        format!("Не удалось получить абсолютный Windows-путь {}: {error}", path.display())
    })?;
    let raw = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
    const VERBATIM_PREFIX: [u16; 4] = [92, 92, 63, 92];
    const DEVICE_PREFIX: [u16; 4] = [92, 92, 46, 92];
    const UNC_PREFIX: [u16; 2] = [92, 92];
    const VERBATIM_UNC_PREFIX: [u16; 8] = [92, 92, 63, 92, 85, 78, 67, 92];

    let mut wide = Vec::with_capacity(raw.len() + VERBATIM_UNC_PREFIX.len() + 1);
    if raw.starts_with(&VERBATIM_PREFIX) || raw.starts_with(&DEVICE_PREFIX) {
        wide.extend_from_slice(&raw);
    } else if raw.starts_with(&UNC_PREFIX) {
        wide.extend_from_slice(&VERBATIM_UNC_PREFIX);
        wide.extend_from_slice(&raw[UNC_PREFIX.len()..]);
    } else {
        wide.extend_from_slice(&VERBATIM_PREFIX);
        wide.extend_from_slice(&raw);
    }
    wide.push(0);
    Ok(wide)
}

pub(crate) fn commit_atomic_temp_file(temp: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "У файла назначения нет родительской папки.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let destination_exists = match std::fs::symlink_metadata(destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(format!(
                    "Небезопасный файл назначения не заменён: {}",
                    destination.display()
                ));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "Не удалось безопасно проверить файл назначения {}: {error}",
                destination.display()
            ));
        }
    };
    #[cfg(not(target_os = "windows"))]
    let _ = destination_exists;
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source = windows_verbatim_wide_path(temp)?;
        let target = windows_verbatim_wide_path(destination)?;
        let move_flags = if destination_exists {
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
        } else {
            MOVEFILE_WRITE_THROUGH
        };
        let moved = unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), move_flags) };
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
    // Keep the temporary basename opaque and short. Repeating the destination
    // basename here can push an otherwise valid Windows destination beyond MAX_PATH
    // before the atomic MoveFileExW boundary.
    let temporary = parent.join(format!(".dok-{}.tmp", Uuid::new_v4().simple()));
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


#[cfg(test)]
#[cfg(target_os = "windows")]
mod atomic_write_windows_contract_tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt as _;

    #[test]
    fn atomic_write_supports_windows_path_where_legacy_temp_name_exceeded_max_path() {
        let temp_root = std::env::temp_dir();
        let service_dir = "generation-completion-receipts";
        let destination_name = format!("{}.json", "f".repeat(64));
        let temp_root_len = temp_root.as_os_str().encode_wide().count();
        let service_len = service_dir.encode_utf16().count();
        let desired_parent_len = 185usize;
        let fixed_parent_len = temp_root_len + 2 + service_len;
        let component_len = desired_parent_len
            .saturating_sub(fixed_parent_len)
            .clamp(48, 180);
        let uuid = Uuid::new_v4().simple().to_string();
        let root_component = format!(
            "{}{}",
            "x".repeat(component_len.saturating_sub(uuid.len())),
            uuid
        );
        let parent = temp_root.join(root_component).join(service_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let destination = parent.join(&destination_name);
        let legacy_temporary = parent.join(format!(
            ".{}.{}.tmp",
            destination_name,
            Uuid::new_v4()
        ));
        let legacy_len = legacy_temporary.as_os_str().encode_wide().count();
        assert!(
            legacy_len > 260,
            "fixture must reproduce the old MAX_PATH failure, got {legacy_len} UTF-16 units"
        );

        atomic_write_file(&destination, b"atomic-long-path-proof")
            .expect("short opaque temp + verbatim MoveFileExW paths must publish");
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"atomic-long-path-proof"
        );
        let leftovers = std::fs::read_dir(&parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".dok-"))
            .count();
        assert_eq!(leftovers, 0, "atomic temp file must not survive commit");

        let root = parent.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_dir_all(root);
    }
}
