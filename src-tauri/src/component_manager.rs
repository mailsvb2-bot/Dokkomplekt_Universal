//! Signed, per-user optional runtime components.
//!
//! Component archives and their catalog are verified with the same Ed25519
//! update trust anchor as application updates. Extracted files are trusted only
//! when they are bound to a signed catalog descriptor and to the signed archive's
//! `component-files.json` hash manifest.

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signature as Ed25519Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration as StdDuration, SystemTime};
use tauri::Emitter as _;
use time::OffsetDateTime;
use uuid::Uuid;
use zip::ZipArchive;

const COMPONENT_CATALOG_SCHEMA: u32 = 1;
const COMPONENT_STATUS_SCHEMA: u32 = 1;
const COMPONENT_FILES_SCHEMA: u32 = 1;
const MAX_COMPONENT_CATALOG_BYTES: u64 = 256 * 1024;
const MAX_COMPONENT_CATALOG_OVERLAYS: usize = 64;
const COMPONENT_CATALOG_OVERLAYS_DIR: &str = "catalog-overlays";
const MAX_COMPONENT_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_COMPONENT_ENTRIES: usize = 50_000;
const MAX_COMPONENT_UNPACKED_BYTES: u64 = 12 * 1024 * 1024 * 1024;
const MAX_OFFLINE_COMPONENT_BUNDLE_BYTES: u64 = 17 * 1024 * 1024 * 1024;
const STALE_COMPONENT_TRANSACTION_AGE: StdDuration = StdDuration::from_secs(6 * 60 * 60);
static COMPONENT_TRANSACTION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const TRUSTED_COMPONENTS_CATALOG_URL: &str = match option_env!("DOKKOMPLEKT_COMPONENTS_CATALOG_URL")
{
    Some(url) => url,
    None => "https://updates.dokkomplekt.invalid/components-catalog.json",
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ComponentDescriptor {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub unlocks: Vec<String>,
    pub target: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub files_manifest_sha256: String,
    #[serde(default)]
    pub archive_name: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ComponentsCatalogPayload {
    pub schema: u32,
    pub app_min_version: String,
    pub published_at: String,
    /// `complete` is authoritative for the whole component set; `partial` only
    /// overlays descriptors it contains. `None` preserves signature compatibility
    /// with legacy schema-1 catalogs and is interpreted as `complete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_scope: Option<String>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    pub components: Vec<ComponentDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignedComponentsCatalog {
    pub payload: ComponentsCatalogPayload,
    pub signature_alg: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComponentFilesManifest {
    schema: u32,
    component_id: String,
    target: String,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledComponentReceipt {
    schema: u32,
    component_id: String,
    target: String,
    archive_sha256: String,
    files_manifest_sha256: String,
    installed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ComponentStatus {
    pub id: String,
    pub label: String,
    pub description: String,
    pub target: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub unlocks: Vec<String>,
    pub state: String,
    pub installed: bool,
    pub available: bool,
    pub catalog_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OfflineComponentImportResult {
    pub components: Vec<ComponentStatus>,
    pub imported_component_ids: Vec<String>,
    pub catalog_scope: String,
}

fn lock_component_transactions() -> Result<MutexGuard<'static, ()>, String> {
    COMPONENT_TRANSACTION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| {
            "Блокировка транзакций компонентов повреждена; изменение остановлено".to_string()
        })
}

#[derive(Debug)]
enum AtomicWriteError {
    BeforeCommit(String),
    AfterCommit(String),
}

impl AtomicWriteError {
    fn authority_committed(&self) -> bool {
        matches!(self, Self::AfterCommit(_))
    }

    fn into_message(self) -> String {
        match self {
            Self::BeforeCommit(message) | Self::AfterCommit(message) => message,
        }
    }
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                format!(
                    "Не удалось зафиксировать каталог {}: {error}",
                    path.display()
                )
            })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn sync_cached_catalog_authority_directories(root: &Path) -> Result<(), String> {
    sync_directory(root)?;
    let overlays = root.join(COMPONENT_CATALOG_OVERLAYS_DIR);
    if overlays.exists() {
        let metadata = std::fs::symlink_metadata(&overlays).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("Каталог partial components catalog имеет небезопасный тип".into());
        }
        sync_directory(&overlays)?;
    }
    Ok(())
}

fn uncertain_previous_path(previous: &Path) -> Option<PathBuf> {
    let name = previous.file_name()?.to_str()?;
    let stem = name.strip_suffix(".previous")?;
    Some(previous.with_file_name(format!("{stem}.durability-uncertain")))
}

fn mark_uncertain_previous_backups(committed: &[(PathBuf, Option<PathBuf>)]) {
    for (_, previous) in committed {
        let Some(previous) = previous else { continue };
        let Some(uncertain) = uncertain_previous_path(previous) else {
            continue;
        };
        // This rename is best effort because the catalog directory already failed
        // its durability sync. Recovery also protects plain `.previous` entries
        // while durability is unconfirmed, so a failed/lost marker is fail-safe.
        let _ = std::fs::rename(previous, uncertain);
    }
}

fn stale_component_transaction_removal_allowed(
    name: &str,
    age: StdDuration,
    catalog_durability_confirmed: bool,
) -> bool {
    if age < STALE_COMPONENT_TRANSACTION_AGE {
        return false;
    }
    let is_catalog_backup = name.ends_with(".previous") || name.ends_with(".durability-uncertain");
    !is_catalog_backup || catalog_durability_confirmed
}

#[derive(Debug, Clone, Serialize)]
struct ComponentProgress {
    id: String,
    phase: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    percent: u8,
    message: String,
}

pub(crate) fn user_components_dir() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("DOKKOMPLEKT_COMPONENTS_DIR") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    #[cfg(windows)]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Dokkomplekt").join("components"));
    }
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library")
                .join("Application Support")
                .join("Dokkomplekt")
                .join("components")
        });
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(value).join("dokkomplekt").join("components"));
        }
        return std::env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join(".local")
                .join("share")
                .join("dokkomplekt")
                .join("components")
        });
    }
    #[allow(unreachable_code)]
    None
}

