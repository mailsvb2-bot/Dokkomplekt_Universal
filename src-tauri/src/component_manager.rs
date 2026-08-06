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
use std::time::{Duration as StdDuration, SystemTime};
use tauri::Emitter as _;
use time::OffsetDateTime;
use uuid::Uuid;
use zip::ZipArchive;

const COMPONENT_CATALOG_SCHEMA: u32 = 1;
const COMPONENT_STATUS_SCHEMA: u32 = 1;
const COMPONENT_FILES_SCHEMA: u32 = 1;
const MAX_COMPONENT_CATALOG_BYTES: u64 = 256 * 1024;
const MAX_COMPONENT_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_COMPONENT_ENTRIES: usize = 50_000;
const MAX_COMPONENT_UNPACKED_BYTES: u64 = 12 * 1024 * 1024 * 1024;
const STALE_COMPONENT_TRANSACTION_AGE: StdDuration = StdDuration::from_secs(6 * 60 * 60);
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
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ComponentsCatalogPayload {
    pub schema: u32,
    pub app_min_version: String,
    pub published_at: String,
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
    let catalog = read_cached_catalog(&root).ok()?;
    let target = crate::current_update_platform();
    for descriptor in catalog
        .payload
        .components
        .iter()
        .filter(|component| component.target == target)
        .filter(|component| component.unlocks.iter().any(|item| item == program))
    {
        let component_dir = root.join(&descriptor.id);
        if validate_installed_component(&component_dir, descriptor).is_err() {
            continue;
        }
        let manifest = read_component_files_manifest(&component_dir, descriptor).ok()?;
        let mut candidates = Vec::new();
        append_component_tool_candidates(&mut candidates, &component_dir, program, executable_name);
        for candidate in candidates {
            let relative = candidate.strip_prefix(&component_dir).ok()?;
            let relative_key = path_key(relative)?;
            let expected = manifest.files.get(&relative_key)?;
            if candidate.is_file()
                && sha256_file(&candidate)
                    .ok()
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            {
                return Some(candidate);
            }
        }
    }
    None
}

pub(crate) fn component_statuses() -> Vec<ComponentStatus> {
    let root = match user_components_dir() {
        Some(value) => value,
        None => return fallback_statuses("Пользовательская папка компонентов недоступна."),
    };
    let catalog = match read_cached_catalog(&root) {
        Ok(value) => value,
        Err(error) => {
            return fallback_statuses(&format!("Подписанный каталог ещё не загружен: {error}"))
        }
    };
    let _ = recover_component_transactions(&root, Some(&catalog));
    statuses_from_catalog(&root, &catalog)
}

