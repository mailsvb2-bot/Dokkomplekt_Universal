// Background watcher, autostart and bounded batch processing.
#[derive(Debug, Deserialize)]
struct WatcherInstallRequest {
    watch_folder: String,
    output_root: String,
    #[serde(default)]
    default_year: Option<i32>,
    #[serde(default)]
    sick_leave_enabled: bool,
    #[serde(default)]
    folder_parts: Vec<FolderNamePart>,
    #[serde(default)]
    auto_print: bool,
    #[serde(default)]
    print_copies_by_document: BTreeMap<String, u16>,
    #[serde(default = "default_parallel_cases")]
    max_parallel_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WatcherHandoffOwner {
    generation: String,
    executable: String,
    executable_sha256: String,
    #[serde(default)]
    ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WatcherRuntimeConfig {
    watch_folder: String,
    #[serde(default)]
    output_root: String,
    default_year: i32,
    sick_leave_enabled: bool,
    folder_parts: Vec<FolderNamePart>,
    #[serde(default)]
    auto_print: bool,
    #[serde(default)]
    print_copies_by_document: BTreeMap<String, u16>,
    #[serde(default = "default_parallel_cases")]
    max_parallel_cases: usize,
    /// Canonical update handoff. Old configs deserialize with `None`, while
    /// handoff-aware versions can retire a stale watcher after a newer install
    /// publishes a ready owner.
    #[serde(default)]
    handoff_owner: Option<WatcherHandoffOwner>,
}

fn watcher_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("intake-agent-autostart.json"))
}

fn read_watcher_runtime_config(path: &Path) -> Result<WatcherRuntimeConfig, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("Настройки фонового агента недоступны: {error}"))?;
    serde_json::from_slice::<WatcherRuntimeConfig>(&bytes)
        .map_err(|error| format!("Настройки фонового агента повреждены: {error}"))
}

fn effective_watcher_output_root(
    app: &tauri::AppHandle,
    runtime: &WatcherRuntimeConfig,
) -> Result<PathBuf, String> {
    let configured = runtime.output_root.trim();
    let raw = if configured.is_empty() {
        let preferences = load_output_preferences_from_store(app)?;
        if preferences.output_root.trim().is_empty() {
            return Err(
                "Фоновый агент создан старой версией без отдельной папки результата. Откройте Доккомплект и заново подтвердите папку готовых документов."
                    .into(),
            );
        }
        preferences.output_root
    } else {
        configured.to_string()
    };
    let output_root = resolve_user_visible_absolute_path(&raw, "Папка готовых документов")?;
    ensure_output_root_path(&output_root)?;
    Ok(output_root)
}

fn effective_watcher_folder_parts(runtime: &WatcherRuntimeConfig) -> Vec<FolderNamePart> {
    if runtime.folder_parts.is_empty() {
        default_output_folder_parts()
    } else {
        runtime.folder_parts.clone()
    }
}

fn watcher_directories_are_same(left: &Path, right: &Path) -> Result<bool, String> {
    let left = std::fs::canonicalize(left).map_err(|error| {
        format!("Не удалось определить фактический путь «{}»: {error}", left.display())
    })?;
    let right = std::fs::canonicalize(right).map_err(|error| {
        format!("Не удалось определить фактический путь «{}»: {error}", right.display())
    })?;
    #[cfg(windows)]
    {
        Ok(left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

fn watcher_owner_for_executable(exe: &Path, ready: bool) -> Result<WatcherHandoffOwner, String> {
    let (_, _, executable_sha256) = file_content_signature(exe)?;
    Ok(WatcherHandoffOwner {
        generation: Uuid::new_v4().to_string(),
        executable: exe.display().to_string(),
        executable_sha256,
        ready,
    })
}

fn watcher_owner_target_is_valid(owner: &WatcherHandoffOwner) -> bool {
    if !owner.ready || owner.executable.trim().is_empty() || owner.executable_sha256.len() != 64 {
        return false;
    }
    let target = PathBuf::from(&owner.executable);
    target.is_file()
        && file_content_signature(&target)
            .map(|(_, _, sha256)| sha256.eq_ignore_ascii_case(&owner.executable_sha256))
            .unwrap_or(false)
}

fn watcher_owner_matches_current(owner: &WatcherHandoffOwner) -> Result<bool, String> {
    if !owner.ready {
        return Ok(true);
    }
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let (_, _, current_sha256) = file_content_signature(&current)?;
    Ok(Path::new(&owner.executable) == current.as_path()
        && current_sha256.eq_ignore_ascii_case(&owner.executable_sha256))
}

fn watcher_owner_superseded(
    captured: Option<&WatcherHandoffOwner>,
    latest: Option<&WatcherHandoffOwner>,
) -> bool {
    let Some(latest) = latest.filter(|owner| owner.ready) else {
        return false;
    };
    match captured {
        None => true,
        Some(captured) => captured.generation != latest.generation
            || captured.executable != latest.executable
            || !captured
                .executable_sha256
                .eq_ignore_ascii_case(&latest.executable_sha256),
    }
}

fn latest_ready_watcher_owner(control_path: Option<&Path>) -> Option<WatcherHandoffOwner> {
    let path = control_path?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<WatcherRuntimeConfig>(&bytes)
        .ok()?
        .handoff_owner
        .filter(watcher_owner_target_is_valid)
}

fn spawn_silent_executable(exe: &Path, background_watch: bool) -> Result<(), String> {
    let mut command = std::process::Command::new(exe);
    if background_watch {
        command.arg("--background-watch");
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Не удалось запустить актуальный Dokkomplekt: {error}"))
}

/// Launch the canonical normal UI process for an accepted dropped source. If a
/// UI is already running, the normal singleton path turns this launch into an
/// activation request and exits, so the watcher never creates a second UI.
fn launch_or_activate_watcher_ui(control_path: Option<&Path>) -> Result<(), String> {
    let target = latest_ready_watcher_owner(control_path)
        .map(|owner| PathBuf::from(owner.executable))
        .unwrap_or(std::env::current_exe().map_err(|error| error.to_string())?);
    spawn_silent_executable(&target, false)
}

fn release_watcher_instance_lock(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let state = app.state::<AppState>();
    let mut slot = state
        .instance_lock
        .lock()
        .map_err(|_| "instance state lock failed".to_string())?;
    let path = slot.take();
    if let Some(path) = path.as_ref() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                *slot = Some(path.clone());
                return Err(format!(
                    "Не удалось освободить singleton старого фонового агента: {error}"
                ));
            }
        }
    }
    Ok(path)
}

fn restore_watcher_instance_lock(app: &tauri::AppHandle) -> Result<(), String> {
    match acquire_instance_lock(app, true)? {
        InstanceLockOutcome::Acquired(path) => {
            let state = app.state::<AppState>();
            *state
                .instance_lock
                .lock()
                .map_err(|_| "instance state lock failed".to_string())? = Some(path);
            Ok(())
        }
        InstanceLockOutcome::AlreadyRunning => Err(
            "Новый фоновый агент уже занял singleton после неудачной передачи.".into(),
        ),
    }
}

fn handoff_watcher_to_successor(
    app: &tauri::AppHandle,
    owner: &WatcherHandoffOwner,
) -> Result<(), String> {
    if !watcher_owner_target_is_valid(owner) {
        return Err("Новый владелец фонового агента не прошёл проверку executable SHA-256.".into());
    }
    let _released_path = release_watcher_instance_lock(app)?;
    let target = PathBuf::from(&owner.executable);
    if let Err(error) = spawn_silent_executable(&target, true) {
        let restore = restore_watcher_instance_lock(app);
        return match restore {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!("{error}; singleton не восстановлен: {restore_error}")),
        };
    }
    Ok(())
}

fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn desktop_exec_quote(raw: &str) -> String {
    format!("\"{}\"", raw.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "windows")]
fn install_windows_run_entry(exe: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let command = format!("\"{}\" --background-watch", exe.display());
    let status = std::process::Command::new("reg.exe")
        .arg("add")
        .arg(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run")
        .arg("/v")
        .arg("DokkomplektWatcher")
        .arg("/t")
        .arg("REG_SZ")
        .arg("/d")
        .arg(&command)
        .arg("/f")
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("reg.exe завершился с кодом {status}"))
    }
}

#[cfg(target_os = "windows")]
fn remove_windows_run_entry() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = std::process::Command::new("reg.exe")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "DokkomplektWatcher",
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("reg.exe завершился с кодом {status}"))
    }
}

/// Configure silent OS autostart. Runtime settings are read from the protected
/// per-user JSON file; raw paths are no longer embedded into shell scripts.
fn write_autostart_entries(exe: &Path) -> Result<(Vec<PathBuf>, Vec<String>), String> {
    let mut files = Vec::new();
    let warnings = Vec::new();
    match std::env::consts::OS {
        "windows" => {
            #[cfg(target_os = "windows")]
            install_windows_run_entry(exe)
                .map_err(|error| format!("Автозапуск Windows не настроен: {error}"))?;
        }
        "macos" => {
            let home = std::env::var("HOME")
                .map_err(|_| "Переменная HOME не найдена; автозапуск не настроен.".to_string())?;
            let dir = PathBuf::from(home).join("Library/LaunchAgents");
            let path = dir.join("ru.dokkomplekt.watcher.plist");
            let body = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>ru.dokkomplekt.watcher</string>\n<key>ProgramArguments</key><array><string>{}</string><string>--background-watch</string></array>\n<key>RunAtLoad</key><true/>\n</dict></plist>\n",
                xml_escape(&exe.display().to_string())
            );
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, body))
                .map_err(|error| format!("LaunchAgent не записан: {error}"))?;
            files.push(path);
        }
        _ => {
            let config = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map_err(|_| "HOME/XDG_CONFIG_HOME не найдены; автозапуск не настроен.".to_string())?;
            let dir = config.join("autostart");
            let path = dir.join("dokkomplekt-watcher.desktop");
            let body = format!(
                "[Desktop Entry]\nType=Application\nName=Dokkomplekt Watcher\nExec={} --background-watch\nX-GNOME-Autostart-enabled=true\n",
                desktop_exec_quote(&exe.display().to_string())
            );
            std::fs::create_dir_all(&dir)
                .and_then(|_| std::fs::write(&path, body))
                .map_err(|error| format!("XDG-автозапуск не записан: {error}"))?;
            files.push(path);
        }
    }
    Ok((files, warnings))
}

fn remove_autostart_entries() -> (Vec<PathBuf>, Vec<String>) {
    let mut removed = Vec::new();
    #[cfg(target_os = "windows")]
    let mut warnings = Vec::new();
    #[cfg(not(target_os = "windows"))]
    let warnings = Vec::new();
    match std::env::consts::OS {
        "windows" => {
            #[cfg(target_os = "windows")]
            if let Err(error) = remove_windows_run_entry() {
                warnings.push(format!("Запись автозапуска Windows не удалена: {error}"));
            }
            if let Ok(appdata) = std::env::var("APPDATA") {
                let legacy = PathBuf::from(appdata)
                    .join("Microsoft/Windows/Start Menu/Programs/Startup/Dokkomplekt-Watcher.cmd");
                if legacy.exists() && std::fs::remove_file(&legacy).is_ok() {
                    removed.push(legacy);
                }
            }
        }
        "macos" => {
            if let Ok(home) = std::env::var("HOME") {
                let path =
                    PathBuf::from(home).join("Library/LaunchAgents/ru.dokkomplekt.watcher.plist");
                if path.exists() && std::fs::remove_file(&path).is_ok() {
                    removed.push(path);
                }
            }
        }
        _ => {
            let config = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")));
            if let Ok(config) = config {
                let path = config.join("autostart/dokkomplekt-watcher.desktop");
                if path.exists() && std::fs::remove_file(&path).is_ok() {
                    removed.push(path);
                }
            }
        }
    }
    (removed, warnings)
}

fn unreadable_note_file_name(source_stem: &str) -> String {
    format!("{source_stem} — НЕ ПРОЧИТАН.txt")
}

const NOTE_SOURCE_SHA256_PREFIX: &str = "source_sha256=";
const NOTE_SOURCE_SIZE_PREFIX: &str = "source_size_bytes=";
const NOTE_SOURCE_MTIME_PREFIX: &str = "source_modified_unix_ms=";
const NOTE_ERROR_CATEGORY_PREFIX: &str = "error_category=";
const NOTE_RETRY_MODE_PREFIX: &str = "retry_mode=";
const NOTE_RETRY_AFTER_PREFIX: &str = "retry_after_unix_ms=";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnreadableRetryPolicy {
    ContentChange,
    Timed(Duration),
}