pub(crate) fn resolve_trusted_component_tool(
    program: &str,
    executable_name: &str,
) -> Option<PathBuf> {
    let root = user_components_dir()?;
    let descriptors = read_effective_component_descriptors(&root).ok()?;
    let target = crate::current_update_platform();
    for descriptor in descriptors
        .iter()
        .filter(|component| component.target == target)
        .filter(|component| component.unlocks.iter().any(|item| item == program))
    {
        let component_dir = root.join(&descriptor.id);
        let manifest = match read_verified_component_manifest(&component_dir, descriptor) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let mut candidates = Vec::new();
        append_component_tool_candidates(&mut candidates, &component_dir, program, executable_name);
        if let Some(candidate) =
            resolve_component_tool_candidate(&component_dir, &manifest, candidates)
        {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn component_statuses() -> Vec<ComponentStatus> {
    let root = match user_components_dir() {
        Some(value) => value,
        None => return fallback_statuses("Пользовательская папка компонентов недоступна."),
    };
    let _transaction_guard = match lock_component_transactions() {
        Ok(value) => value,
        Err(error) => return fallback_statuses(&error),
    };
    let descriptors = match read_effective_component_descriptors(&root) {
        Ok(value) => value,
        Err(error) => {
            return fallback_statuses(&format!("Подписанный каталог ещё не загружен: {error}"))
        }
    };
    let _ = recover_component_transactions(&root, Some(&descriptors));
    statuses_from_descriptors(&root, &descriptors)
}

pub(crate) fn refresh_component_catalog(
    app: &tauri::AppHandle,
) -> Result<Vec<ComponentStatus>, String> {
    let catalog = fetch_and_verify_catalog(app)?;
    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let _transaction_guard = lock_component_transactions()?;
    guard_catalog_not_older(&root, &catalog)?;
    persist_verified_catalog(&root, &catalog).map_err(AtomicWriteError::into_message)?;
    let descriptors = read_effective_component_descriptors(&root)?;
    recover_component_transactions(&root, Some(&descriptors))?;
    Ok(statuses_from_descriptors(&root, &descriptors))
}

pub(crate) async fn install_component(
    app: tauri::AppHandle,
    id: String,
) -> Result<ComponentStatus, String> {
    let id = validate_component_id(&id)?.to_string();
    tauri::async_runtime::spawn_blocking(move || install_component_blocking(&app, &id))
        .await
        .map_err(|error| format!("Фоновая установка компонента завершилась ошибкой: {error}"))?
}

fn validate_offline_catalog_component_set(
    catalog: &SignedComponentsCatalog,
    current_target: &str,
) -> Result<(), String> {
    if catalog.payload.components.is_empty() && catalog_is_partial(catalog) {
        return Err("Partial офлайн-каталог должен содержать хотя бы один компонент".into());
    }
    if catalog
        .payload
        .components
        .iter()
        .any(|descriptor| descriptor.target != current_target)
    {
        return Err(format!(
            "Офлайн-комплект должен содержать только компоненты для {current_target}"
        ));
    }
    Ok(())
}

pub(crate) fn import_offline_component_bundle(
    app: &tauri::AppHandle,
    selected_path: &Path,
) -> Result<OfflineComponentImportResult, String> {
    if !selected_path.is_absolute() {
        return Err("Офлайн-комплект компонентов должен быть выбран по абсолютному пути".into());
    }
    let metadata = std::fs::symlink_metadata(selected_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Офлайн-комплект должен быть обычным ZIP-файлом, а не ссылкой".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_OFFLINE_COMPONENT_BUNDLE_BYTES {
        return Err("Офлайн-комплект имеет недопустимый размер".into());
    }

    emit_progress(
        app,
        "offline-bundle",
        "verify",
        0,
        metadata.len(),
        "Проверяется локальный подписанный комплект",
    );
    let file = File::open(selected_path).map_err(|error| error.to_string())?;
    let mut bundle =
        ZipArchive::new(file).map_err(|error| format!("Офлайн-комплект повреждён: {error}"))?;
    if bundle.is_empty() || bundle.len() > 64 {
        return Err("Офлайн-комплект содержит недопустимое число файлов".into());
    }

    let mut names = BTreeSet::new();
    let mut catalog_bytes = None;
    for index in 0..bundle.len() {
        let mut entry = bundle.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Папки и символические ссылки в офлайн-комплекте запрещены".into());
        }
        let name = entry.name().replace('\\', "/");
        if name.contains('/') || safe_relative_path(&name)?.components().count() != 1 {
            return Err("Офлайн-комплект должен содержать только файлы верхнего уровня".into());
        }
        if !names.insert(name.clone()) {
            return Err(format!(
                "Офлайн-комплект содержит повторяющийся файл: {name}"
            ));
        }
        if name == "components-catalog.json" {
            if entry.size() > MAX_COMPONENT_CATALOG_BYTES {
                return Err("Подписанный каталог офлайн-комплекта превышает лимит".into());
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            (&mut entry)
                .take(MAX_COMPONENT_CATALOG_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > MAX_COMPONENT_CATALOG_BYTES {
                return Err("Подписанный каталог офлайн-комплекта превышает лимит".into());
            }
            catalog_bytes = Some(bytes);
        }
    }
    let catalog_bytes = catalog_bytes
        .ok_or_else(|| "Офлайн-комплект не содержит components-catalog.json".to_string())?;
    let catalog: SignedComponentsCatalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| format!("Некорректный components catalog в офлайн-комплекте: {error}"))?;
    verify_catalog(&catalog)?;

    let current_target = crate::current_update_platform();
    validate_offline_catalog_component_set(&catalog, current_target)?;
    let mut expected_names = BTreeSet::from(["components-catalog.json".to_string()]);
    let mut descriptors_by_name = BTreeMap::new();
    for descriptor in &catalog.payload.components {
        guard_descriptor(&catalog.payload, descriptor)?;
        guard_target_matches_platform(descriptor)?;
        let archive_name = component_archive_name(descriptor)?;
        if !expected_names.insert(archive_name.clone())
            || descriptors_by_name
                .insert(archive_name.clone(), descriptor.clone())
                .is_some()
        {
            return Err(format!(
                "Подписанный каталог повторяет архив: {archive_name}"
            ));
        }
    }
    if names != expected_names {
        let missing = expected_names
            .difference(&names)
            .cloned()
            .collect::<Vec<_>>();
        let extra = names
            .difference(&expected_names)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "Содержимое офлайн-комплекта не совпадает с подписанным каталогом; missing={missing:?}; extra={extra:?}"
        ));
    }

    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;

    let mut staged = Vec::<(ComponentDescriptor, PathBuf)>::new();
    let stage_result = (|| -> Result<(), String> {
        for (archive_name, descriptor) in &descriptors_by_name {
            emit_progress(
                app,
                &descriptor.id,
                "import",
                0,
                descriptor.size_bytes,
                "Проверяется локальный архив компонента",
            );
            let temp_archive = root.join(format!(
                ".{}.{}.offline-part",
                descriptor.id,
                Uuid::new_v4()
            ));
            let copy_result = (|| -> Result<(), String> {
                let mut entry = bundle
                    .by_name(archive_name)
                    .map_err(|error| format!("Архив компонента отсутствует: {error}"))?;
                if entry.size() != descriptor.size_bytes
                    || entry.size() > MAX_COMPONENT_ARCHIVE_BYTES
                {
                    return Err(format!(
                        "Размер локального компонента {} не совпадает с подписанным каталогом",
                        descriptor.id
                    ));
                }
                let mut output = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp_archive)
                    .map_err(|error| error.to_string())?;
                let mut digest = Sha256::new();
                let mut copied = 0u64;
                let mut buffer = [0u8; 128 * 1024];
                loop {
                    let read = entry.read(&mut buffer).map_err(|error| error.to_string())?;
                    if read == 0 {
                        break;
                    }
                    copied = copied.saturating_add(read as u64);
                    if copied > descriptor.size_bytes {
                        return Err("Локальный компонент превысил подписанный размер".into());
                    }
                    digest.update(&buffer[..read]);
                    output
                        .write_all(&buffer[..read])
                        .map_err(|error| error.to_string())?;
                }
                output.sync_all().map_err(|error| error.to_string())?;
                if copied != descriptor.size_bytes
                    || !hex::encode(digest.finalize()).eq_ignore_ascii_case(&descriptor.sha256)
                {
                    return Err(format!(
                        "SHA-256 локального компонента {} не совпал с подписанным каталогом",
                        descriptor.id
                    ));
                }
                Ok(())
            })();
            if let Err(error) = copy_result {
                let _ = std::fs::remove_file(&temp_archive);
                return Err(error);
            }
            let stage = match stage_verified_component_archive(&root, descriptor, &temp_archive) {
                Ok(value) => value,
                Err(error) => {
                    let _ = std::fs::remove_file(&temp_archive);
                    return Err(error);
                }
            };
            let _ = std::fs::remove_file(&temp_archive);
            staged.push((descriptor.clone(), stage));
        }
        Ok(())
    })();
    if let Err(error) = stage_result {
        for (_, stage) in staged {
            let _ = std::fs::remove_dir_all(stage);
        }
        return Err(error);
    }

    let imported_component_ids = catalog
        .payload
        .components
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect::<Vec<_>>();
    let catalog_scope = if catalog_is_partial(&catalog) {
        "partial".to_string()
    } else {
        "complete".to_string()
    };
    let _transaction_guard = lock_component_transactions()?;
    if let Err(error) = guard_catalog_not_older(&root, &catalog) {
        for (_, stage) in staged {
            let _ = std::fs::remove_dir_all(stage);
        }
        return Err(error);
    }
    commit_staged_offline_components(&root, &catalog, staged)?;
    let sidecars = crate::universal_intake::sidecar_tool_statuses();
    for descriptor in &catalog.payload.components {
        emit_progress(
            app,
            &descriptor.id,
            "complete",
            descriptor.size_bytes,
            descriptor.size_bytes,
            "Компонент установлен из подписанного офлайн-комплекта",
        );
    }
    let descriptors = read_effective_component_descriptors(&root)?;
    Ok(OfflineComponentImportResult {
        components: statuses_from_descriptors_with_sidecars(&root, &descriptors, &sidecars),
        imported_component_ids,
        catalog_scope,
    })
}

fn component_archive_name(descriptor: &ComponentDescriptor) -> Result<String, String> {
    let explicit = descriptor.archive_name.trim();
    let name = if !explicit.is_empty() {
        explicit.to_string()
    } else {
        let url = reqwest::Url::parse(&descriptor.url)
            .map_err(|_| "Некорректный URL компонента".to_string())?;
        url.path_segments()
            .and_then(|mut segments| segments.rfind(|item| !item.is_empty()))
            .ok_or_else(|| "URL компонента не содержит имени архива".to_string())?
            .to_string()
    };
    if name.len() > 180
        || !name.to_ascii_lowercase().ends_with(".zip")
        || !name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
    {
        return Err("Некорректное имя ZIP компонента".into());
    }
    if !descriptor.url.trim().is_empty() {
        let url = reqwest::Url::parse(&descriptor.url)
            .map_err(|_| "Некорректный URL компонента".to_string())?;
        let url_name = url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|item| !item.is_empty()))
            .ok_or_else(|| "URL компонента не содержит имени архива".to_string())?;
        if url_name != name {
            return Err("Имя архива компонента не совпадает с URL".into());
        }
    }
    Ok(name)
}

fn stage_verified_component_archive(
    root: &Path,
    descriptor: &ComponentDescriptor,
    archive_path: &Path,
) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(archive_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != descriptor.size_bytes
        || !sha256_file(archive_path)?.eq_ignore_ascii_case(&descriptor.sha256)
    {
        return Err(format!(
            "Локальный архив компонента {} не прошёл проверку целостности",
            descriptor.id
        ));
    }
    let stage_dir = root.join(format!(".{}.{}.installing", descriptor.id, Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        std::fs::create_dir(&stage_dir).map_err(|error| error.to_string())?;
        safe_extract_zip(archive_path, &stage_dir)?;
        let manifest = read_component_files_manifest(&stage_dir, descriptor)?;
        validate_all_manifest_files(&stage_dir, &manifest)?;
        let receipt = InstalledComponentReceipt {
            schema: COMPONENT_STATUS_SCHEMA,
            component_id: descriptor.id.clone(),
            target: descriptor.target.clone(),
            archive_sha256: descriptor.sha256.to_ascii_lowercase(),
            files_manifest_sha256: descriptor.files_manifest_sha256.to_ascii_lowercase(),
            installed_at: OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|error| error.to_string())?,
        };
        atomic_write_json(&stage_dir.join("component-status.json"), &receipt)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(error);
    }
    Ok(stage_dir)
}

fn rollback_offline_component_commits(committed: &[(PathBuf, Option<PathBuf>)]) {
    for (final_dir, previous_dir) in committed.iter().rev() {
        let _ = std::fs::remove_dir_all(final_dir);
        if let Some(previous_dir) = previous_dir {
            let _ = std::fs::rename(previous_dir, final_dir);
        }
    }
}