pub(crate) fn refresh_component_catalog(
    app: &tauri::AppHandle,
) -> Result<Vec<ComponentStatus>, String> {
    let catalog = fetch_and_verify_catalog(app)?;
    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    recover_component_transactions(&root, Some(&catalog))?;
    guard_catalog_not_older(&root, &catalog)?;
    atomic_write_json(&root.join("components-catalog.json"), &catalog)?;
    recover_component_transactions(&root, Some(&catalog))?;
    Ok(statuses_from_catalog(&root, &catalog))
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

pub(crate) fn remove_component(id: &str) -> Result<ComponentStatus, String> {
    let id = validate_component_id(id)?;
    let root =
        user_components_dir().ok_or_else(|| "Нет пользовательского каталога данных".to_string())?;
    let catalog = read_cached_catalog(&root)?;
    let descriptor = catalog
        .payload
        .components
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
    guard_catalog_not_older(&root, &catalog)?;
    atomic_write_json(&root.join("components-catalog.json"), &catalog)?;

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
    let stage_dir = root.join(format!(".{}.{}.installing", id, Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        std::fs::create_dir(&stage_dir).map_err(|error| error.to_string())?;
        safe_extract_zip(&temp_archive, &stage_dir)?;
        let manifest = read_component_files_manifest(&stage_dir, &descriptor)?;
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
        atomic_write_json(&stage_dir.join("component-status.json"), &receipt)?;
        let final_dir = root.join(id);
        let previous_dir = root.join(format!(".{}.{}.previous", id, Uuid::new_v4()));
        if final_dir.exists() {
            std::fs::rename(&final_dir, &previous_dir).map_err(|error| error.to_string())?;
        }
        if let Err(error) = std::fs::rename(&stage_dir, &final_dir) {
            if previous_dir.exists() {
                let _ = std::fs::rename(&previous_dir, &final_dir);
            }
            return Err(error.to_string());
        }
        if previous_dir.exists() {
            let _ = std::fs::remove_dir_all(previous_dir);
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp_archive);
    if let Err(error) = result {
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
    catalog: Option<&SignedComponentsCatalog>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let root_metadata = std::fs::symlink_metadata(root).map_err(|error| error.to_string())?;
    if !root_metadata.file_type().is_dir() || root_metadata.file_type().is_symlink() {
        return Err("Пользовательский каталог компонентов имеет небезопасный тип".into());
    }

    if let Some(catalog) = catalog {
        for descriptor in &catalog.payload.components {
            if descriptor.target != crate::current_update_platform() {
                continue;
            }
            let final_dir = root.join(&descriptor.id);
            if final_dir.exists() {
                continue;
            }
            let mut previous = std::fs::read_dir(root)
                .map_err(|error| error.to_string())?
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_name().to_str().is_some_and(|name| {
                        transaction_name_matches(name, &descriptor.id, "previous")
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
            for (_, candidate) in previous {
                let valid = validate_installed_component(&candidate, descriptor)
                    .and_then(|_| read_component_files_manifest(&candidate, descriptor))
                    .and_then(|manifest| validate_all_manifest_files(&candidate, &manifest));
                if valid.is_ok() {
                    std::fs::rename(&candidate, &final_dir).map_err(|error| {
                        format!(
                            "Не удалось восстановить предыдущую версию компонента {}: {error}",
                            descriptor.id
                        )
                    })?;
                    break;
                }
            }
        }
    }

    let now = SystemTime::now();
    for entry in std::fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let is_transaction = name.starts_with('.')
            && (name.ends_with(".download-part")
                || name.ends_with(".installing")
                || name.ends_with(".previous"));
        if !is_transaction {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age = now.duration_since(modified).unwrap_or_default();
        if age < STALE_COMPONENT_TRANSACTION_AGE {
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
    catalog_published_at(catalog)?;
    let current = crate::parse_semver(env!("CARGO_PKG_VERSION"))?;
    let minimum = crate::parse_semver(&catalog.payload.app_min_version)?;
    if current < minimum {
        return Err(format!(
            "Каталог требует Dokkomplekt {} или новее; установлена {}",
            minimum, current
        ));
    }
    if catalog.payload.allowed_hosts.is_empty() {
        return Err("Подписанный каталог не содержит allow-list доменов".into());
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
    if let Ok(current) = read_cached_catalog(root) {
        let current_at = catalog_published_at(&current)?;
        if incoming_at < current_at {
            return Err("Подписанный каталог старее уже принятого; rollback отклонён".into());
        }
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
    // Catalog verification must be deterministic and offline: validate the URL
    // structure here, then resolve and pin public IP addresses immediately before
    // the actual download in `install_component_blocking`.
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

fn read_cached_catalog(root: &Path) -> Result<SignedComponentsCatalog, String> {
    let path = root.join("components-catalog.json");
    let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Кэш каталога компонентов имеет небезопасный тип".into());
    }
    if metadata.len() > MAX_COMPONENT_CATALOG_BYTES {
        return Err("Кэш каталога компонентов превышает лимит".into());
    }
    let catalog: SignedComponentsCatalog =
        serde_json::from_slice(&std::fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Некорректный кэш components catalog: {error}"))?;
    verify_catalog(&catalog)?;
    Ok(catalog)
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
    Ok(())
}

fn statuses_from_catalog(root: &Path, catalog: &SignedComponentsCatalog) -> Vec<ComponentStatus> {
    let sidecars = crate::universal_intake::sidecar_tool_statuses();
    catalog
        .payload
        .components
        .iter()
        .filter(|component| component.target == crate::current_update_platform())
        .map(|component| status_for_descriptor(root, component, true, &sidecars))
        .collect()
}

fn status_for_descriptor(
    root: &Path,
    descriptor: &ComponentDescriptor,
    catalog_available: bool,
    sidecars: &[crate::universal_intake::SidecarToolStatus],
) -> ComponentStatus {
    let component_dir = root.join(&descriptor.id);
    let valid = validate_installed_component(&component_dir, descriptor)
        .and_then(|_| read_component_files_manifest(&component_dir, descriptor))
        .and_then(|manifest| validate_all_manifest_files(&component_dir, &manifest));
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
        output.sync_all().map_err(|error| error.to_string())?;
    }
    Ok(())
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

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Некорректный путь JSON".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("component"),
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    output
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    output.write_all(b"\n").map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())?;
    drop(output);
    if path.exists() {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            let _ = std::fs::remove_file(&temporary);
            return Err("Целевой JSON имеет небезопасный тип".into());
        }
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
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
            url: "https://downloads.dokkomplekt.ru/ocr.zip".into(),
        };
        let payload = ComponentsCatalogPayload {
            schema: 1,
            app_min_version: env!("CARGO_PKG_VERSION").into(),
            published_at: "2026-07-20T00:00:00Z".into(),
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
    }

    #[test]
    fn installed_component_manifest_detects_tampering() {
        use super::{
            read_component_files_manifest, validate_all_manifest_files,
            validate_installed_component, ComponentDescriptor, ComponentFilesManifest,
            InstalledComponentReceipt, COMPONENT_FILES_SCHEMA, COMPONENT_STATUS_SCHEMA,
        };
        use sha2::{Digest as _, Sha256};
        use std::collections::BTreeMap;
        use uuid::Uuid;

        let root =
            std::env::temp_dir().join(format!("dokkomplekt-component-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let tool = root.join("bin").join("tool");
        std::fs::write(&tool, b"trusted").unwrap();
        let tool_hash = hex::encode(Sha256::digest(b"trusted"));
        let manifest = ComponentFilesManifest {
            schema: COMPONENT_FILES_SCHEMA,
            component_id: "ocr".into(),
            target: super::crate_platform_for_test(),
            files: BTreeMap::from([("bin/tool".into(), tool_hash)]),
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
        std::fs::write(&tool, b"tampered").unwrap();
        assert!(validate_all_manifest_files(&root, &parsed).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_component_upgrade_restores_verified_previous_version() {
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
        let catalog = SignedComponentsCatalog {
            payload: ComponentsCatalogPayload {
                schema: 1,
                app_min_version: env!("CARGO_PKG_VERSION").into(),
                published_at: "2026-07-20T00:00:00Z".into(),
                allowed_hosts: vec!["example.com".into()],
                components: vec![descriptor],
            },
            signature_alg: "Ed25519".into(),
            signature: String::new(),
        };
        recover_component_transactions(&root, Some(&catalog)).unwrap();
        assert!(root.join("ocr/bin/tool").is_file());
        assert!(!previous.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