fn unix_time_ms(value: std::time::SystemTime) -> u128 {
    value
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn classify_watcher_configuration_error(_error: &str) -> (&'static str, UnreadableRetryPolicy) {
    (
        "watcher_configuration_unavailable",
        UnreadableRetryPolicy::Timed(Duration::from_secs(60)),
    )
}

fn classify_processing_error(error: &str) -> (&'static str, UnreadableRetryPolicy) {
    let normalized = error.to_lowercase();
    if normalized.contains("internal watcher panic") {
        return ("internal_failure", UnreadableRetryPolicy::ContentChange);
    }
    let permanent_source_markers = [
        "поврежд", "invalid zip", "invalid docx", "invalid xml", "неподдерживаемый формат",
        "unsupported format", "не удалось распознать формат", "path traversal",
        "превышает допустимый размер", "слишком большой", "too large", "payload too large",
    ];
    if permanent_source_markers.iter().any(|marker| normalized.contains(marker)) {
        return ("source_invalid", UnreadableRetryPolicy::ContentChange);
    }
    let component_markers = [
        "component", "sidecar", "tesseract", "poppler", "libreoffice", "soffice", "llama", "ocr",
    ];
    if component_markers.iter().any(|marker| normalized.contains(marker)) {
        return (
            "component_unavailable",
            UnreadableRetryPolicy::Timed(Duration::from_secs(2 * 60)),
        );
    }
    let storage_markers = [
        "database", "postgres", "sqlite", "permission denied", "access is denied",
        "занят другим процессом", "нет доступа", "network", "timeout", "connection", "queue",
    ];
    if storage_markers.iter().any(|marker| normalized.contains(marker)) {
        return (
            "infrastructure_unavailable",
            UnreadableRetryPolicy::Timed(Duration::from_secs(60)),
        );
    }
    let license_markers = ["license", "лиценз", "лимит", "activation"];
    if license_markers.iter().any(|marker| normalized.contains(marker)) {
        return (
            "access_temporarily_denied",
            UnreadableRetryPolicy::Timed(Duration::from_secs(10 * 60)),
        );
    }
    (
        "processing_error",
        UnreadableRetryPolicy::Timed(Duration::from_secs(5 * 60)),
    )
}

fn note_with_source_fingerprint(
    body: &str,
    source_sha256: &str,
    error_category: Option<&str>,
    retry_policy: Option<UnreadableRetryPolicy>,
    created_at: std::time::SystemTime,
) -> String {
    let mut metadata = format!("{NOTE_SOURCE_SHA256_PREFIX}{source_sha256}\n");
    if let Some(category) = error_category {
        metadata.push_str(&format!("{NOTE_ERROR_CATEGORY_PREFIX}{category}\n"));
    }
    if let Some(policy) = retry_policy {
        match policy {
            UnreadableRetryPolicy::ContentChange => {
                metadata.push_str(&format!("{NOTE_RETRY_MODE_PREFIX}content_change\n"));
            }
            UnreadableRetryPolicy::Timed(delay) => {
                let retry_at = unix_time_ms(created_at + delay);
                metadata.push_str(&format!("{NOTE_RETRY_MODE_PREFIX}timed\n"));
                metadata.push_str(&format!("{NOTE_RETRY_AFTER_PREFIX}{retry_at}\n"));
            }
        }
    }
    format!("{}\n---\n[dokkomplekt]\n{metadata}", body.trim_end())
}

fn note_with_source_metadata(
    body: &str,
    size_bytes: u64,
    modified_unix_ms: u128,
    error_category: Option<&str>,
    retry_policy: Option<UnreadableRetryPolicy>,
    created_at: std::time::SystemTime,
) -> String {
    let mut metadata = format!(
        "{NOTE_SOURCE_SIZE_PREFIX}{size_bytes}\n{NOTE_SOURCE_MTIME_PREFIX}{modified_unix_ms}\n"
    );
    if let Some(category) = error_category {
        metadata.push_str(&format!("{NOTE_ERROR_CATEGORY_PREFIX}{category}\n"));
    }
    if let Some(policy) = retry_policy {
        match policy {
            UnreadableRetryPolicy::ContentChange => {
                metadata.push_str(&format!("{NOTE_RETRY_MODE_PREFIX}content_change\n"));
            }
            UnreadableRetryPolicy::Timed(delay) => {
                metadata.push_str(&format!("{NOTE_RETRY_MODE_PREFIX}timed\n"));
                metadata.push_str(&format!(
                    "{NOTE_RETRY_AFTER_PREFIX}{}\n",
                    unix_time_ms(created_at + delay)
                ));
            }
        }
    }
    format!("{}\n---\n[dokkomplekt]\n{metadata}", body.trim_end())
}

fn note_matches_source_content(note_path: &Path, source: &Path) -> bool {
    if !note_path.is_file() {
        return false;
    }
    let Ok(note) = std::fs::read_to_string(note_path) else {
        return false;
    };
    if let Some(expected) = note
        .lines()
        .find_map(|line| line.trim().strip_prefix(NOTE_SOURCE_SHA256_PREFIX))
        .filter(|digest| digest.len() == 64 && digest.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return file_content_signature(source)
            .map(|(_, _, actual)| actual.eq_ignore_ascii_case(expected))
            .unwrap_or(false);
    }
    let expected_size = note
        .lines()
        .find_map(|line| line.trim().strip_prefix(NOTE_SOURCE_SIZE_PREFIX))
        .and_then(|value| value.parse::<u64>().ok());
    let expected_modified = note
        .lines()
        .find_map(|line| line.trim().strip_prefix(NOTE_SOURCE_MTIME_PREFIX))
        .and_then(|value| value.parse::<u128>().ok());
    let Ok(metadata) = std::fs::metadata(source) else {
        return false;
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis())
        .unwrap_or_default();
    expected_size == Some(metadata.len()) && expected_modified == Some(modified)
}

fn unreadable_note_blocks_retry(
    note_path: &Path,
    source: &Path,
    now: std::time::SystemTime,
) -> bool {
    if !note_matches_source_content(note_path, source) {
        return false;
    }
    let Ok(note) = std::fs::read_to_string(note_path) else {
        return false;
    };
    let retry_mode = note.lines().find_map(|line| {
        line.trim()
            .strip_prefix(NOTE_RETRY_MODE_PREFIX)
            .map(str::trim)
    });
    match retry_mode {
        Some("content_change") => true,
        Some("timed") => note
            .lines()
            .find_map(|line| line.trim().strip_prefix(NOTE_RETRY_AFTER_PREFIX))
            .and_then(|value| value.parse::<u128>().ok())
            .is_some_and(|retry_after| unix_time_ms(now) < retry_after),
        _ => false,
    }
}

#[derive(Debug, Clone)]
struct FileStabilityObservation {
    size_bytes: u64,
    modified_unix_ms: u128,
    sha256: String,
    identical_observations: u8,
    last_seen: std::time::SystemTime,
}

fn observe_file_stability(
    path: &Path,
    observations: &mut HashMap<PathBuf, FileStabilityObservation>,
    now: std::time::SystemTime,
) -> Result<bool, String> {
    let (size_bytes, modified_unix_ms, sha256) = file_content_signature(path)?;
    use std::collections::hash_map::Entry;
    match observations.entry(path.to_path_buf()) {
        Entry::Vacant(slot) => {
            slot.insert(FileStabilityObservation {
                size_bytes,
                modified_unix_ms,
                sha256,
                identical_observations: 0,
                last_seen: now,
            });
            Ok(false)
        }
        Entry::Occupied(mut slot) => {
            let entry = slot.get_mut();
            if entry.size_bytes == size_bytes
                && entry.modified_unix_ms == modified_unix_ms
                && entry.sha256 == sha256
            {
                entry.identical_observations = entry.identical_observations.saturating_add(1);
            } else {
                entry.size_bytes = size_bytes;
                entry.modified_unix_ms = modified_unix_ms;
                entry.sha256 = sha256;
                entry.identical_observations = 0;
            }
            entry.last_seen = now;
            Ok(entry.identical_observations >= 1)
        }
    }
}

fn prune_file_stability_observations(
    observations: &mut HashMap<PathBuf, FileStabilityObservation>,
    now: std::time::SystemTime,
) {
    observations.retain(|path, observation| {
        path.exists()
            && now
                .duration_since(observation.last_seen)
                .unwrap_or_default()
                < Duration::from_secs(10 * 60)
    });
}

fn write_unreadable_source_note_with_classifier(
    source: &Path,
    error: &str,
    now: std::time::SystemTime,
    classifier: fn(&str) -> (&'static str, UnreadableRetryPolicy),
) -> Result<PathBuf, String> {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Документ");
    let note_path = source.with_file_name(unreadable_note_file_name(stem));
    let safe_error = error
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (category, retry_policy) = classifier(error);
    let source_kind = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_uppercase())
        .unwrap_or_else(|| "ФАЙЛ".to_string());
    let next_action = match retry_policy {
        UnreadableRetryPolicy::ContentChange => format!(
            "Проверьте исходный {source_kind}, исправьте или пересохраните его. После изменения содержимого программа повторит обработку автоматически."
        ),
        UnreadableRetryPolicy::Timed(delay) => format!(
            "Исходный файл менять не требуется. Программа повторит попытку автоматически примерно через {} мин.; после устранения причины можно также запустить повтор из центра обработки.",
            delay.as_secs().div_ceil(60)
        ),
    };
    let body = format!(
        "ДОКУМЕНТ НЕ ОБРАБОТАН\n\nТип ошибки: {category}\nИсточник: {source_kind}\nПричина: {}\n\nЧто сделать:\n{next_action}\n\nСистема не будет бесконечно повторять ошибку: постоянные дефекты источника ждут изменения файла, временные сбои повторяются с ограниченной задержкой.\n",
        if safe_error.is_empty() { "неизвестная ошибка" } else { safe_error.as_str() }
    );
    let metadata = std::fs::metadata(source).map_err(|error| error.to_string())?;
    let note_body = if metadata.len() <= universal_intake::MAX_SOURCE_FILE_BYTES {
        let (_, _, source_sha256) = file_content_signature(source)?;
        note_with_source_fingerprint(
            &body,
            &source_sha256,
            Some(category),
            Some(retry_policy),
            now,
        )
    } else {
        let modified_unix_ms = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_millis())
            .unwrap_or_default();
        note_with_source_metadata(
            &body,
            metadata.len(),
            modified_unix_ms,
            Some(category),
            Some(retry_policy),
            now,
        )
    };
    std::fs::write(&note_path, note_body).map_err(|write_error| {
        format!("Не удалось записать понятное сообщение об ошибке: {write_error}")
    })?;
    Ok(note_path)
}