fn commit_staged_offline_components(
    root: &Path,
    catalog: &SignedComponentsCatalog,
    staged: Vec<(ComponentDescriptor, PathBuf)>,
) -> Result<(), String> {
    let mut committed = Vec::<(PathBuf, Option<PathBuf>)>::new();
    for (descriptor, stage_dir) in staged {
        let final_dir = root.join(&descriptor.id);
        let previous_dir = root.join(format!(".{}.{}.previous", descriptor.id, Uuid::new_v4()));
        let previous = if final_dir.exists() {
            if let Err(error) = std::fs::rename(&final_dir, &previous_dir) {
                rollback_offline_component_commits(&committed);
                let _ = std::fs::remove_dir_all(&stage_dir);
                return Err(error.to_string());
            }
            Some(previous_dir)
        } else {
            None
        };
        if let Err(error) = std::fs::rename(&stage_dir, &final_dir) {
            if let Some(previous_dir) = &previous {
                let _ = std::fs::rename(previous_dir, &final_dir);
            }
            rollback_offline_component_commits(&committed);
            return Err(error.to_string());
        }
        committed.push((final_dir, previous));
    }
    if let Err(error) = persist_verified_catalog(root, catalog) {
        if error.authority_committed() {
            mark_uncertain_previous_backups(&committed);
        } else {
            rollback_offline_component_commits(&committed);
        }
        return Err(error.into_message());
    }
    for (_, previous) in committed {
        if let Some(previous) = previous {
            let _ = std::fs::remove_dir_all(previous);
        }
    }
    Ok(())
}

pub(crate) fn remove_component(id: &str) -> Result<ComponentStatus, String> {
    let id = validate_component_id(id)?;
    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    let _transaction_guard = lock_component_transactions()?;
    let descriptors = read_effective_component_descriptors(&root)?;
    let descriptor = descriptors
        .iter()
        .find(|component| component.id == id)
        .ok_or_else(|| "Неизвестный компонент".to_string())?;
    let component_dir = root.join(id);
    if component_dir.exists() {
        let metadata =
            std::fs::symlink_metadata(&component_dir).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("Путь компонента имеет небезопасный тип".into());
        }
        std::fs::remove_dir_all(&component_dir).map_err(|error| error.to_string())?;
    }
    let sidecars = crate::universal_intake::sidecar_tool_statuses();
    Ok(status_for_descriptor(&root, descriptor, true, &sidecars))
}

fn install_component_blocking(app: &tauri::AppHandle, id: &str) -> Result<ComponentStatus, String> {
    emit_progress(app, id, "catalog", 0, 0, "Проверяется подписанный каталог");
    let catalog = fetch_and_verify_catalog(app)?;
    let descriptor = catalog
        .payload
        .components
        .iter()
        .find(|component| {
            component.id == id && component.target == crate::current_update_platform()
        })
        .cloned()
        .ok_or_else(|| "Неизвестный компонент".to_string())?;
    guard_descriptor(&catalog.payload, &descriptor)?;
    guard_target_matches_platform(&descriptor)?;

    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;

    if descriptor.url.trim().is_empty() {
        return Err("Компонент доступен только в подписанном офлайн-комплекте; выберите «Импортировать офлайн-комплект»".into());
    }
    let validated = crate::validate_update_url(&descriptor.url)?;
    let host = validated.host.trim_end_matches('.').to_ascii_lowercase();
    let allowed_hosts = catalog
        .payload
        .allowed_hosts
        .iter()
        .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if !allowed_hosts.contains(&host) {
        return Err("Домен компонента отсутствует в подписанном allow-list каталога".into());
    }
    let client = crate::pinned_update_client(&validated)?;
    let mut response = client
        .get(validated.url.clone())
        .send()
        .map_err(|error| format!("Ошибка загрузки компонента: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Сервер компонента вернул HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length != descriptor.size_bytes)
    {
        return Err("Content-Length компонента не совпадает с подписанным каталогом".into());
    }

    let temp_archive = root.join(format!(".{}.{}.download-part", id, Uuid::new_v4()));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_archive)
        .map_err(|error| error.to_string())?;
    let transfer = (|| -> Result<String, String> {
        let mut digest = Sha256::new();
        let mut downloaded = 0u64;
        let mut buffer = [0u8; 128 * 1024];
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            downloaded = downloaded.saturating_add(read as u64);
            if downloaded > descriptor.size_bytes || downloaded > MAX_COMPONENT_ARCHIVE_BYTES {
                return Err("Компонент превышает подписанный лимит размера".into());
            }
            digest.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            emit_progress(
                app,
                id,
                "download",
                downloaded,
                descriptor.size_bytes,
                "Загружается компонент",
            );
        }
        output.sync_all().map_err(|error| error.to_string())?;
        if downloaded != descriptor.size_bytes {
            return Err("Фактический размер компонента не совпадает с каталогом".into());
        }
        Ok(hex::encode(digest.finalize()))
    })();
    drop(output);
    let actual_hash = match transfer {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_archive);
            return Err(error);
        }
    };
    if !actual_hash.eq_ignore_ascii_case(&descriptor.sha256) {
        let _ = std::fs::remove_file(&temp_archive);
        return Err("Хеш компонента не совпал — установка отклонена".into());
    }

    emit_progress(
        app,
        id,
        "extract",
        0,
        descriptor.size_bytes,
        "Компонент проверен; выполняется атомарная распаковка",
    );
    let stage_dir = match stage_verified_component_archive(&root, &descriptor, &temp_archive) {
        Ok(value) => value,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_archive);
            return Err(error);
        }
    };
    let _ = std::fs::remove_file(&temp_archive);

    // Network transfer and extraction use unique staging paths and do not mutate
    // authoritative component state. Serialize only the final transaction: a
    // competing newer catalog must be visible to the rollback check before this
    // directory/catalog pair can commit.
    let _transaction_guard = lock_component_transactions()?;
    if let Err(error) = guard_catalog_not_older(&root, &catalog) {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(error);
    }
    if let Err(error) = commit_staged_offline_components(
        &root,
        &catalog,
        vec![(descriptor.clone(), stage_dir.clone())],
    ) {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(error);
    }
    emit_progress(
        app,
        id,
        "complete",
        descriptor.size_bytes,
        descriptor.size_bytes,
        "Компонент установлен и доступен офлайн",
    );
    let sidecars = crate::universal_intake::sidecar_tool_statuses();
    Ok(status_for_descriptor(&root, &descriptor, true, &sidecars))
}

fn recover_component_transactions(
    root: &Path,
    descriptors: Option<&[ComponentDescriptor]>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let root_metadata = std::fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return Err("Пользовательский каталог компонентов имеет небезопасный тип".into());
    }

    if let Some(descriptors) = descriptors {
        for descriptor in descriptors {
            if descriptor.target != crate::current_update_platform() {
                continue;
            }
            let final_dir = root.join(&descriptor.id);
            // The cached signed descriptor defines which side of a component swap
            // committed. If the final directory already matches it, the catalog
            // commit happened (or no swap was interrupted) and `.previous` is only
            // cleanup. If it does not match, a crash may have occurred after the
            // directory rename but before catalog publication; recover the newest
            // fully verified previous installation instead of discarding it later.
            if final_dir.exists()
                && read_verified_component_manifest(&final_dir, descriptor).is_ok()
            {
                continue;
            }
            let mut previous = std::fs::read_dir(root)
                .map_err(|error| error.to_string())?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name().to_str().is_some_and(|name| {
                        transaction_name_matches(name, &descriptor.id, "previous")
                            || transaction_name_matches(
                                name,
                                &descriptor.id,
                                "durability-uncertain",
                            )
                    })
                })
                .filter_map(|entry| {
                    let path = entry.path();
                    let metadata = std::fs::symlink_metadata(&path).ok()?;
                    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                        return None;
                    }
                    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    Some((modified, path))
                })
                .collect::<Vec<_>>();
            previous.sort_by_key(|item| std::cmp::Reverse(item.0));
            let verified_previous = previous.into_iter().find_map(|(_, candidate)| {
                read_verified_component_manifest(&candidate, descriptor)
                    .is_ok()
                    .then_some(candidate)
            });
            let Some(previous_dir) = verified_previous else {
                continue;
            };

            if final_dir.exists() {
                let metadata =
                    std::fs::symlink_metadata(&final_dir).map_err(|error| error.to_string())?;
                if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Небезопасный финальный путь прерванной установки компонента {}",
                        descriptor.id
                    ));
                }
                let interrupted =
                    root.join(format!(".{}.{}.interrupted", descriptor.id, Uuid::new_v4()));
                std::fs::rename(&final_dir, &interrupted).map_err(|error| {
                    format!(
                        "Не удалось изолировать незавершённую версию компонента {}: {error}",
                        descriptor.id
                    )
                })?;
                if let Err(error) = std::fs::rename(&previous_dir, &final_dir) {
                    let _ = std::fs::rename(&interrupted, &final_dir);
                    return Err(format!(
                        "Не удалось восстановить предыдущую версию компонента {}: {error}",
                        descriptor.id
                    ));
                }
                let _ = std::fs::remove_dir_all(interrupted);
            } else {
                std::fs::rename(&previous_dir, &final_dir).map_err(|error| {
                    format!(
                        "Не удалось восстановить предыдущую версию компонента {}: {error}",
                        descriptor.id
                    )
                })?;
            }
        }
    }

    // A post-rename fsync failure leaves the catalog authoritative in the live
    // namespace but not yet proven durable across power loss. Do not age-delete
    // any old component backup until a later sync of every catalog authority
    // directory succeeds. This also protects a plain `.previous` if the best-effort
    // durability marker itself could not be persisted.
    let catalog_durability_confirmed = sync_cached_catalog_authority_directories(root).is_ok();
    let now = SystemTime::now();
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let is_transaction = name.starts_with('.')
            && (name.ends_with(".download-part")
                || name.ends_with(".installing")
                || name.ends_with(".previous")
                || name.ends_with(".durability-uncertain")
                || name.ends_with(".interrupted"));
        if !is_transaction {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now.duration_since(modified).unwrap_or_default();
        if !stale_component_transaction_removal_allowed(name, age, catalog_durability_confirmed) {
            continue;
        }
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "В каталоге компонентов обнаружена небезопасная ссылка: {name}"
            ));
        }
        if metadata.file_type().is_dir() {
            std::fs::remove_dir_all(entry.path()).map_err(|error| error.to_string())?;
        } else if metadata.file_type().is_file() {
            std::fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn transaction_name_matches(name: &str, component_id: &str, suffix: &str) -> bool {
    let prefix = format!(".{component_id}.");
    let suffix = format!(".{suffix}");
    name.starts_with(&prefix) && name.ends_with(&suffix) && name.len() > prefix.len() + suffix.len()
}

fn fetch_and_verify_catalog(app: &tauri::AppHandle) -> Result<SignedComponentsCatalog, String> {
    emit_progress(
        app,
        "catalog",
        "catalog",
        0,
        0,
        "Загружается подписанный каталог компонентов",
    );
    let validated = crate::validate_update_url(TRUSTED_COMPONENTS_CATALOG_URL)?;
    let client = crate::pinned_update_client(&validated)?;
    let bytes = crate::fetch_limited_bytes(&client, &validated, MAX_COMPONENT_CATALOG_BYTES)?;
    let catalog: SignedComponentsCatalog = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Некорректный components catalog: {error}"))?;
    verify_catalog(&catalog)?;
    Ok(catalog)
}

fn verify_catalog(catalog: &SignedComponentsCatalog) -> Result<(), String> {
    if catalog.payload.schema != COMPONENT_CATALOG_SCHEMA {
        return Err("Неподдерживаемая схема components catalog".into());
    }
    verify_signed_payload(
        &catalog.payload,
        &catalog.signature_alg,
        &catalog.signature,
        "components catalog",
    )?;
    match catalog.payload.catalog_scope.as_deref() {
        None | Some("complete") | Some("partial") => {}
        Some(_) => return Err("Некорректный catalog_scope components catalog".into()),
    }
    catalog_published_at(catalog)?;
    let current = crate::parse_semver(env!("CARGO_PKG_VERSION"))?;
    let minimum = crate::parse_semver(&catalog.payload.app_min_version)?;
    if current < minimum {
        return Err(format!(
            "Каталог требует Dokkomplekt {} или новее; установлена {}",
            minimum, current
        ));
    }
    if catalog
        .payload
        .components
        .iter()
        .any(|item| !item.url.trim().is_empty())
        && catalog.payload.allowed_hosts.is_empty()
    {
        return Err("Сетевой подписанный каталог не содержит allow-list доменов".into());
    }
    let mut ids = BTreeSet::new();
    for descriptor in &catalog.payload.components {
        guard_descriptor(&catalog.payload, descriptor)?;
        let identity = format!("{}:{}", descriptor.target, descriptor.id);
        if !ids.insert(identity) {
            return Err(format!(
                "Дублирующийся component id для target {}: {}",
                descriptor.target, descriptor.id
            ));
        }
    }
    Ok(())
}

fn catalog_published_at(catalog: &SignedComponentsCatalog) -> Result<OffsetDateTime, String> {
    OffsetDateTime::parse(
        catalog.payload.published_at.trim(),
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| "Некорректный published_at components catalog".to_string())
}

fn guard_catalog_not_older(root: &Path, incoming: &SignedComponentsCatalog) -> Result<(), String> {
    let incoming_at = catalog_published_at(incoming)?;
    if !catalog_cache_exists(root)? {
        return Ok(());
    }
    let current = read_cached_catalogs(root)?;
    let current_at = current
        .iter()
        .map(catalog_published_at)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or_else(|| "Кэш подписанных каталогов пуст".to_string())?;
    if incoming_at < current_at {
        return Err("Подписанный каталог старее уже принятого; rollback отклонён".into());
    }
    Ok(())
}

fn guard_target_matches_platform(descriptor: &ComponentDescriptor) -> Result<(), String> {
    if descriptor.target != crate::current_update_platform() {
        return Err(format!(
            "Компонент {} предназначен для {}, а не для {}",
            descriptor.id,
            descriptor.target,
            crate::current_update_platform()
        ));
    }
    Ok(())
}

fn verify_signed_payload<T: Serialize>(
    payload: &T,
    signature_alg: &str,
    signature: &str,
    label: &str,
) -> Result<(), String> {
    if !signature_alg.trim().eq_ignore_ascii_case("ed25519") {
        return Err(format!("Неподдерживаемый алгоритм подписи {label}"));
    }
    let key_bytes = BASE64_STANDARD
        .decode(crate::TRUSTED_UPDATE_PUBKEY_B64.trim())
        .map_err(|_| "Некорректный встроенный update public key".to_string())?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Update public key должен содержать 32 байта".to_string())?;
    let key = VerifyingKey::from_bytes(&key_array)
        .map_err(|_| "Некорректный встроенный update public key".to_string())?;
    let signature_bytes = BASE64_STANDARD
        .decode(signature.trim())
        .map_err(|_| format!("Некорректная подпись {label}"))?;
    let signature = Ed25519Signature::from_slice(&signature_bytes)
        .map_err(|_| format!("Некорректная длина подписи {label}"))?;
    let payload_bytes = crate::canonical_json_bytes(
        &serde_json::to_value(payload).map_err(|error| error.to_string())?,
    )?;
    key.verify(&payload_bytes, &signature)
        .map_err(|_| format!("Подпись {label} не прошла проверку"))
}

fn guard_descriptor(
    payload: &ComponentsCatalogPayload,
    descriptor: &ComponentDescriptor,
) -> Result<(), String> {
    validate_component_id(&descriptor.id)?;
    if descriptor.label.trim().is_empty() || descriptor.label.len() > 200 {
        return Err("Некорректная подпись компонента".into());
    }
    if descriptor.target.is_empty()
        || descriptor.target.len() > 80
        || !descriptor
            .target
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err("Некорректный target компонента".into());
    }
    if descriptor.size_bytes == 0 || descriptor.size_bytes > MAX_COMPONENT_ARCHIVE_BYTES {
        return Err("Некорректный размер компонента".into());
    }
    validate_sha256(&descriptor.sha256, "архива компонента")?;
    validate_sha256(
        &descriptor.files_manifest_sha256,
        "component-files manifest",
    )?;
    if descriptor.unlocks.is_empty()
        || descriptor.unlocks.iter().any(|item| {
            item.is_empty()
                || item.len() > 80
                || !item.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        })
    {
        return Err("Некорректный список unlocks компонента".into());
    }
    component_archive_name(descriptor)?;
    // Catalog verification must be deterministic and offline. A descriptor may
    // intentionally omit its URL when it is distributed only inside a signed
    // local bundle. Network catalogs still receive the same HTTPS/allow-list checks.
    if descriptor.url.trim().is_empty() {
        return Ok(());
    }
    let url = reqwest::Url::parse(&descriptor.url)
        .map_err(|_| "Некорректный URL компонента".to_string())?;
    if url.scheme() != "https" {
        return Err("Компоненты разрешено загружать только по HTTPS".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL компонента не должен содержать credentials".into());
    }
    if url.fragment().is_some() {
        return Err("URL компонента не должен содержать fragment".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "В URL компонента отсутствует host".to_string())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if crate::is_forbidden_public_download_host(&host) {
        return Err("Placeholder, local или некорректный host запрещён для компонентов".into());
    }
    let allowed = payload
        .allowed_hosts
        .iter()
        .any(|item| item.trim_end_matches('.').eq_ignore_ascii_case(&host));
    if !allowed {
        return Err(format!(
            "Домен {} отсутствует в подписанном allow-list каталога",
            host
        ));
    }
    Ok(())
}

fn catalog_is_partial(catalog: &SignedComponentsCatalog) -> bool {
    catalog.payload.catalog_scope.as_deref() == Some("partial")
}

fn catalog_cache_exists(root: &Path) -> Result<bool, String> {
    if root.join("components-catalog.json").exists() {
        return Ok(true);
    }
    let overlays = root.join(COMPONENT_CATALOG_OVERLAYS_DIR);
    if !overlays.exists() {
        return Ok(false);
    }
    let metadata = std::fs::symlink_metadata(&overlays).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Каталог partial components catalog имеет небезопасный тип".into());
    }
    for entry in std::fs::read_dir(overlays).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| !name.starts_with('.'))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read_catalog_file(path: &Path, label: &str) -> Result<SignedComponentsCatalog, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{label} имеет небезопасный тип"));
    }
    if metadata.len() == 0 || metadata.len() > MAX_COMPONENT_CATALOG_BYTES {
        return Err(format!("{label} превышает лимит или пуст"));
    }
    let catalog: SignedComponentsCatalog =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Некорректный {label}: {error}"))?;
    verify_catalog(&catalog)?;
    Ok(catalog)
}

fn read_cached_catalogs(root: &Path) -> Result<Vec<SignedComponentsCatalog>, String> {
    let mut catalogs = Vec::new();
    let primary = root.join("components-catalog.json");
    if primary.exists() {
        catalogs.push(read_catalog_file(&primary, "кэш components catalog")?);
    }

    let overlays = root.join(COMPONENT_CATALOG_OVERLAYS_DIR);
    if overlays.exists() {
        let metadata = std::fs::symlink_metadata(&overlays).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("Каталог partial components catalog имеет небезопасный тип".into());
        }
        let mut paths = std::fs::read_dir(&overlays)
            .map_err(|error| error.to_string())?
            .map(|entry| {
                entry
                    .map(|value| value.path())
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        paths.retain(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| !name.starts_with('.'))
        });
        paths.sort();
        if paths.len() > MAX_COMPONENT_CATALOG_OVERLAYS {
            return Err("Слишком много partial components catalog; состояние отклонено".into());
        }
        for path in paths {
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return Err("В каталоге partial components catalog найден посторонний файл".into());
            }
            let catalog = read_catalog_file(&path, "partial components catalog")?;
            if !catalog_is_partial(&catalog) {
                return Err(
                    "Overlay-каталог обязан иметь подписанный catalog_scope=partial".into(),
                );
            }
            catalogs.push(catalog);
        }
    }
    if catalogs.is_empty() {
        return Err("Кэш подписанного каталога компонентов отсутствует".into());
    }
    Ok(catalogs)
}