fn write_unreadable_source_note(
    source: &Path,
    error: &str,
    now: std::time::SystemTime,
) -> Result<PathBuf, String> {
    write_unreadable_source_note_with_classifier(source, error, now, classify_processing_error)
}

fn write_watcher_configuration_note(
    source: &Path,
    error: &str,
    now: std::time::SystemTime,
) -> Result<PathBuf, String> {
    write_unreadable_source_note_with_classifier(
        source,
        error,
        now,
        classify_watcher_configuration_error,
    )
}

fn default_parallel_cases() -> usize {
    2
}

fn normalize_parallel_cases(value: usize) -> usize {
    value.clamp(1, 4)
}

fn process_watcher_source(
    app: tauri::AppHandle,
    path: PathBuf,
    control_path: Option<PathBuf>,
    log_path: PathBuf,
) {
    use std::io::Write;
    let runtime = match control_path
        .as_deref()
        .ok_or_else(|| "Настройки фонового агента не найдены.".to_string())
        .and_then(read_watcher_runtime_config)
    {
        Ok(runtime) => runtime,
        Err(error) => {
            increment_metric(&app, "failed_sources", 1);
            let note = write_watcher_configuration_note(&path, &error, std::time::SystemTime::now()).ok();
            let response = CreatedDocumentsIntakeResponse {
                status: "attention".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: note.as_ref().map(|value| value.display().to_string()),
                print_triage: None,
                message: "Фоновая обработка остановлена: настройки агента недоступны. Исходник сохранён; исправьте настройки и повторите обработку.".into(),
            };
            let _ = app.emit("document-batch-ready", response);
            return;
        }
    };
    let output_root = match effective_watcher_output_root(&app, &runtime) {
        Ok(path) => path,
        Err(error) => {
            increment_metric(&app, "failed_sources", 1);
            let note = write_watcher_configuration_note(&path, &error, std::time::SystemTime::now()).ok();
            let response = CreatedDocumentsIntakeResponse {
                status: "attention".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: note.as_ref().map(|value| value.display().to_string()),
                print_triage: None,
                message: "Фоновая обработка остановлена: папка готовых документов не подтверждена. Исходник сохранён.".into(),
            };
            let _ = app.emit("document-batch-ready", response);
            return;
        }
    };
    let folder_parts = effective_watcher_folder_parts(&runtime);
    let default_year = current_year_utc();
    let sick_leave_enabled = runtime.sick_leave_enabled;
    let fallback_auto_print = runtime.auto_print;
    let fallback_copies = runtime.print_copies_by_document.clone();

    // Donor parity: a stable primary dropped while the main UI is closed must
    // open the program. Launch the normal singleton path, never the hidden
    // watcher window; an existing UI receives its ordinary activation request.
    if launch_or_activate_watcher_ui(control_path.as_deref()).is_err() {
        if let Ok(mut log) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let _ = writeln!(log, "[watcher] ui_activation_failed=true");
        }
    }
    let req = CreatedDocumentsIntakeRequest {
        source_path: path.display().to_string(),
        output_root: output_root.display().to_string(),
        folder_parts,
        default_year,
        sick_leave_enabled,
        model_output: None,
        confirmed_fields: Vec::new(),
        confirmed_document_ids: Vec::new(),
        force_reissue: false,
        preserve_source_after_success: false,
        resume_from_case_id: None,
    };
    let state = app.state::<AppState>();
    match perform_created_documents_intake(&state, &app, req) {
        Ok(response) => {
            let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or_default();
            let unreadable_note = path.with_file_name(unreadable_note_file_name(stem));
            let _ = std::fs::remove_file(&unreadable_note);
            let _ = app.emit("document-batch-ready", response.clone());
            let (latest_runtime, control_error) = match control_path.as_deref() {
                None => (None, Some("Настройки фонового агента не найдены.".to_string())),
                Some(control_path) => match read_watcher_runtime_config(control_path) {
                    Ok(runtime) => (Some(runtime), None),
                    Err(error) => (None, Some(error)),
                },
            };
            if let Some(error) = control_error.as_deref() {
                let details = serde_json::json!({ "error": error });
                let _ = create_automation_exception(
                    &app,
                    "watcher_control_unavailable",
                    response.patient_folder.as_deref().unwrap_or(""),
                    "Автопечать отключена: настройки фонового агента недоступны или повреждены.",
                    &details,
                );
                let _ = append_audit_event(
                    &app,
                    "automatic_print_blocked_control_error",
                    "",
                    &details,
                );
            }
            let effective_auto_print = if control_error.is_some() {
                false
            } else {
                latest_runtime
                    .as_ref()
                    .map(|runtime| runtime.auto_print)
                    .unwrap_or(fallback_auto_print)
            };
            let effective_copies = latest_runtime
                .as_ref()
                .map(|runtime| runtime.print_copies_by_document.clone())
                .unwrap_or(fallback_copies);
            if effective_auto_print && response.status == "processed" {
                let triage = response.print_triage.as_ref();
                if triage.is_some_and(|report| report.auto_print_allowed) {
                    let jobs = response
                        .created_documents
                        .iter()
                        .map(|document| {
                            let copies = effective_copies
                                .get(&document.document_id)
                                .copied()
                                .unwrap_or(1);
                            (PathBuf::from(&document.path), copies)
                        })
                        .collect::<Vec<_>>();
                    let print_preferences = match load_print_preferences(&app) {
                        Ok(preferences) => preferences,
                        Err(error) => {
                            increment_metric(&app, "print_failures", jobs.len() as u64);
                            let details = serde_json::json!({
                                "error": error,
                                "blocked_job_count": jobs.len(),
                            });
                            let _ = create_automation_exception(
                                &app,
                                "print_preferences_unavailable",
                                response.patient_folder.as_deref().unwrap_or(""),
                                "Автопечать не выполнялась: настройки принтера недоступны или повреждены.",
                                &details,
                            );
                            let _ = append_audit_event(
                                &app,
                                "automatic_print_blocked_preferences_error",
                                "",
                                &details,
                            );
                            if let Ok(mut log) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&log_path)
                            {
                                let _ = writeln!(
                                    log,
                                    "[watcher] automatic_print_blocked; print_preferences_unavailable=true"
                                );
                            }
                            return;
                        }
                    };
                    let print_result = print_resolved_jobs(&jobs, &print_preferences);
                    if !print_result.failed_files.is_empty() {
                        increment_metric(&app, "print_failures", print_result.failed_files.len() as u64);
                        let details = serde_json::to_value(&print_result).unwrap_or_else(|_| {
                            serde_json::json!({ "failed_count": print_result.failed_files.len() })
                        });
                        let _ = create_automation_exception(
                            &app,
                            "print_failure",
                            response.patient_folder.as_deref().unwrap_or(""),
                            "Не все документы были отправлены на печать.",
                            &details,
                        );
                        let _ = append_audit_event(&app, "automatic_print_failed", "", &details);
                        if let Ok(mut log) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&log_path)
                        {
                            let _ = writeln!(
                                log,
                                "[watcher] automatic_print_failed; failed_count={}",
                                print_result.failed_files.len()
                            );
                        }
                    } else if !print_result.queued_files.is_empty() {
                        let details = serde_json::to_value(&print_result).unwrap_or_default();
                        let _ = append_audit_event(
                            &app,
                            "automatic_print_queued_after_triage",
                            "",
                            &details,
                        );
                    }
                } else {
                    let details = triage
                        .and_then(|report| serde_json::to_value(report).ok())
                        .unwrap_or_else(|| {
                            serde_json::json!({ "reason": "confidence triage unavailable" })
                        });
                    let _ = append_audit_event(
                        &app,
                        "automatic_print_blocked_review_required",
                        "",
                        &details,
                    );
                    if let Ok(mut log) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log_path)
                    {
                        let _ = writeln!(
                            log,
                            "[watcher] automatic_print_blocked; review_required=true"
                        );
                    }
                }
            }
        }
        Err(error) => {
            increment_metric(&app, "failed_sources", 1);
            let details = serde_json::json!({ "error": &error });
            let _ = create_automation_exception(
                &app,
                "processing_error",
                &path.display().to_string(),
                "Фоновый агент не смог обработать источник.",
                &details,
            );
            let _ = append_audit_event(&app, "watcher_intake_failed", "", &details);
            let now = std::time::SystemTime::now();
            let note = write_unreadable_source_note(&path, &error, now).ok();
            let response = CreatedDocumentsIntakeResponse {
                status: "attention".into(),
                patient_folder: None,
                created_files: Vec::new(),
                created_documents: Vec::new(),
                missing: Vec::new(),
                attention_file: note.as_ref().map(|path| path.display().to_string()),
                print_triage: None,
                message: "Документ не обработан. Рядом создан диагностический файл «НЕ ПРОЧИТАН»; постоянная ошибка ждёт исправления источника, временная будет повторена автоматически.".into(),
            };
            let _ = app.emit("document-batch-ready", response);
            if let Ok(mut log) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                let _ = writeln!(
                    log,
                    "[watcher] source_processing_failed; visible_note={}",
                    note.is_some()
                );
            }
        }
    }
}

fn start_watcher_thread(
    app: tauri::AppHandle,
    config: WatcherRuntimeConfig,
    terminate_app_when_disabled: bool,
) -> Result<Arc<AtomicBool>, String> {
    let watch_folder = config.watch_folder.clone();
    let max_parallel_cases = config.max_parallel_cases;
    let handoff_owner = config.handoff_owner.clone();
    if let Some(owner) = handoff_owner.as_ref().filter(|owner| owner.ready) {
        if !watcher_owner_matches_current(owner)? {
            return Err(
                "Этот фоновый агент устарел: конфигурация уже принадлежит другому проверенному executable."
                    .into(),
            );
        }
    }
    let max_parallel_cases = normalize_parallel_cases(max_parallel_cases);
    let folder = PathBuf::from(watch_folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        let mut guard = state.watcher.lock().map_err(|_| "state lock failed")?;
        if let Some(existing) = guard.take() {
            existing.stop.store(true, Ordering::SeqCst);
        }
        *guard = Some(WatcherHandle {
            stop: Arc::clone(&stop),
            folder: folder.clone(),
        });
    }

    let thread_stop = Arc::clone(&stop);
    let control_path = watcher_config_path(&app).ok();
    let captured_owner = handoff_owner.clone();
    let log_path = app
        .path()
        .app_data_dir()
        .map(|d| d.join("watcher.log"))
        .unwrap_or_else(|_| folder.join("watcher.log"));
    std::thread::spawn(move || {
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let mut native_watcher = notify::recommended_watcher(move |event| {
            let _ = event_tx.send(event);
        })
        .ok();
        if let Some(watcher) = native_watcher.as_mut() {
            if watcher.watch(&folder, RecursiveMode::NonRecursive).is_err() {
                native_watcher = None;
            }
        }
        let mut last_hygiene = std::time::UNIX_EPOCH;
        let mut last_fallback_scan = std::time::UNIX_EPOCH;
        let mut pending_paths: BTreeSet<PathBuf> = BTreeSet::new();
        let mut stability_observations: HashMap<PathBuf, FileStabilityObservation> = HashMap::new();
        let in_flight = Arc::new(Mutex::new(BTreeSet::<PathBuf>::new()));
        while !thread_stop.load(Ordering::SeqCst) {
            let now = std::time::SystemTime::now();
            if now.duration_since(last_hygiene).unwrap_or_default()
                >= Duration::from_secs(15 * 60)
            {
                if let Ok(privacy) = load_privacy_preferences(&app) {
                    if let Ok(report) = workspace_hygiene::cleanup_workspace_folder(
                        &folder,
                        &privacy.retention_policy(),
                        now,
                    ) {
                        if !report.archived_processed_sources.is_empty()
                            || !report.archived_service_files.is_empty()
                            || !report.removed_orphan_markers.is_empty()
                            || !report.removed_expired_archived_files.is_empty()
                            || !report.removed_queue_receipts.is_empty()
                            || !report.warnings.is_empty()
                        {
                            let details = serde_json::to_value(&report).unwrap_or_default();
                            let _ = append_audit_event(&app, "workspace_hygiene_completed", "", &details);
                        }
                    }
                }
                last_hygiene = now;
            }
            if control_path.as_ref().map(|path| !path.exists()).unwrap_or(false) {
                if terminate_app_when_disabled {
                    app.exit(0);
                }
                return;
            }

            // Handoff is fail-safe and drain-first. A pending owner never retires
            // the current watcher. Once a newer ready owner is published, stop
            // admitting work, drain active cases, release the watcher singleton,
            // verify the target by SHA-256 and launch the successor without shell.
            if let Some(latest_owner) = latest_ready_watcher_owner(control_path.as_deref()) {
                if watcher_owner_superseded(captured_owner.as_ref(), Some(&latest_owner)) {
                    let active = in_flight.lock().map(|items| items.len()).unwrap_or(1);
                    if active == 0
                        && handoff_watcher_to_successor(&app, &latest_owner).is_ok()
                    {
                        if terminate_app_when_disabled {
                            app.exit(0);
                        }
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }

            while let Ok(event_result) = event_rx.try_recv() {
                if let Ok(event) = event_result {
                    for path in event.paths {
                        pending_paths.insert(path);
                    }
                }
            }
            if now.duration_since(last_fallback_scan).unwrap_or_default()
                >= Duration::from_secs(30)
            {
                if let Ok(entries) = std::fs::read_dir(&folder) {
                    for entry in entries.flatten() {
                        pending_paths.insert(entry.path());
                    }
                }
                last_fallback_scan = now;
            }
            let candidates = pending_paths.iter().cloned().collect::<Vec<_>>();
            for path in candidates {
                if thread_stop.load(Ordering::SeqCst) {
                    return;
                }
                let is_supported = universal_intake::is_supported_path(&path);
                let is_temp = universal_intake::is_temporary_source(&path);
                if !path.is_file() || !is_supported || is_temp {
                    pending_paths.remove(&path);
                    stability_observations.remove(&path);
                    continue;
                }
                if let Err(error) = universal_intake::validate_source_file_size(&path) {
                    let _ = write_unreadable_source_note(&path, &error, now);
                    pending_paths.remove(&path);
                    stability_observations.remove(&path);
                    continue;
                }
                if !observe_file_stability(&path, &mut stability_observations, now)
                    .unwrap_or(false)
                {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                let attention_note = path.with_file_name(attention_file_name(stem));
                let unreadable_note = path.with_file_name(unreadable_note_file_name(stem));
                if note_matches_source_content(&attention_note, &path)
                    || unreadable_note_blocks_retry(&unreadable_note, &path, now)
                {
                    pending_paths.remove(&path);
                    stability_observations.remove(&path);
                    continue;
                }
                let can_start = in_flight
                    .lock()
                    .map(|active| active.len() < max_parallel_cases)
                    .unwrap_or(false);
                if !can_start {
                    continue;
                }
                let inserted = in_flight
                    .lock()
                    .map(|mut active| active.insert(path.clone()))
                    .unwrap_or(false);
                if !inserted {
                    pending_paths.remove(&path);
                    stability_observations.remove(&path);
                    continue;
                }
                pending_paths.remove(&path);
                stability_observations.remove(&path);
                let worker_app = app.clone();
                let worker_path = path.clone();
                let worker_control_path = control_path.clone();
                let worker_log_path = log_path.clone();
                let worker_in_flight = Arc::clone(&in_flight);
                std::thread::spawn(move || {
                    let processing_app = worker_app.clone();
                    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_watcher_source(
                            processing_app,
                            worker_path.clone(),
                            worker_control_path,
                            worker_log_path.clone(),
                        );
                    }));
                    if panic_result.is_err() {
                        increment_metric(&worker_app, "failed_sources", 1);
                        let error = "internal watcher panic: обработка аварийно остановлена до безопасной публикации";
                        let now = std::time::SystemTime::now();
                        let note = write_unreadable_source_note(&worker_path, error, now).ok();
                        let details = serde_json::json!({
                            "category": "internal_failure",
                            "source_content_not_logged": true,
                            "retry_requires_source_change": true,
                        });
                        let _ = create_automation_exception(
                            &worker_app,
                            "internal_failure",
                            &worker_path.display().to_string(),
                            "Внутренняя ошибка фонового обработчика. Исходник сохранён и не будет зациклен.",
                            &details,
                        );
                        let _ = append_audit_event(
                            &worker_app,
                            "watcher_worker_panic_blocked",
                            "",
                            &details,
                        );
                        let response = CreatedDocumentsIntakeResponse {
                            status: "attention".into(),
                            patient_folder: None,
                            created_files: Vec::new(),
                            created_documents: Vec::new(),
                            missing: Vec::new(),
                            attention_file: note.as_ref().map(|path| path.display().to_string()),
                            print_triage: None,
                            message: "Фоновая обработка аварийно остановлена. Исходник не удалён; рядом создан файл «НЕ ПРОЧИТАН», повторный цикл заблокирован до изменения источника.".into(),
                        };
                        let _ = worker_app.emit("document-batch-ready", response);
                        if let Ok(mut log) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&worker_log_path)
                        {
                            let _ = writeln!(log, "[watcher] worker_panic; retry_blocked=true");
                        }
                    }
                    if let Ok(mut active) = worker_in_flight.lock() {
                        active.remove(&worker_path);
                    }
                });
            }
            prune_file_stability_observations(&mut stability_observations, now);
            if native_watcher.is_some() {
                if let Ok(Ok(event)) = event_rx.recv_timeout(Duration::from_secs(2)) {
                    for path in event.paths {
                        pending_paths.insert(path);
                    }
                }
            } else {
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    });
    Ok(stop)
}

#[tauri::command]
fn get_background_watcher_state(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let config_path = watcher_config_path(&app)?;
    if !config_path.exists() {
        return Ok(serde_json::json!({
            "platform": std::env::consts::OS,
            "installed": false,
            "migration_required": false,
        }));
    }
    let runtime = read_watcher_runtime_config(&config_path)?;
    let migration_required = runtime.output_root.trim().is_empty();
    let effective_output = effective_watcher_output_root(&app, &runtime)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    Ok(serde_json::json!({
        "platform": std::env::consts::OS,
        "installed": true,
        "watch_folder": runtime.watch_folder,
        "output_root": effective_output,
        "folder_parts": effective_watcher_folder_parts(&runtime),
        "auto_print": runtime.auto_print,
        "print_copies_by_document": runtime.print_copies_by_document,
        "max_parallel_cases": normalize_parallel_cases(runtime.max_parallel_cases),
        "migration_required": migration_required,
    }))
}

#[tauri::command]
fn install_background_watcher(
    req: WatcherInstallRequest,
    app: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    let watch_folder = resolve_user_visible_absolute_path(&req.watch_folder, "Рабочая папка фонового агента")?;
    ensure_output_root_path(&watch_folder)?;
    let output_root = resolve_user_visible_absolute_path(&req.output_root, "Папка готовых документов")?;
    ensure_output_root_path(&output_root)?;
    if watcher_directories_are_same(&watch_folder, &output_root)? {
        return Err("Рабочая папка фонового агента и папка готовых документов должны быть разными.".into());
    }
    let default_year = req.default_year.unwrap_or_else(current_year_utc);
    validate_output_folder_parts(&req.folder_parts)?;
    let folder_parts = req.folder_parts.clone();
    if let Some((document_id, copies)) = req
        .print_copies_by_document
        .iter()
        .find(|(_, copies)| **copies > MAX_PRINT_COPIES)
    {
        return Err(format!(
            "Количество копий для документа «{document_id}» ({copies}) превышает предел {MAX_PRINT_COPIES}."
        ));
    }

    let mut owner = watcher_owner_for_executable(&exe, false)?;
    let mut runtime = WatcherRuntimeConfig {
        watch_folder: watch_folder.display().to_string(),
        output_root: output_root.display().to_string(),
        default_year,
        sick_leave_enabled: req.sick_leave_enabled,
        folder_parts: folder_parts.clone(),
        auto_print: req.auto_print,
        print_copies_by_document: req.print_copies_by_document.clone(),
        max_parallel_cases: normalize_parallel_cases(req.max_parallel_cases),
        handoff_owner: Some(owner.clone()),
    };
    let config_path = watcher_config_path(&app)?;
    let previous_config = if config_path.exists() {
        Some(
            std::fs::read(&config_path)
                .map_err(|error| format!("Не удалось прочитать предыдущие настройки фонового агента: {error}"))?,
        )
    } else {
        None
    };
    // Phase 1: publish a pending owner. Old watchers deliberately do not retire
    // until OS autostart has been proven and the ready phase is durable.
    atomic_write_file(
        &config_path,
        &serde_json::to_vec_pretty(&runtime).map_err(|e| e.to_string())?,
    )?;

    let (autostart_files, warnings) = match write_autostart_entries(&exe) {
        Ok(result) => result,
        Err(error) => {
            let rollback = match previous_config.as_deref() {
                Some(bytes) => atomic_write_file(&config_path, bytes),
                None => {
                    if config_path.exists() {
                        std::fs::remove_file(&config_path).map_err(|e| e.to_string())
                    } else {
                        Ok(())
                    }
                }
            };
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!(
                    "{error}; дополнительно не удалось восстановить настройки: {rollback_error}"
                )),
            };
        }
    };

    // Phase 2: only after autostart is durable may the new executable become the
    // retirement target for a stale owner.
    owner.ready = true;
    runtime.handoff_owner = Some(owner.clone());
    if let Err(error) = atomic_write_file(
        &config_path,
        &serde_json::to_vec_pretty(&runtime).map_err(|e| e.to_string())?,
    ) {
        let rollback = match previous_config.as_deref() {
            Some(bytes) => atomic_write_file(&config_path, bytes),
            None => {
                let (_, _) = remove_autostart_entries();
                if config_path.exists() {
                    std::fs::remove_file(&config_path).map_err(|e| e.to_string())
                } else {
                    Ok(())
                }
            }
        };
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!("{error}; откат не завершён: {rollback_error}")),
        };
    }

    let max_parallel_cases = runtime.max_parallel_cases;
    if let Err(error) = start_watcher_thread(app.clone(), runtime, false) {
        let rollback_config = match previous_config.as_deref() {
            Some(bytes) => atomic_write_file(&config_path, bytes),
            None => {
                if config_path.exists() {
                    std::fs::remove_file(&config_path).map_err(|e| e.to_string())
                } else {
                    Ok(())
                }
            }
        };
        let rollback_autostart = if previous_config.is_none() {
            let (_, rollback_warnings) = remove_autostart_entries();
            if rollback_warnings.is_empty() {
                Ok(())
            } else {
                Err(rollback_warnings.join("; "))
            }
        } else {
            Ok(())
        };
        let mut failures = Vec::new();
        if let Err(rollback_error) = rollback_config {
            failures.push(format!("восстановление настроек: {rollback_error}"));
        }
        if let Err(rollback_error) = rollback_autostart {
            failures.push(format!("откат автозапуска: {rollback_error}"));
        }
        if failures.is_empty() {
            return Err(error);
        }
        return Err(format!("{error}; ошибки отката: {}", failures.join("; ")));
    }

    Ok(serde_json::json!({
        "platform": std::env::consts::OS,
        "installed": true,
        "watch_folder": watch_folder.display().to_string(),
        "output_root": output_root.display().to_string(),
        "folder_parts": folder_parts,
        "auto_print": req.auto_print,
        "print_copies_by_document": req.print_copies_by_document,
        "executable": exe.display().to_string(),
        "executable_sha256": owner.executable_sha256,
        "handoff_generation": owner.generation,
        "args": ["--background-watch"],
        "autostart_files": autostart_files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "warnings": warnings,
        "autostart_state_file": config_path.display().to_string(),
        "max_parallel_cases": max_parallel_cases,
    }))
}