fn effective_component_descriptors_from_catalogs(
    catalogs: &[SignedComponentsCatalog],
) -> Result<Vec<ComponentDescriptor>, String> {
    let mut ordered = catalogs
        .iter()
        .map(|catalog| {
            Ok((
                catalog_published_at(catalog)?,
                if catalog_is_partial(catalog) {
                    0u8
                } else {
                    1u8
                },
                catalog.signature.as_str(),
                catalog,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    ordered.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(right.2))
    });

    let mut effective = BTreeMap::<String, ComponentDescriptor>::new();
    for (_, _, _, catalog) in ordered {
        if !catalog_is_partial(catalog) {
            effective.clear();
        }
        for descriptor in &catalog.payload.components {
            effective.insert(
                format!("{}:{}", descriptor.target, descriptor.id),
                descriptor.clone(),
            );
        }
    }
    Ok(effective.into_values().collect())
}

fn read_effective_component_descriptors(root: &Path) -> Result<Vec<ComponentDescriptor>, String> {
    let catalogs = read_cached_catalogs(root)?;
    effective_component_descriptors_from_catalogs(&catalogs)
}

fn ensure_catalog_overlays_dir(root: &Path) -> Result<PathBuf, String> {
    let overlays = root.join(COMPONENT_CATALOG_OVERLAYS_DIR);
    if overlays.exists() {
        let metadata = std::fs::symlink_metadata(&overlays).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err("Каталог partial components catalog имеет небезопасный тип".into());
        }
    } else {
        std::fs::create_dir(&overlays).map_err(|error| error.to_string())?;
    }
    Ok(overlays)
}

fn clear_catalog_overlays(root: &Path) -> Result<(), String> {
    let overlays = root.join(COMPONENT_CATALOG_OVERLAYS_DIR);
    if !overlays.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(&overlays).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Каталог partial components catalog имеет небезопасный тип".into());
    }
    std::fs::remove_dir_all(overlays).map_err(|error| error.to_string())
}

fn persist_verified_catalog(
    root: &Path,
    catalog: &SignedComponentsCatalog,
) -> Result<(), AtomicWriteError> {
    verify_catalog(catalog).map_err(AtomicWriteError::BeforeCommit)?;
    if catalog_is_partial(catalog) {
        let overlays = ensure_catalog_overlays_dir(root).map_err(AtomicWriteError::BeforeCommit)?;
        let fingerprint = sha256_bytes(catalog.signature.as_bytes());
        let path = overlays.join(format!("{fingerprint}.json"));
        if !path.exists() {
            let mut count = 0usize;
            for entry in std::fs::read_dir(&overlays)
                .map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?
            {
                let entry =
                    entry.map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| !name.starts_with('.'))
                {
                    count += 1;
                }
            }
            if count >= MAX_COMPONENT_CATALOG_OVERLAYS {
                return Err(AtomicWriteError::BeforeCommit(
                    "Достигнут лимит partial components catalog".into(),
                ));
            }
        }
        // Component swaps and the overlay directory live in `root`, while the
        // signed partial authority is published one directory below it. Flush the
        // root *before* publishing that authority so a surviving overlay can never
        // describe component directory renames that were still only in cache.
        sync_directory(root).map_err(AtomicWriteError::BeforeCommit)?;
        atomic_write_json_with_commit_state(&path, catalog)
    } else {
        // The signed complete catalog is the authority commit point. Older/equal
        // partial overlays cannot override it: effective ordering applies partial
        // catalogs before a complete catalog at the same timestamp, and production
        // callers reject catalog rollback before persistence. Cleanup is therefore
        // post-commit hygiene, not part of the transaction. A post-rename durability
        // failure is reported as `AfterCommit`; callers must keep the new component
        // directories and their `.previous` recovery copies instead of rolling back
        // under a catalog that is already authoritative in the live namespace.
        atomic_write_json_with_commit_state(&root.join("components-catalog.json"), catalog)?;
        let _ = clear_catalog_overlays(root);
        Ok(())
    }
}

fn validate_installed_component(
    component_dir: &Path,
    descriptor: &ComponentDescriptor,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(component_dir).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Папка компонента имеет небезопасный тип".into());
    }
    let status_path = component_dir.join("component-status.json");
    let status_metadata =
        std::fs::symlink_metadata(&status_path).map_err(|error| error.to_string())?;
    if !status_metadata.file_type().is_file() || status_metadata.file_type().is_symlink() {
        return Err("component-status.json имеет небезопасный тип".into());
    }
    if status_metadata.len() > 64 * 1024 {
        return Err("component-status.json превышает лимит".into());
    }
    let receipt: InstalledComponentReceipt =
        serde_json::from_slice(&std::fs::read(&status_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Некорректный component-status.json: {error}"))?;
    if receipt.schema != COMPONENT_STATUS_SCHEMA
        || receipt.component_id != descriptor.id
        || receipt.target != descriptor.target
        || !receipt
            .archive_sha256
            .eq_ignore_ascii_case(&descriptor.sha256)
        || !receipt
            .files_manifest_sha256
            .eq_ignore_ascii_case(&descriptor.files_manifest_sha256)
    {
        return Err("Локальный статус компонента не совпадает с подписанным каталогом".into());
    }
    Ok(())
}

fn read_component_files_manifest(
    component_dir: &Path,
    descriptor: &ComponentDescriptor,
) -> Result<ComponentFilesManifest, String> {
    let path = component_dir.join("component-files.json");
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("component-files.json имеет небезопасный тип".into());
    }
    if metadata.len() > 4 * 1024 * 1024 {
        return Err("component-files.json превышает лимит".into());
    }
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let actual = sha256_bytes(&bytes);
    if !actual.eq_ignore_ascii_case(&descriptor.files_manifest_sha256) {
        return Err("Хеш component-files.json не совпадает с подписанным каталогом".into());
    }
    let manifest: ComponentFilesManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Некорректный component-files.json: {error}"))?;
    if manifest.schema != COMPONENT_FILES_SCHEMA
        || manifest.component_id != descriptor.id
        || manifest.target != descriptor.target
        || manifest.files.is_empty()
    {
        return Err("component-files.json не соответствует компоненту".into());
    }
    for (path, hash) in &manifest.files {
        safe_relative_path(path)?;
        validate_sha256(hash, "файла компонента")?;
    }
    Ok(manifest)
}

fn validate_all_manifest_files(
    component_dir: &Path,
    manifest: &ComponentFilesManifest,
) -> Result<(), String> {
    for (relative, expected) in &manifest.files {
        let path = component_dir.join(safe_relative_path(relative)?);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("Компонент содержит небезопасный файл: {relative}"));
        }
        let actual = sha256_file(&path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!("Хеш файла компонента не совпадает: {relative}"));
        }
    }

    // The signed manifest is also an allow-list. Reject injected DLLs, executables
    // or any other unexpected filesystem object instead of merely checking that
    // the expected files still exist. The two control JSON files are separately
    // bound to the signed descriptor/receipt and are intentionally outside the
    // payload file map.
    fn walk(root: &Path, current: &Path, allowed: &BTreeMap<String, String>) -> Result<(), String> {
        for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err(
                    "Символические ссылки внутри установленного компонента запрещены".into(),
                );
            }
            if metadata.file_type().is_dir() {
                walk(root, &path, allowed)?;
                continue;
            }
            if !metadata.file_type().is_file() {
                return Err("Компонент содержит неподдерживаемый объект файловой системы".into());
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Файл компонента вышел за пределы корня".to_string())?;
            let key = path_key(relative)
                .ok_or_else(|| "Некорректный путь файла компонента".to_string())?;
            if matches!(
                key.as_str(),
                "component-files.json" | "component-status.json"
            ) {
                continue;
            }
            if !allowed.contains_key(&key) {
                return Err(format!("Компонент содержит неподписанный файл: {key}"));
            }
        }
        Ok(())
    }

    walk(component_dir, component_dir, &manifest.files)
}

fn read_verified_component_manifest(
    component_dir: &Path,
    descriptor: &ComponentDescriptor,
) -> Result<ComponentFilesManifest, String> {
    validate_installed_component(component_dir, descriptor)?;
    let manifest = read_component_files_manifest(component_dir, descriptor)?;
    // Executables are never trusted in isolation. DLLs, models and every other
    // manifest-bound companion must still match before any path is returned to a
    // process launcher, otherwise a valid executable can load a tampered DLL.
    validate_all_manifest_files(component_dir, &manifest)?;
    Ok(manifest)
}

fn statuses_from_descriptors(
    root: &Path,
    descriptors: &[ComponentDescriptor],
) -> Vec<ComponentStatus> {
    let sidecars = crate::universal_intake::sidecar_tool_statuses();
    statuses_from_descriptors_with_sidecars(root, descriptors, &sidecars)
}

fn statuses_from_descriptors_with_sidecars(
    root: &Path,
    descriptors: &[ComponentDescriptor],
    sidecars: &[crate::universal_intake::SidecarToolStatus],
) -> Vec<ComponentStatus> {
    descriptors
        .iter()
        .filter(|component| component.target == crate::current_update_platform())
        .map(|component| status_for_descriptor(root, component, true, sidecars))
        .collect()
}