#[derive(Debug, Deserialize)]
struct WatcherPreferencesRequest {
    output_root: String,
    #[serde(default)]
    folder_parts: Vec<FolderNamePart>,
    #[serde(default)]
    auto_print: bool,
    #[serde(default)]
    print_copies_by_document: BTreeMap<String, u16>,
}

#[tauri::command]
fn update_background_watcher_preferences(
    req: WatcherPreferencesRequest,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    if let Some((document_id, copies)) = req
        .print_copies_by_document
        .iter()
        .find(|(_, copies)| **copies > MAX_PRINT_COPIES)
    {
        return Err(format!(
            "Количество копий для документа «{document_id}» ({copies}) превышает предел {MAX_PRINT_COPIES}."
        ));
    }
    let config_path = watcher_config_path(&app)?;
    if !config_path.exists() {
        return Ok(false);
    }
    let bytes = std::fs::read(&config_path).map_err(|error| error.to_string())?;
    let mut runtime: WatcherRuntimeConfig = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Настройки фонового агента повреждены: {error}"))?;
    let output_root = resolve_user_visible_absolute_path(&req.output_root, "Папка готовых документов")?;
    ensure_output_root_path(&output_root)?;
    let watch_folder = resolve_user_visible_absolute_path(
        &runtime.watch_folder,
        "Рабочая папка фонового агента",
    )?;
    ensure_output_root_path(&watch_folder)?;
    if watcher_directories_are_same(&watch_folder, &output_root)? {
        return Err("Рабочая папка фонового агента и папка готовых документов должны быть разными.".into());
    }
    runtime.output_root = output_root.display().to_string();
    validate_output_folder_parts(&req.folder_parts)?;
    runtime.folder_parts = req.folder_parts;
    runtime.default_year = current_year_utc();
    runtime.auto_print = req.auto_print;
    runtime.print_copies_by_document = req.print_copies_by_document;
    atomic_write_file(
        &config_path,
        &serde_json::to_vec_pretty(&runtime).map_err(|error| error.to_string())?,
    )?;
    Ok(true)
}