fn status_for_descriptor(
    root: &Path,
    descriptor: &ComponentDescriptor,
    catalog_available: bool,
    sidecars: &[crate::universal_intake::SidecarToolStatus],
) -> ComponentStatus {
    let component_dir = root.join(&descriptor.id);
    let valid = read_verified_component_manifest(&component_dir, descriptor);
    let downloaded = valid.is_ok();
    let relevant = descriptor
        .unlocks
        .iter()
        .filter_map(|tool| sidecars.iter().find(|status| status.tool == *tool))
        .collect::<Vec<_>>();
    let externally_available = relevant.len() == descriptor.unlocks.len()
        && relevant.iter().all(|status| status.available);
    let external_state =
        if externally_available && relevant.iter().all(|status| status.state == "bundled") {
            Some("bundled")
        } else if externally_available {
            Some("system")
        } else {
            None
        };
    let state = if downloaded {
        "downloaded"
    } else {
        external_state.unwrap_or("missing")
    };
    let available = downloaded || externally_available;
    ComponentStatus {
        id: descriptor.id.clone(),
        label: descriptor.label.clone(),
        description: descriptor.description.clone(),
        target: descriptor.target.clone(),
        size_bytes: descriptor.size_bytes,
        size_label: human_size(descriptor.size_bytes),
        unlocks: descriptor.unlocks.clone(),
        state: state.into(),
        installed: downloaded,
        available,
        catalog_available,
        message: if downloaded {
            "Установлен в пользовательской папке; SHA-256 каждого исполняемого файла подтверждён подписанным каталогом.".into()
        } else if state == "bundled" {
            "Все инструменты компонента уже встроены в установщик и доступны.".into()
        } else if state == "system" {
            "Все инструменты компонента найдены в системе; отдельная загрузка не требуется.".into()
        } else if component_dir.exists() {
            format!(
                "Компонент присутствует, но не прошёл проверку: {}",
                valid.err().unwrap_or_else(|| "неизвестная ошибка".into())
            )
        } else {
            format!(
                "Разовая загрузка {}. После установки работает офлайн.",
                human_size(descriptor.size_bytes)
            )
        },
    }
}

fn fallback_statuses(message: &str) -> Vec<ComponentStatus> {
    [
        (
            "ocr",
            "Распознавание сканов (OCR)",
            vec!["tesseract", "pdftotext", "pdftoppm"],
        ),
        (
            "office",
            "Конвертация и печать",
            vec!["soffice", "sumatrapdf"],
        ),
        (
            "semantic",
            "Локальная модель понимания текста",
            vec!["llama_cpp", "semantic_model"],
        ),
        ("archive", "Распаковка входящих архивов", vec!["7z"]),
    ]
    .into_iter()
    .map(|(id, label, unlocks)| ComponentStatus {
        id: id.into(),
        label: label.into(),
        description: String::new(),
        target: crate::current_update_platform().into(),
        size_bytes: 0,
        size_label: "размер появится после проверки каталога".into(),
        unlocks: unlocks.into_iter().map(str::to_string).collect(),
        state: "missing".into(),
        installed: false,
        available: false,
        catalog_available: false,
        message: message.into(),
    })
    .collect()
}

fn sanitized_component_file_mode(unix_mode: Option<u32>) -> u32 {
    // Component archives are signed, but permission metadata still must not be
    // allowed to introduce setuid/setgid/sticky or broader write permissions.
    // Official packs encode regular data as 0644 and executables as 0755; retain
    // only the execute intent and use the application's safe read/write baseline.
    0o644 | unix_mode.unwrap_or(0) & 0o111
}

fn apply_sanitized_component_file_mode(path: &Path, unix_mode: Option<u32>) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(sanitized_component_file_mode(unix_mode));
        std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, unix_mode);
    }
    Ok(())
}

fn sync_extracted_directory_tree(root: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut pending = vec![root.to_path_buf()];
        let mut directories = Vec::<PathBuf>::new();
        while let Some(directory) = pending.pop() {
            let metadata =
                std::fs::symlink_metadata(&directory).map_err(|error| error.to_string())?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err("Папка распакованного компонента имеет небезопасный тип".into());
            }
            directories.push(directory.clone());
            for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let metadata =
                    std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
                if metadata.file_type().is_symlink() {
                    return Err("Символические ссылки в распакованном компоненте запрещены".into());
                }
                if metadata.file_type().is_dir() {
                    pending.push(entry.path());
                }
            }
        }
        // A file fsync does not persist the directory entry that names it. Flush
        // deepest directories first so every nested file/child directory name is
        // durable before its parent, the staging root, and finally the component
        // root are published by the transaction/catalog commit.
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        for directory in directories {
            sync_directory(&directory)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = root;
    }
    Ok(())
}

fn safe_extract_zip(archive_path: &Path, stage_dir: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    if archive.is_empty() || archive.len() > MAX_COMPONENT_ENTRIES {
        return Err("Компонент содержит недопустимое число файлов".into());
    }
    let mut unpacked = 0u64;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let unix_mode = entry.unix_mode();
        let name = entry.name().replace('\\', "/");
        let relative = safe_relative_path(&name)?;
        let key =
            path_key(&relative).ok_or_else(|| "Некорректный путь в компоненте".to_string())?;
        if !seen.insert(key.clone()) {
            return Err(format!("Дублирующийся путь в компоненте: {key}"));
        }
        if entry.is_dir() {
            std::fs::create_dir_all(stage_dir.join(relative)).map_err(|error| error.to_string())?;
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Символические ссылки в компоненте запрещены".into());
        }
        unpacked = unpacked.saturating_add(entry.size());
        if unpacked > MAX_COMPONENT_UNPACKED_BYTES {
            return Err("Распакованный компонент превышает лимит".into());
        }
        let destination = stage_dir.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        apply_sanitized_component_file_mode(&destination, unix_mode)?;
        // Persist file contents and the sanitized executable metadata before the
        // containing directory entry is made durable below.
        output.sync_all().map_err(|error| error.to_string())?;
    }
    sync_extracted_directory_tree(stage_dir)?;
    Ok(())
}

fn resolve_component_tool_candidate(
    component_dir: &Path,
    manifest: &ComponentFilesManifest,
    candidates: Vec<PathBuf>,
) -> Option<PathBuf> {
    for candidate in candidates {
        let Ok(relative) = candidate.strip_prefix(component_dir) else {
            continue;
        };
        let Some(relative_key) = path_key(relative) else {
            continue;
        };
        let Some(expected) = manifest.files.get(&relative_key) else {
            continue;
        };
        if candidate.is_file()
            && sha256_file(&candidate)
                .ok()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        {
            return Some(candidate);
        }
    }
    None
}

fn append_component_tool_candidates(
    candidates: &mut Vec<PathBuf>,
    root: &Path,
    program: &str,
    executable_name: &str,
) {
    candidates.extend([
        root.join(executable_name),
        root.join(program).join(executable_name),
        root.join("bin").join(executable_name),
    ]);
    match program {
        "tesseract" => candidates.push(root.join("tesseract").join(executable_name)),
        "pdftotext" | "pdftoppm" => {
            candidates.push(root.join("poppler").join("bin").join(executable_name))
        }
        "soffice" => candidates.push(
            root.join("libreoffice")
                .join("program")
                .join(executable_name),
        ),
        "7z" => candidates.push(root.join("7zip").join(executable_name)),
        "sumatrapdf" => candidates.push(root.join("sumatrapdf").join("SumatraPDF.exe")),
        "llama_cpp" => {
            candidates.push(root.join("llama_cpp").join(if cfg!(windows) {
                "llama-server.exe"
            } else {
                "llama-server"
            }));
            candidates.push(root.join("llama.cpp").join(if cfg!(windows) {
                "llama-server.exe"
            } else {
                "llama-server"
            }));
        }
        "semantic_model" => candidates.push(
            root.join("semantic_model")
                .join("dokkomplekt-instruct.gguf"),
        ),
        _ => {}
    }
}

fn validate_component_id(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err("Некорректный идентификатор компонента".into());
    }
    Ok(value)
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty()
        || value.len() > 1024
        || value.contains('\0')
        || value.contains('\\')
        || value.contains(':')
    {
        return Err("Некорректный путь в компоненте".into());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err("Абсолютные пути в компоненте запрещены".into());
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err("Path traversal в компоненте запрещён".into()),
        }
    }
    Ok(path.to_path_buf())
}

fn path_key(path: &Path) -> Option<String> {
    let value = path.to_string_lossy().replace('\\', "/");
    (!value.is_empty()).then_some(value)
}