#[tauri::command]
fn uninstall_background_watcher(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let stopped_folder = {
        let mut guard = state.watcher.lock().map_err(|_| "state lock failed")?;
        guard.take().map(|handle| {
            handle.stop.store(true, Ordering::SeqCst);
            handle.folder.display().to_string()
        })
    };
    let (removed, warnings) = remove_autostart_entries();
    if !warnings.is_empty() {
        return Err(format!(
            "Фоновый агент остановлен в текущем сеансе, но автозапуск не удалён: {}. Конфигурация сохранена, чтобы проблема не маскировалась как успешное отключение.",
            warnings.join("; ")
        ));
    }
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let plan_file = app_data.join("intake-agent-autostart.json");
    if plan_file.exists() {
        std::fs::remove_file(&plan_file).map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({
        "platform": std::env::consts::OS,
        "installed": false,
        "watch_folder": stopped_folder,
        "removed_files": removed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "warnings": warnings,
    }))
}

#[cfg(test)]
mod watcher_handoff_tests {
    use super::*;

    fn owner(generation: &str, ready: bool) -> WatcherHandoffOwner {
        WatcherHandoffOwner {
            generation: generation.into(),
            executable: "/install/app.exe".into(),
            executable_sha256: "a".repeat(64),
            ready,
        }
    }