fn validate_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(format!("Некорректный SHA-256 {label}"));
    }
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn replace_file_atomically(temporary: &Path, destination: &Path) -> Result<(), AtomicWriteError> {
    if destination.exists() {
        let metadata = std::fs::symlink_metadata(destination)
            .map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(AtomicWriteError::BeforeCommit(
                "Целевой JSON имеет небезопасный тип".into(),
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source = temporary
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
            return Err(AtomicWriteError::BeforeCommit(format!(
                "Не удалось атомарно заменить {}: {}",
                destination.display(),
                std::io::Error::last_os_error()
            )));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::rename(temporary, destination).map_err(|error| {
            AtomicWriteError::BeforeCommit(format!(
                "Не удалось атомарно заменить {}: {error}",
                destination.display()
            ))
        })?;
        // From this instruction onward the destination is authoritative in the
        // live namespace. A directory-sync failure is therefore *after commit*:
        // callers may report durability uncertainty, but must never restore old
        // components underneath the already replaced catalog.
        #[cfg(unix)]
        {
            let parent = destination.parent().ok_or_else(|| {
                AtomicWriteError::AfterCommit("Некорректный путь JSON".to_string())
            })?;
            sync_directory(parent).map_err(|error| {
                AtomicWriteError::AfterCommit(format!(
                    "{error}. Каталог уже опубликован; откат компонентов запрещён, резервная предыдущая версия сохранена."
                ))
            })?;
        }
    }
    Ok(())
}

fn atomic_write_json_with_commit_state<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), AtomicWriteError> {
    let parent = path
        .parent()
        .ok_or_else(|| AtomicWriteError::BeforeCommit("Некорректный путь JSON".to_string()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("component"),
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| AtomicWriteError::BeforeCommit(error.to_string()))?;
    let write_result = (|| -> Result<(), String> {
        output
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        output.write_all(b"\n").map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    })();
    drop(output);
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temporary);
        return Err(AtomicWriteError::BeforeCommit(error));
    }
    match replace_file_atomically(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            if !error.authority_committed() {
                let _ = std::fs::remove_file(&temporary);
            }
            Err(error)
        }
    }
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    atomic_write_json_with_commit_state(path, value).map_err(AtomicWriteError::into_message)
}

fn emit_progress(
    app: &tauri::AppHandle,
    id: &str,
    phase: &str,
    downloaded_bytes: u64,
    total_bytes: u64,
    message: &str,
) {
    let percent = downloaded_bytes
        .saturating_mul(100)
        .checked_div(total_bytes)
        .unwrap_or(0)
        .min(100) as u8;
    let _ = app.emit(
        "component://progress",
        ComponentProgress {
            id: id.into(),
            phase: phase.into(),
            downloaded_bytes,
            total_bytes,
            percent,
            message: message.into(),
        },
    );
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} ГБ", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} МБ", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} Б")
    }
}

#[cfg(test)]
fn crate_platform_for_test() -> String {
    crate::current_update_platform().to_string()
}

#[cfg(test)]
mod tests {
    use super::{safe_relative_path, validate_component_id};

    #[test]
    fn component_paths_reject_traversal_and_absolute_paths() {
        assert!(safe_relative_path("bin/tool.exe").is_ok());
        assert!(safe_relative_path("../tool.exe").is_err());
        assert!(safe_relative_path("/tmp/tool").is_err());
        assert!(safe_relative_path("C:\\tool.exe").is_err());
    }

    #[test]
    fn component_ids_are_conservative() {
        assert_eq!(
            validate_component_id("semantic-model").unwrap(),
            "semantic-model"
        );
        assert!(validate_component_id("../semantic").is_err());
        assert!(validate_component_id("Semantic").is_err());
    }

    #[test]
    fn descriptor_rejects_wrong_target_and_non_https_url() {
        use super::{
            guard_descriptor, guard_target_matches_platform, ComponentDescriptor,
            ComponentsCatalogPayload,
        };
        let mut descriptor = ComponentDescriptor {
            id: "ocr".into(),
            label: "OCR".into(),
            description: String::new(),
            unlocks: vec!["tesseract".into()],
            target: super::crate_platform_for_test(),
            size_bytes: 1024,
            sha256: "a".repeat(64),
            files_manifest_sha256: "b".repeat(64),
            archive_name: "ocr.zip".into(),
            url: "https://downloads.dokkomplekt.ru/ocr.zip".into(),
        };
        let payload = ComponentsCatalogPayload {
            schema: 1,
            app_min_version: env!("CARGO_PKG_VERSION").into(),
            published_at: "2026-07-20T00:00:00Z".into(),
            catalog_scope: None,
            allowed_hosts: vec!["downloads.dokkomplekt.ru".into()],
            components: vec![descriptor.clone()],
        };
        assert!(guard_descriptor(&payload, &descriptor).is_ok());
        assert!(guard_target_matches_platform(&descriptor).is_ok());
        descriptor.url = "https://downloads.example.com/ocr.zip".into();
        assert!(guard_descriptor(&payload, &descriptor).is_err());
        descriptor.url = "https://downloads.dokkomplekt.ru/ocr.zip".into();
        descriptor.url = "http://downloads.dokkomplekt.ru/ocr.zip".into();
        assert!(guard_descriptor(&payload, &descriptor).is_err());
        descriptor.url = "https://downloads.dokkomplekt.ru/ocr.zip".into();
        descriptor.target = "other-platform".into();
        assert!(guard_descriptor(&payload, &descriptor).is_ok());
        assert!(guard_target_matches_platform(&descriptor).is_err());

        descriptor.target = super::crate_platform_for_test();
        descriptor.url.clear();
        let mut offline_payload = payload.clone();
        offline_payload.allowed_hosts.clear();
        assert!(guard_descriptor(&offline_payload, &descriptor).is_ok());
        descriptor.archive_name = "../ocr.zip".into();
        assert!(guard_descriptor(&offline_payload, &descriptor).is_err());
        descriptor.archive_name = "other.zip".into();
        descriptor.url = "https://downloads.dokkomplekt.ru/ocr.zip".into();
        assert!(guard_descriptor(&payload, &descriptor).is_err());
    }

    #[test]
    fn installed_component_manifest_detects_tampering() {
        use super::{
            read_component_files_manifest, read_verified_component_manifest,
            validate_all_manifest_files, validate_installed_component, ComponentDescriptor,
            ComponentFilesManifest, InstalledComponentReceipt, COMPONENT_FILES_SCHEMA,
            COMPONENT_STATUS_SCHEMA,
        };
        use sha2::{Digest as _, Sha256};
        use std::collections::BTreeMap;
        use uuid::Uuid;

        let root =
            std::env::temp_dir().join(format!("dokkomplekt-component-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let tool = root.join("bin").join("tool");
        let companion = root.join("bin").join("helper.dll");
        std::fs::write(&tool, b"trusted").unwrap();
        std::fs::write(&companion, b"trusted-companion").unwrap();
        let tool_hash = hex::encode(Sha256::digest(b"trusted"));
        let companion_hash = hex::encode(Sha256::digest(b"trusted-companion"));
        let manifest = ComponentFilesManifest {
            schema: COMPONENT_FILES_SCHEMA,
            component_id: "ocr".into(),
            target: super::crate_platform_for_test(),
            files: BTreeMap::from([
                ("bin/tool".into(), tool_hash),
                ("bin/helper.dll".into(), companion_hash),
            ]),
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(root.join("component-files.json"), &manifest_bytes).unwrap();
        let manifest_hash = hex::encode(Sha256::digest(&manifest_bytes));
        let descriptor = ComponentDescriptor {
            id: "ocr".into(),
            label: "OCR".into(),
            description: String::new(),
            unlocks: vec!["tesseract".into()],
            target: super::crate_platform_for_test(),
            size_bytes: 1,
            sha256: "a".repeat(64),
            files_manifest_sha256: manifest_hash.clone(),
            archive_name: "ocr.zip".into(),
            url: "https://example.com/ocr.zip".into(),
        };
        let receipt = InstalledComponentReceipt {
            schema: COMPONENT_STATUS_SCHEMA,
            component_id: "ocr".into(),
            target: descriptor.target.clone(),
            archive_sha256: descriptor.sha256.clone(),
            files_manifest_sha256: manifest_hash,
            installed_at: "2026-07-20T00:00:00Z".into(),
        };
        std::fs::write(
            root.join("component-status.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        validate_installed_component(&root, &descriptor).unwrap();
        let parsed = read_component_files_manifest(&root, &descriptor).unwrap();
        validate_all_manifest_files(&root, &parsed).unwrap();
        read_verified_component_manifest(&root, &descriptor).unwrap();
        std::fs::write(&companion, b"tampered-companion").unwrap();
        assert!(read_verified_component_manifest(&root, &descriptor).is_err());
        std::fs::write(&companion, b"trusted-companion").unwrap();
        let injected = root.join("bin").join("injected.dll");
        std::fs::write(&injected, b"not-in-signed-manifest").unwrap();
        assert!(read_verified_component_manifest(&root, &descriptor).is_err());
        std::fs::remove_file(injected).unwrap();
        std::fs::write(&tool, b"tampered").unwrap();
        assert!(validate_all_manifest_files(&root, &parsed).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_component_upgrade_restores_verified_previous_over_uncommitted_final() {
        use super::{
            recover_component_transactions, ComponentDescriptor, ComponentFilesManifest,
            ComponentsCatalogPayload, InstalledComponentReceipt, SignedComponentsCatalog,
            COMPONENT_FILES_SCHEMA, COMPONENT_STATUS_SCHEMA,
        };
        use sha2::{Digest as _, Sha256};
        use std::collections::BTreeMap;
        use uuid::Uuid;

        let root =
            std::env::temp_dir().join(format!("dokkomplekt-component-recovery-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let previous = root.join(format!(".ocr.{}.previous", Uuid::new_v4()));
        std::fs::create_dir_all(previous.join("bin")).unwrap();
        let tool = previous.join("bin").join("tool");
        std::fs::write(&tool, b"trusted").unwrap();
        let tool_hash = hex::encode(Sha256::digest(b"trusted"));
        let manifest = ComponentFilesManifest {
            schema: COMPONENT_FILES_SCHEMA,
            component_id: "ocr".into(),
            target: super::crate_platform_for_test(),
            files: BTreeMap::from([("bin/tool".into(), tool_hash)]),
        };
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(previous.join("component-files.json"), &manifest_bytes).unwrap();
        let manifest_hash = hex::encode(Sha256::digest(&manifest_bytes));
        let descriptor = ComponentDescriptor {
            id: "ocr".into(),
            label: "OCR".into(),
            description: String::new(),
            unlocks: vec!["tesseract".into()],
            target: super::crate_platform_for_test(),
            size_bytes: 1,
            sha256: "a".repeat(64),
            files_manifest_sha256: manifest_hash.clone(),
            archive_name: "ocr.zip".into(),
            url: "https://example.com/ocr.zip".into(),
        };
        let receipt = InstalledComponentReceipt {
            schema: COMPONENT_STATUS_SCHEMA,
            component_id: "ocr".into(),
            target: descriptor.target.clone(),
            archive_sha256: descriptor.sha256.clone(),
            files_manifest_sha256: manifest_hash,
            installed_at: "2026-07-20T00:00:00Z".into(),
        };
        std::fs::write(
            previous.join("component-status.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();

        // Simulate a crash after the new staged directory was renamed into place
        // but before the new signed catalog became authoritative. The cached
        // descriptor still describes `previous`, so recovery must not accept the
        // mere existence of this uncommitted final directory.
        let uncommitted_final = root.join("ocr");
        std::fs::create_dir_all(uncommitted_final.join("bin")).unwrap();
        std::fs::write(uncommitted_final.join("bin/tool"), b"new-uncommitted").unwrap();

        let catalog = SignedComponentsCatalog {
            payload: ComponentsCatalogPayload {
                schema: 1,
                app_min_version: env!("CARGO_PKG_VERSION").into(),
                published_at: "2026-07-20T00:00:00Z".into(),
                catalog_scope: None,
                allowed_hosts: vec!["example.com".into()],
                components: vec![descriptor],
            },
            signature_alg: "Ed25519".into(),
            signature: String::new(),
        };
        recover_component_transactions(&root, Some(&catalog.payload.components)).unwrap();
        assert_eq!(
            std::fs::read(root.join("ocr/bin/tool")).unwrap(),
            b"trusted"
        );
        assert!(!previous.exists());
        assert!(!std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".interrupted"))
            }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolver_skips_missing_candidates_before_packaged_7zip_path() {
        use super::{
            resolve_component_tool_candidate, ComponentFilesManifest, COMPONENT_FILES_SCHEMA,
        };
        use sha2::{Digest as _, Sha256};
        use std::collections::BTreeMap;
        use uuid::Uuid;

        let root =
            std::env::temp_dir().join(format!("dokkomplekt-7zip-resolver-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("7zip")).unwrap();
        let tool = root
            .join("7zip")
            .join(if cfg!(windows) { "7z.exe" } else { "7z" });
        std::fs::write(&tool, b"trusted-7zip").unwrap();
        let hash = hex::encode(Sha256::digest(b"trusted-7zip"));
        let key = format!("7zip/{}", tool.file_name().unwrap().to_string_lossy());
        let manifest = ComponentFilesManifest {
            schema: COMPONENT_FILES_SCHEMA,
            component_id: "archive".into(),
            target: super::crate_platform_for_test(),
            files: BTreeMap::from([(key, hash)]),
        };
        let candidates = vec![
            root.join(tool.file_name().unwrap()),
            root.join("7z").join(tool.file_name().unwrap()),
            root.join("bin").join(tool.file_name().unwrap()),
            tool.clone(),
        ];
        assert_eq!(
            resolve_component_tool_candidate(&root, &manifest, candidates),
            Some(tool)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn partial_catalog_overlays_preserve_omitted_components_until_next_complete_catalog() {
        use super::{
            effective_component_descriptors_from_catalogs, ComponentDescriptor,
            ComponentsCatalogPayload, SignedComponentsCatalog,
        };

        fn descriptor(id: &str) -> ComponentDescriptor {
            ComponentDescriptor {
                id: id.into(),
                label: id.into(),
                description: String::new(),
                unlocks: vec![id.into()],
                target: super::crate_platform_for_test(),
                size_bytes: 1,
                sha256: "a".repeat(64),
                files_manifest_sha256: "b".repeat(64),
                archive_name: format!("{id}.zip"),
                url: String::new(),
            }
        }
        fn catalog(
            scope: &str,
            published_at: &str,
            components: Vec<ComponentDescriptor>,
        ) -> SignedComponentsCatalog {
            SignedComponentsCatalog {
                payload: ComponentsCatalogPayload {
                    schema: 1,
                    app_min_version: env!("CARGO_PKG_VERSION").into(),
                    published_at: published_at.into(),
                    catalog_scope: Some(scope.into()),
                    allowed_hosts: vec![],
                    components,
                },
                signature_alg: "Ed25519".into(),
                signature: format!("{scope}-{published_at}"),
            }
        }

        let complete = catalog(
            "complete",
            "2026-07-20T00:00:00Z",
            vec![descriptor("ocr"), descriptor("office")],
        );
        let partial = catalog(
            "partial",
            "2026-07-21T00:00:00Z",
            vec![descriptor("archive")],
        );
        let effective =
            effective_component_descriptors_from_catalogs(&[complete.clone(), partial.clone()])
                .unwrap();
        let ids = effective
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["archive", "ocr", "office"]);

        let replacement = catalog(
            "complete",
            "2026-07-22T00:00:00Z",
            vec![descriptor("office")],
        );
        let effective =
            effective_component_descriptors_from_catalogs(&[complete, partial, replacement])
                .unwrap();
        let ids = effective
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["office"]);

        // Physical deletion of an old overlay is only cleanup. Even if it is
        // temporarily locked on Windows and remains on disk, an equal-timestamp
        // complete catalog is ordered after partial catalogs and is authoritative.
        let stale_overlay = catalog(
            "partial",
            "2026-07-22T00:00:00Z",
            vec![descriptor("archive")],
        );
        let same_time_complete = catalog(
            "complete",
            "2026-07-22T00:00:00Z",
            vec![descriptor("office")],
        );
        let effective =
            effective_component_descriptors_from_catalogs(&[stale_overlay, same_time_complete])
                .unwrap();
        let ids = effective
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["office"]);
    }

    #[test]
    fn empty_complete_catalog_is_a_valid_full_revocation_but_empty_partial_is_not() {
        use super::{
            validate_offline_catalog_component_set, ComponentsCatalogPayload,
            SignedComponentsCatalog,
        };

        fn catalog(scope: &str) -> SignedComponentsCatalog {
            SignedComponentsCatalog {
                payload: ComponentsCatalogPayload {
                    schema: 1,
                    app_min_version: env!("CARGO_PKG_VERSION").into(),
                    published_at: "2026-09-02T00:00:00Z".into(),
                    catalog_scope: Some(scope.into()),
                    allowed_hosts: vec![],
                    components: vec![],
                },
                signature_alg: "Ed25519".into(),
                signature: "test-signature".into(),
            }
        }

        let target = super::crate_platform_for_test();
        assert!(validate_offline_catalog_component_set(&catalog("complete"), &target).is_ok());
        assert!(validate_offline_catalog_component_set(&catalog("partial"), &target).is_err());
    }

    #[test]
    fn atomic_catalog_error_distinguishes_precommit_from_postcommit_failure() {
        use super::AtomicWriteError;

        let before = AtomicWriteError::BeforeCommit("before".into());
        let after = AtomicWriteError::AfterCommit("after".into());
        assert!(!before.authority_committed());
        assert!(after.authority_committed());
    }

    #[test]
    fn component_zip_modes_strip_privilege_bits_but_keep_execute_intent() {
        use super::sanitized_component_file_mode;

        assert_eq!(sanitized_component_file_mode(Some(0o644)), 0o644);
        assert_eq!(sanitized_component_file_mode(Some(0o755)), 0o755);
        assert_eq!(sanitized_component_file_mode(Some(0o4777)), 0o755);
        assert_eq!(sanitized_component_file_mode(None), 0o644);
    }

    #[cfg(unix)]
    #[test]
    fn safe_extract_restores_executable_mode_for_nested_component_tool() {
        use super::safe_extract_zip;
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;
        use uuid::Uuid;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-component-exec-mode-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("component.zip");
        let archive_file = std::fs::File::create(&archive_path).unwrap();
        let mut writer = ZipWriter::new(archive_file);
        writer
            .start_file(
                "nested/bin/tool",
                SimpleFileOptions::default().unix_permissions(0o755),
            )
            .unwrap();
        writer.write_all(b"trusted-tool").unwrap();
        writer.finish().unwrap();

        let stage = root.join("stage");
        std::fs::create_dir(&stage).unwrap();
        safe_extract_zip(&archive_path, &stage).unwrap();
        let tool = stage.join("nested/bin/tool");
        let mode = std::fs::metadata(&tool).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
        assert_eq!(std::fs::read(&tool).unwrap(), b"trusted-tool");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_previous_cleanup_requires_confirmed_catalog_durability() {
        use super::{stale_component_transaction_removal_allowed, STALE_COMPONENT_TRANSACTION_AGE};
        use std::time::Duration;

        let stale = STALE_COMPONENT_TRANSACTION_AGE + Duration::from_secs(1);
        assert!(!stale_component_transaction_removal_allowed(
            ".ocr.example.previous",
            stale,
            false,
        ));
        assert!(!stale_component_transaction_removal_allowed(
            ".ocr.example.durability-uncertain",
            stale,
            false,
        ));
        assert!(stale_component_transaction_removal_allowed(
            ".ocr.example.previous",
            stale,
            true,
        ));
        assert!(stale_component_transaction_removal_allowed(
            ".ocr.example.durability-uncertain",
            stale,
            true,
        ));
        assert!(stale_component_transaction_removal_allowed(
            ".ocr.example.installing",
            stale,
            false,
        ));
    }

    #[test]
    fn component_transaction_lock_serializes_mutating_commit_boundaries() {
        use super::lock_component_transactions;
        use std::sync::mpsc;
        use std::time::Duration;

        let first = lock_component_transactions().unwrap();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _second = lock_component_transactions().unwrap();
            sender.send(()).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }
}