    #[test]
    fn pending_owner_never_retires_running_watcher() {
        assert!(!watcher_owner_superseded(
            Some(&owner("A", true)),
            Some(&owner("B", false))
        ));
    }

    #[test]
    fn newer_ready_owner_retires_stale_watcher() {
        assert!(watcher_owner_superseded(
            Some(&owner("A", true)),
            Some(&owner("B", true))
        ));
    }

    #[test]
    fn current_ready_owner_stays_in_charge() {
        assert!(!watcher_owner_superseded(
            Some(&owner("B", true)),
            Some(&owner("B", true))
        ));
    }

    #[test]
    fn legacy_watcher_can_handoff_once_ready_owner_exists() {
        assert!(watcher_owner_superseded(None, Some(&owner("B", true))));
    }

    #[test]
    fn legacy_runtime_deserializes_fail_closed_without_output_root() {
        let runtime: WatcherRuntimeConfig = serde_json::from_value(serde_json::json!({
            "watch_folder": "C:/Watch",
            "default_year": 2026,
            "sick_leave_enabled": false,
            "folder_parts": ["DocumentNumber", "DocumentDate"],
            "auto_print": false,
            "print_copies_by_document": {},
            "max_parallel_cases": 2
        }))
        .unwrap();
        assert!(runtime.output_root.is_empty());
        assert_eq!(effective_watcher_folder_parts(&runtime).len(), 2);
    }

    #[test]
    fn watcher_runtime_keeps_source_and_destination_as_distinct_fields() {
        let runtime = WatcherRuntimeConfig {
            watch_folder: "C:/Inbox".into(),
            output_root: "D:/Ready".into(),
            default_year: 2026,
            sick_leave_enabled: false,
            folder_parts: default_output_folder_parts(),
            auto_print: false,
            print_copies_by_document: BTreeMap::new(),
            max_parallel_cases: 2,
            handoff_owner: None,
        };
        assert_ne!(runtime.watch_folder, runtime.output_root);
        assert_eq!(runtime.output_root, "D:/Ready");
    }

    #[test]
    fn corrupt_watcher_configuration_is_timed_retry_not_content_change() {
        let (category, retry_policy) = classify_watcher_configuration_error(
            "Настройки фонового агента повреждены: invalid json",
        );
        assert_eq!(category, "watcher_configuration_unavailable");
        match retry_policy {
            UnreadableRetryPolicy::Timed(delay) => assert_eq!(delay, Duration::from_secs(60)),
            UnreadableRetryPolicy::ContentChange => {
                panic!("watcher configuration faults must not require source-content changes")
            }
        }
    }

    #[test]
    fn corrupt_source_content_remains_content_change_retry() {
        let (category, retry_policy) = classify_processing_error("DOCX поврежден");
        assert_eq!(category, "source_invalid");
        assert!(matches!(retry_policy, UnreadableRetryPolicy::ContentChange));
    }

    #[test]
    fn watcher_directory_identity_resolves_aliases_before_comparison() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-watcher-identity-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        let watched = root.join("inbox");
        let separate = root.join("ready");
        std::fs::create_dir_all(&watched).expect("create watched folder");
        std::fs::create_dir_all(&separate).expect("create separate folder");
        let alias = watched.join(".");
        assert!(watcher_directories_are_same(&watched, &alias).expect("compare alias"));
        assert!(!watcher_directories_are_same(&watched, &separate).expect("compare distinct"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn watcher_directory_identity_is_case_insensitive_on_windows() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-watcher-case-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create watcher case folder");
        let different_case = PathBuf::from(root.to_string_lossy().to_ascii_uppercase());
        assert!(watcher_directories_are_same(&root, &different_case).expect("compare case alias"));
        let _ = std::fs::remove_dir_all(root);
    }

}
