use dokkomplekt_storage::{CounterValue, LocalRepository, UsageReservation};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

fn read_optional_guard_file(path: &Path, label: &str) -> Result<Option<String>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Не удалось безопасно проверить {label} {}: {error}",
                path.display()
            ))
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} не может быть ссылкой: {}", path.display()));
    }
    if !metadata.is_file() {
        return Err(format!(
            "{label} имеет недопустимый тип: {}",
            path.display()
        ));
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("Не удалось прочитать {label} {}: {error}", path.display()))
}

pub(crate) fn local_completion_receipt(app_data: &Path, processing_job_sha256: &str) -> PathBuf {
    app_data
        .join("intake-completion-receipts")
        .join(format!("{processing_job_sha256}.done"))
}

pub(crate) fn local_completion_receipt_matches(
    app_data: &Path,
    processing_job_sha256: &str,
    source_sha256: &str,
    processing_fingerprint: &str,
) -> Result<bool, String> {
    let path = local_completion_receipt(app_data, processing_job_sha256);
    let Some(body) = read_optional_guard_file(&path, "локальную квитанцию завершения")?
    else {
        return Ok(false);
    };
    let required = [
        "schema=1".to_string(),
        format!("processing_job_sha256={processing_job_sha256}"),
        format!("source_sha256={source_sha256}"),
        format!("processing_fingerprint={processing_fingerprint}"),
    ];
    if required
        .iter()
        .all(|expected| body.lines().any(|line| line.trim() == expected))
    {
        Ok(true)
    } else {
        Err(format!(
            "Локальная квитанция завершения повреждена или не соответствует plan binding: {}",
            path.display()
        ))
    }
}

pub(crate) fn plan_bound_emergency_completion_exists(
    source: &Path,
    processing_job_sha256: &str,
) -> Result<bool, String> {
    for path in crate::workspace_hygiene::processed_marker_candidates(source) {
        let Some(body) = read_optional_guard_file(&path, "аварийный publication guard")?
        else {
            continue;
        };
        let plan_matches = body
            .lines()
            .any(|line| line.trim() == format!("processing_job_sha256={processing_job_sha256}"));
        if !plan_matches {
            continue;
        }
        let terminal_attention = body.lines().any(|line| {
            matches!(
                line.trim(),
                "status=published_completion_ledgers_failed"
                    | "status=unverified_publication_quarantined"
            )
        });
        if terminal_attention {
            return Ok(true);
        }
        return Err(format!(
            "Plan-bound аварийный publication guard имеет неизвестный статус: {}",
            path.display()
        ));
    }
    Ok(false)
}

pub(crate) fn mark_plan_bound_emergency_guard(
    source: &Path,
    source_sha256: &str,
    processing_job_sha256: &str,
    status: &str,
) -> Result<PathBuf, String> {
    if !matches!(
        status,
        "published_completion_ledgers_failed" | "unverified_publication_quarantined"
    ) {
        return Err("Недопустимый статус аварийного publication guard.".into());
    }
    let marker = crate::workspace_hygiene::processed_marker_path(source);
    let payload = format!(
        "sha256={source_sha256}\nprocessing_job_sha256={processing_job_sha256}\nstatus={status}\n"
    );
    crate::atomic_write_file(&marker, payload.as_bytes()).map_err(|error| {
        format!(
            "Не удалось записать plan-bound аварийный publication guard {}: {error}",
            marker.display()
        )
    })?;
    Ok(marker)
}

pub(crate) fn mark_local_completion(
    app_data: &Path,
    processing_job_sha256: &str,
    source_sha256: &str,
    processing_fingerprint: &str,
) -> Result<PathBuf, String> {
    let final_path = local_completion_receipt(app_data, processing_job_sha256);
    let payload = format!(
        "schema=1\nprocessing_job_sha256={processing_job_sha256}\nsource_sha256={source_sha256}\nprocessing_fingerprint={processing_fingerprint}\ncompleted_unix={}\nhost={}\n",
        crate::unix_now_seconds(),
        crate::processing_lock_host_id(),
    );
    crate::atomic_write_file(&final_path, payload.as_bytes()).map_err(|error| {
        format!("Не удалось записать локальную квитанцию завершённого дела: {error}")
    })?;
    Ok(final_path)
}

const RECEIPT_SCHEMA: u32 = 3;
const LEGACY_RECEIPT_SCHEMA_V1: u32 = 1;
const LEGACY_RECEIPT_SCHEMA_V2: u32 = 2;
const RECEIPT_DIR: &str = "generation-publication-receipts";
const COMPLETION_RECEIPT_SCHEMA: u32 = 1;
const COMPLETION_RECEIPT_DIR: &str = "generation-completion-receipts";
const COMPLETION_PROOF_CONTRACT: &str = "publication-digest-v1";
const COMPLETION_VERIFIER_CONTRACT: &str = "published-readback-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicationPhase {
    Prepared,
    Published,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicationReceipt {
    schema: u32,
    reservation_id: String,
    output_sha256: String,
    #[serde(default)]
    phase: Option<PublicationPhase>,
    #[serde(default)]
    prepared_unix: Option<i64>,
    #[serde(default)]
    published_unix: Option<i64>,
    #[serde(default)]
    processing_job_sha256: Option<String>,
    #[serde(default)]
    source_sha256: Option<String>,
    #[serde(default)]
    processing_fingerprint: Option<String>,
    #[serde(default)]
    recovery_blob: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PublicationPlanBinding {
    pub processing_job_sha256: String,
    pub source_sha256: String,
    pub processing_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerationCompletionReceipt {
    schema: u32,
    receipt_id: String,
    output_id: String,
    output_sha256: String,
    status: String,
    committed_unix: i64,
    proof_contract: String,
    verifier_contract: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_binding_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicationRecoveryContext {
    stage_location: String,
    counter_reservations: Vec<CounterValue>,
    #[serde(default)]
    replacement_target: Option<String>,
    #[serde(default)]
    replacement_backup: Option<String>,
}

impl PublicationReceipt {
    fn effective_phase(&self) -> Option<PublicationPhase> {
        match (self.schema, self.phase) {
            (LEGACY_RECEIPT_SCHEMA_V1, None) => Some(PublicationPhase::Published),
            (_, Some(phase)) => Some(phase),
            _ => None,
        }
    }

    fn plan_binding_matches(
        &self,
        processing_job_sha256: &str,
        source_sha256: &str,
        processing_fingerprint: &str,
    ) -> bool {
        self.processing_job_sha256.as_deref() == Some(processing_job_sha256)
            && self.source_sha256.as_deref() == Some(source_sha256)
            && self.processing_fingerprint.as_deref() == Some(processing_fingerprint)
    }

    fn has_complete_plan_binding(&self) -> bool {
        self.processing_job_sha256
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && self
                .source_sha256
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            && self
                .processing_fingerprint
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    }
}

#[derive(Debug, Default)]
pub(crate) struct PublicationReconciliationReport {
    pub finalized: usize,
    pub rolled_back: usize,
    pub ambiguous: usize,
    pub warnings: Vec<String>,
}

fn receipt_name(reservation_id: &str) -> String {
    format!(
        "{:x}.receipt.json",
        Sha256::digest(reservation_id.as_bytes())
    )
}

fn receipt_path(app_data: &Path, reservation_id: &str) -> PathBuf {
    app_data
        .join(RECEIPT_DIR)
        .join(receipt_name(reservation_id))
}

#[cfg(unix)]
fn file_link_count(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.nlink())
}

#[cfg(windows)]
fn file_link_count(path: &Path) -> Option<u64> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let file = std::fs::File::open(path).ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    let succeeded =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &raw mut info) };
    (succeeded != 0).then_some(u64::from(info.nNumberOfLinks))
}

#[cfg(not(any(unix, windows)))]
fn file_link_count(_path: &Path) -> Option<u64> {
    None
}

fn staged_output_definitely_unpublished(context: &PublicationRecoveryContext) -> bool {
    let path = Path::new(&context.stage_location);
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    if metadata.is_dir() {
        return true;
    }
    metadata.is_file() && file_link_count(path) == Some(1)
}

fn recovery_blob(
    repo: &LocalRepository,
    staged_output: &Path,
    counter_reservations: &[CounterValue],
) -> Result<String, String> {
    let context = PublicationRecoveryContext {
        stage_location: staged_output.display().to_string(),
        counter_reservations: counter_reservations.to_vec(),
        replacement_target: None,
        replacement_backup: None,
    };
    let json = serde_json::to_string(&context).map_err(|error| error.to_string())?;
    repo.protect_local_value(&json)
        .map_err(|error| error.to_string())
}

fn decode_recovery_blob(
    repo: &LocalRepository,
    receipt: &PublicationReceipt,
) -> Result<Option<PublicationRecoveryContext>, String> {
    let Some(stored) = receipt.recovery_blob.as_deref() else {
        return Ok(None);
    };
    let json = repo
        .unprotect_local_value(stored)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| {
        format!("Не удалось прочитать результат для квитанции публикации: {error}")
    })?;
    hasher.update(b"file\0");
    hasher.update(Sha256::digest(bytes));
    Ok(())
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = std::fs::read_dir(current)
        .map_err(|error| format!("Не удалось проверить комплект перед публикацией: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Не удалось проверить комплект перед публикацией: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let ty = entry
            .file_type()
            .map_err(|error| format!("Не удалось проверить тип файла комплекта: {error}"))?;
        if ty.is_symlink() {
            return Err("Комплект неожиданно содержит символическую ссылку.".into());
        }
        if ty.is_dir() {
            collect_files(root, &path, files)?;
        } else if ty.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Файл вышел за границы комплекта.".to_string())?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

pub(crate) fn output_digest(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    if path.is_file() {
        hash_file(path, &mut hasher)?;
    } else if path.is_dir() {
        hasher.update(b"directory\0");
        let mut files = Vec::new();
        collect_files(path, path, &mut files)?;
        files.sort();
        for relative in files {
            let relative_hash = Sha256::digest(relative.to_string_lossy().as_bytes());
            hasher.update(relative_hash);
            hash_file(&path.join(relative), &mut hasher)?;
        }
    } else {
        return Err("Результат не найден для квитанции публикации.".into());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn stable_identity_hash(namespace: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn completion_receipt_from_publication(
    receipt: &PublicationReceipt,
) -> Result<GenerationCompletionReceipt, String> {
    if receipt.effective_phase() != Some(PublicationPhase::Published) {
        return Err(
            "Generation receipt допускается только после подтверждённой публикации.".into(),
        );
    }
    let plan_binding_sha256 = if receipt.has_complete_plan_binding() {
        Some(stable_identity_hash(
            "plan-binding-v1",
            &[
                receipt.processing_job_sha256.as_deref().unwrap_or_default(),
                receipt.source_sha256.as_deref().unwrap_or_default(),
                receipt
                    .processing_fingerprint
                    .as_deref()
                    .unwrap_or_default(),
            ],
        ))
    } else {
        None
    };
    let receipt_id = stable_identity_hash(
        "generation-receipt-v1",
        &[&receipt.reservation_id, &receipt.output_sha256],
    );
    let output_id = stable_identity_hash(
        "generation-output-v1",
        &[&receipt.reservation_id, &receipt.output_sha256],
    );
    Ok(GenerationCompletionReceipt {
        schema: COMPLETION_RECEIPT_SCHEMA,
        receipt_id,
        output_id,
        output_sha256: receipt.output_sha256.clone(),
        status: "committed".into(),
        committed_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        proof_contract: COMPLETION_PROOF_CONTRACT.into(),
        verifier_contract: COMPLETION_VERIFIER_CONTRACT.into(),
        plan_binding_sha256,
    })
}

fn completion_receipts_match_identity(
    existing: &GenerationCompletionReceipt,
    expected: &GenerationCompletionReceipt,
) -> bool {
    existing.schema == expected.schema
        && existing.receipt_id == expected.receipt_id
        && existing.output_id == expected.output_id
        && existing.output_sha256 == expected.output_sha256
        && existing.status == expected.status
        && existing.proof_contract == expected.proof_contract
        && existing.verifier_contract == expected.verifier_contract
        && existing.plan_binding_sha256 == expected.plan_binding_sha256
}

fn persist_generation_completion_receipt(
    app_data: &Path,
    publication: &PublicationReceipt,
) -> Result<PathBuf, String> {
    let receipt = completion_receipt_from_publication(publication)?;
    let path = app_data
        .join(COMPLETION_RECEIPT_DIR)
        .join(format!("{}.json", receipt.receipt_id));
    if path.exists() {
        let existing_bytes = std::fs::read(&path).map_err(|error| {
            format!("Не удалось прочитать существующий committed GenerationReceipt: {error}")
        })?;
        let existing = serde_json::from_slice::<GenerationCompletionReceipt>(&existing_bytes)
            .map_err(|error| {
                format!("Существующий committed GenerationReceipt повреждён: {error}")
            })?;
        if !completion_receipts_match_identity(&existing, &receipt) {
            return Err("Конфликт committed GenerationReceipt: существующая квитанция с тем же identity не соответствует опубликованному результату.".into());
        }
        return Ok(path);
    }
    let bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
    crate::atomic_write_file(&path, &bytes).map_err(|error| {
        format!("Не удалось зафиксировать committed GenerationReceipt: {error}")
    })?;
    Ok(path)
}

fn write_receipt(path: &Path, receipt: &PublicationReceipt) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?;
    crate::atomic_write_file(path, &bytes)
}

fn load_receipt(path: &Path) -> Result<PublicationReceipt, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice::<PublicationReceipt>(&bytes).map_err(|error| error.to_string())
}

fn known_receipt_identity(receipt: &PublicationReceipt) -> bool {
    matches!(
        receipt.schema,
        LEGACY_RECEIPT_SCHEMA_V1 | LEGACY_RECEIPT_SCHEMA_V2 | RECEIPT_SCHEMA
    ) && !receipt.reservation_id.trim().is_empty()
        && !receipt.output_sha256.trim().is_empty()
}

fn supported_receipt(receipt: &PublicationReceipt) -> bool {
    known_receipt_identity(receipt) && receipt.effective_phase().is_some()
}

pub(crate) fn prepare_publication(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    staged_output: &Path,
    counter_reservations: &[CounterValue],
    plan_binding: Option<&PublicationPlanBinding>,
) -> Result<(), String> {
    use tauri::Manager as _;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let state_path = crate::default_state_db_path(app)?;
    let repo = crate::repository_for(&state_path)?;
    let binding = plan_binding.cloned();
    let receipt = PublicationReceipt {
        schema: RECEIPT_SCHEMA,
        reservation_id: permit.reservation.reservation_id.clone(),
        output_sha256: output_digest(staged_output)?,
        phase: Some(PublicationPhase::Prepared),
        prepared_unix: Some(time::OffsetDateTime::now_utc().unix_timestamp()),
        published_unix: None,
        processing_job_sha256: binding
            .as_ref()
            .map(|value| value.processing_job_sha256.clone()),
        source_sha256: binding.as_ref().map(|value| value.source_sha256.clone()),
        processing_fingerprint: binding
            .as_ref()
            .map(|value| value.processing_fingerprint.clone()),
        recovery_blob: Some(recovery_blob(&repo, staged_output, counter_reservations)?),
    };
    write_receipt(
        &receipt_path(&app_data, &permit.reservation.reservation_id),
        &receipt,
    )
}

pub(crate) fn attach_replacement_recovery(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    target: &Path,
    backup: &Path,
) -> Result<(), String> {
    use tauri::Manager as _;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let path = receipt_path(&app_data, &permit.reservation.reservation_id);
    let mut receipt = load_receipt(&path)?;
    if receipt.schema != RECEIPT_SCHEMA
        || receipt.reservation_id != permit.reservation.reservation_id
        || receipt.effective_phase() != Some(PublicationPhase::Prepared)
    {
        return Err(
            "Pre-publication квитанция не допускает привязку recovery безопасной замены.".into(),
        );
    }
    let state_path = crate::default_state_db_path(app)?;
    let repo = crate::repository_for(&state_path)?;
    let mut context = decode_recovery_blob(&repo, &receipt)?
        .ok_or_else(|| "Pre-publication квитанция не содержит recovery-контекста.".to_string())?;
    context.replacement_target = Some(target.display().to_string());
    context.replacement_backup = Some(backup.display().to_string());
    let json = serde_json::to_string(&context).map_err(|error| error.to_string())?;
    receipt.recovery_blob = Some(
        repo.protect_local_value(&json)
            .map_err(|error| error.to_string())?,
    );
    write_receipt(&path, &receipt)
}

fn verify_published_output_digest(
    receipt: &PublicationReceipt,
    published_output: &Path,
) -> Result<(), String> {
    let published_sha256 = output_digest(published_output).map_err(|error| {
        format!("Нельзя доказать целостность опубликованного результата: {error}")
    })?;
    if published_sha256 != receipt.output_sha256 {
        return Err(format!("Опубликованный результат не совпал с подготовленным snapshot: ожидался SHA-256 {}, получен {}.", receipt.output_sha256, published_sha256));
    }
    Ok(())
}

pub(crate) fn confirm_publication(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    published_output: &Path,
) -> Result<Vec<String>, String> {
    use tauri::Manager as _;
    let app_data = app.path().app_data_dir().map_err(|error| {
        format!("Не удалось получить app-data для проверки публикации: {error}")
    })?;
    let path = receipt_path(&app_data, &permit.reservation.reservation_id);
    let mut receipt = load_receipt(&path).map_err(|error| {
        format!(
            "Нельзя доказать целостность публикации: pre-publication квитанция недоступна: {error}"
        )
    })?;
    if !supported_receipt(&receipt) || receipt.reservation_id != permit.reservation.reservation_id {
        return Err("Нельзя доказать целостность публикации: квитанция не соответствует резервации генерации.".into());
    }
    verify_published_output_digest(&receipt, published_output)?;
    receipt.schema = RECEIPT_SCHEMA;
    receipt.phase = Some(PublicationPhase::Published);
    receipt.published_unix = Some(time::OffsetDateTime::now_utc().unix_timestamp());
    write_receipt(&path, &receipt).map_err(|error| format!("Байты опубликованного результата подтверждены SHA-256, но durable-квитанцию не удалось перевести в состояние published: {error}"))?;
    persist_generation_completion_receipt(&app_data, &receipt)?;
    Ok(Vec::new())
}

pub(crate) fn abort_prepared_publication(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
) -> Result<(), String> {
    use tauri::Manager as _;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let path = receipt_path(&app_data, &permit.reservation.reservation_id);
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|error| format!("Не удалось удалить pre-publication квитанцию: {error}"))?;
    }
    Ok(())
}

pub(crate) fn complete_publication_receipt(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
) -> Result<(), String> {
    abort_prepared_publication(app, permit)
}

fn publication_receipt_for_permit(
    app_data: &Path,
    permit: &crate::GenerationPermit,
) -> Result<PublicationReceipt, String> {
    let receipt = load_receipt(&receipt_path(app_data, &permit.reservation.reservation_id))?;
    if !supported_receipt(&receipt) {
        return Err("Некорректная квитанция опубликованной генерации.".into());
    }
    Ok(receipt)
}

pub(crate) fn finalize_published_generation(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    retain_receipt_for_completion: bool,
) -> Vec<String> {
    use tauri::Manager as _;
    let app_data = app.path().app_data_dir();
    let publication_receipt = app_data
        .as_ref()
        .map_err(|error| error.to_string())
        .and_then(|app_data| publication_receipt_for_permit(app_data, permit));
    let accounting_result = crate::commit_generation_access(app, permit);
    if accounting_result.is_ok() {
        let mut warnings = Vec::new();
        match publication_receipt {
            Ok(receipt) if receipt.effective_phase() == Some(PublicationPhase::Published) => {
                if let Ok(app_data) = app_data {
                    if let Err(error) = persist_generation_completion_receipt(&app_data, &receipt) {
                        warnings.push(format!("Committed GenerationReceipt уже требовался на publication boundary, но повторная проверка записи не удалась: {error}"));
                    } else if !retain_receipt_for_completion {
                        remove_publication_receipt(&app_data, &permit.reservation.reservation_id);
                    }
                }
            }
            Ok(_) => warnings.push("Документ опубликован, но publication journal не содержит подтверждённую фазу Published; recovery guard сохранён.".to_string()),
            Err(error) => warnings.push(format!("Документ опубликован и committed receipt должен был быть записан на publication boundary, но recovery journal недоступен: {error}")),
        }
        return warnings;
    }
    let accounting_error = accounting_result
        .err()
        .unwrap_or_else(|| "unknown accounting error".into());
    let receipt_persisted = publication_receipt.is_ok();
    let warning = if receipt_persisted {
        "Документ опубликован. Учёт лимита будет автоматически дофинализирован по защищённой квитанции при следующем запуске.".to_string()
    } else {
        "Документ опубликован. Учёт лимита временно не дофинализирован; резервация сохранена и не возвращена, чтобы исключить бесплатную повторную выдачу.".to_string()
    };
    let details = serde_json::json!({
        "reservation_id": permit.reservation.reservation_id,
        "receipt_persisted": receipt_persisted,
        "accounting_error": accounting_error,
    });
    let _ = crate::create_automation_exception(
        app,
        "published_generation_accounting",
        "",
        &warning,
        &details,
    );
    let _ = crate::append_audit_event(
        app,
        "published_generation_accounting_degraded",
        "",
        &details,
    );
    vec![warning]
}

pub(crate) fn plan_bound_publication_guard_exists(
    app_data: &Path,
    processing_job_sha256: &str,
    source_sha256: &str,
    processing_fingerprint: &str,
) -> Result<bool, String> {
    let root = app_data.join(RECEIPT_DIR);
    let root_metadata = match std::fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Не удалось безопасно проверить каталог publication guards {}: {error}",
                root.display()
            ))
        }
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(format!(
            "Каталог publication guards имеет недопустимый тип: {}",
            root.display()
        ));
    }
    let entries = std::fs::read_dir(&root).map_err(|error| {
        format!(
            "Не удалось прочитать каталог publication guards {}: {error}",
            root.display()
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Не удалось прочитать запись каталога publication guards {}: {error}",
                root.display()
            )
        })?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Не удалось проверить publication guard {}: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "Publication guard имеет недопустимый тип: {}",
                path.display()
            ));
        }
        let receipt = load_receipt(&path)
            .map_err(|error| format!("Publication guard повреждён {}: {error}", path.display()))?;
        if !known_receipt_identity(&receipt) {
            return Err(format!(
                "Publication guard имеет неизвестный или неполный формат: {}",
                path.display()
            ));
        }
        if receipt.plan_binding_matches(
            processing_job_sha256,
            source_sha256,
            processing_fingerprint,
        ) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn remove_publication_receipt(app_data: &Path, reservation_id: &str) {
    let path = receipt_path(app_data, reservation_id);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

fn restore_interrupted_replacement(
    context: &PublicationRecoveryContext,
    report: &mut PublicationReconciliationReport,
) -> Result<(), String> {
    let (Some(target_raw), Some(backup_raw)) = (
        context.replacement_target.as_deref(),
        context.replacement_backup.as_deref(),
    ) else {
        return Ok(());
    };
    let target = Path::new(target_raw);
    let backup = Path::new(backup_raw);
    let target_parent = target
        .parent()
        .ok_or_else(|| "Recovery-путь публикации не имеет родительской папки.".to_string())?;
    let expected_backup_root = target_parent.join(".dokkomplekt-backups");
    if backup.parent() != Some(expected_backup_root.as_path()) {
        return Err("Recovery-путь резервной копии вышел за допустимый каталог backup.".into());
    }
    if !backup.exists() {
        return Ok(());
    }
    crate::publication_service_directory(target_parent, ".dokkomplekt-backups", false)?;
    let metadata = std::fs::symlink_metadata(backup)
        .map_err(|error| format!("Не удалось проверить резервную копию после сбоя: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Резервная копия после сбоя имеет небезопасный тип файла.".into());
    }
    if target.exists() {
        report.warnings.push("После прерванной безопасной замены исходный backup сохранён: пользовательский путь уже занят и не был перезаписан.".into());
        return Ok(());
    }
    std::fs::create_dir_all(target_parent).map_err(|error| {
        format!("Не удалось подготовить папку для восстановления backup: {error}")
    })?;
    std::fs::rename(backup, target).map_err(|error| {
        format!("Не удалось восстановить предыдущий комплект после сбоя: {error}")
    })?;
    report.warnings.push("После прерванной безопасной замены предыдущий пользовательский комплект автоматически восстановлен.".into());
    Ok(())
}

fn rollback_unpublished_receipt(
    repo: &mut LocalRepository,
    receipt: &PublicationReceipt,
    context: &PublicationRecoveryContext,
    report: &mut PublicationReconciliationReport,
) -> Result<bool, String> {
    if !staged_output_definitely_unpublished(context) {
        return Ok(false);
    }
    restore_interrupted_replacement(context, report)?;
    let reservation = UsageReservation {
        reservation_id: receipt.reservation_id.clone(),
        month_key: String::new(),
        documents: 0,
        trial: false,
    };
    if !repo
        .rollback_usage(&reservation)
        .map_err(|error| error.to_string())?
    {
        return Ok(false);
    }
    let mut counter_gaps = Vec::new();
    for counter in context.counter_reservations.iter().rev() {
        match repo.rollback_counter(counter) {
            Ok(true) => {}
            Ok(false) => counter_gaps.push(format!(
                "{}:{}={}",
                counter.counter_key, counter.year, counter.value
            )),
            Err(error) => counter_gaps.push(format!(
                "{}:{}={} ({error})",
                counter.counter_key, counter.year, counter.value
            )),
        }
    }
    report.rolled_back += 1;
    if !counter_gaps.is_empty() {
        report.warnings.push(format!("Неопубликованная генерация отменена и лимит возвращён, но более новые номера не позволяют безопасно откатить: {}.", counter_gaps.join(", ")));
    }
    Ok(true)
}

pub(crate) fn reconcile_publication_receipts(
    app_data: &Path,
    repo: &mut LocalRepository,
) -> PublicationReconciliationReport {
    let mut report = PublicationReconciliationReport::default();
    let root = app_data.join(RECEIPT_DIR);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return report;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let receipt = match load_receipt(&path) {
            Ok(receipt) if supported_receipt(&receipt) => receipt,
            Ok(receipt) if known_receipt_identity(&receipt) => {
                report.ambiguous += 1;
                report.warnings.push("Квитанция известного формата не содержит допустимую фазу публикации; автоматическая финализация и повтор заблокированы до ручной проверки.".into());
                continue;
            }
            Ok(_) => {
                report.warnings.push("Некорректная квитанция опубликованной генерации оставлена для ручной проверки.".into());
                continue;
            }
            Err(_) => {
                report.warnings.push("Повреждённая квитанция опубликованной генерации оставлена для ручной проверки.".into());
                continue;
            }
        };
        if receipt.effective_phase() == Some(PublicationPhase::Prepared)
            && receipt.schema == RECEIPT_SCHEMA
        {
            match decode_recovery_blob(repo, &receipt) {
                Ok(Some(context)) => match rollback_unpublished_receipt(repo, &receipt, &context, &mut report) {
                    Ok(true) => { let _ = std::fs::remove_file(&path); continue; }
                    Ok(false) => {}
                    Err(error) => report.warnings.push(format!("Не удалось отменить доказанно неопубликованную генерацию: {error}")),
                },
                Ok(None) => report.warnings.push("Pre-publication квитанция нового формата не содержит recovery-контекста; применяется консервативная финализация.".into()),
                Err(error) => report.warnings.push(format!("Recovery-контекст pre-publication квитанции повреждён ({error}); применяется консервативная финализация.")),
            }
        }
        match repo.finalize_published_usage(&receipt.reservation_id) {
            Ok(true) => {
                report.finalized += 1;
                match receipt.effective_phase() {
                    Some(PublicationPhase::Prepared) => {
                        report.ambiguous += 1;
                        report.warnings.push("Обнаружена pre-publication квитанция после прерывания процесса. Публикация не может быть доказанно исключена, поэтому резервация дофинализирована консервативно, а квитанция сохранена от бесплатного или двойного повтора.".into());
                    }
                    Some(PublicationPhase::Published) if receipt.has_complete_plan_binding() => {
                        let generation_completion = persist_generation_completion_receipt(app_data, &receipt);
                        let local_completion = mark_local_completion(
                            app_data,
                            receipt.processing_job_sha256.as_deref().unwrap_or_default(),
                            receipt.source_sha256.as_deref().unwrap_or_default(),
                            receipt.processing_fingerprint.as_deref().unwrap_or_default(),
                        );
                        if generation_completion.is_ok() && local_completion.is_ok() {
                            let _ = std::fs::remove_file(path);
                        } else {
                            report.warnings.push("Учёт опубликованного комплекта восстановлен, но durable completion evidence записано не полностью; publication guard сохранён.".into());
                        }
                    }
                    Some(PublicationPhase::Published) => {
                        if persist_generation_completion_receipt(app_data, &receipt).is_ok() {
                            let _ = std::fs::remove_file(path);
                        } else {
                            report.warnings.push("Учёт опубликованного результата восстановлен, но committed GenerationReceipt не записан; publication guard сохранён.".into());
                        }
                    }
                    None => {
                        report.ambiguous += 1;
                        report.warnings.push("Квитанция не содержит допустимую фазу публикации; состояние оставлено для ручной проверки.".into());
                    }
                }
            }
            Ok(false) => report.warnings.push("Квитанция опубликованной генерации не связана с известной резервацией лимита.".into()),
            Err(_) => report.warnings.push("Учёт опубликованной генерации пока не удалось финализировать; квитанция сохранена для следующего запуска.".into()),
        }
    }
    report
}

fn recover_stale_prepublication_reservations(
    app_data: &Path,
    repo: &mut LocalRepository,
) -> Result<usize, String> {
    let stale = repo
        .stale_publication_recovery_reservations(24 * 60)
        .map_err(|error| error.to_string())?;
    let mut rolled_back = 0usize;
    for reservation in stale {
        let path = receipt_path(app_data, &reservation.reservation_id);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Не удалось безопасно проверить publication receipt {}: {error}",
                    path.display()
                ))
            }
        }
        if repo
            .rollback_usage(&reservation)
            .map_err(|error| error.to_string())?
        {
            rolled_back += 1;
        }
    }
    Ok(rolled_back)
}

pub(crate) fn recover_startup_generation_state(app: &tauri::AppHandle, repo: &mut LocalRepository) {
    use tauri::Manager as _;
    if let Ok(app_data) = app.path().app_data_dir() {
        let report = reconcile_publication_receipts(&app_data, repo);
        if report.finalized > 0 {
            eprintln!(
                "Восстановлен учёт {} опубликованных генераций после сбоя.",
                report.finalized
            );
        }
        if report.rolled_back > 0 {
            eprintln!(
                "Отменено {} доказанно неопубликованных генераций после сбоя; лимит возвращён.",
                report.rolled_back
            );
        }
        if report.ambiguous > 0 {
            eprintln!("Обнаружено {} двусмысленных pre-publication состояний; повтор заблокирован до ручной проверки.", report.ambiguous);
        }
        for warning in report.warnings {
            eprintln!("Восстановление опубликованной генерации: {warning}");
        }
        match recover_stale_prepublication_reservations(&app_data, repo) {
            Ok(count) if count > 0 => eprintln!(
                "Возвращён лимит для {count} зависших v3-резерваций без publication receipt."
            ),
            Ok(_) => {}
            Err(error) => eprintln!(
                "Не удалось безопасно восстановить v3-резервации до publication receipt: {error}"
            ),
        }
    }
    if let Err(error) = repo.recover_stale_usage_reservations(24 * 60) {
        eprintln!("Не удалось восстановить зависшие резервации лимита: {error}");
    }
    if let Err(error) = repo.recover_interrupted_case_runs() {
        eprintln!("Не удалось восстановить прерванные дела: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_serialization_does_not_store_output_paths_or_patient_data() {
        let receipt = PublicationReceipt {
            schema: RECEIPT_SCHEMA,
            reservation_id: "123-2026-08-456".into(),
            output_sha256: "ab".repeat(32),
            phase: Some(PublicationPhase::Prepared),
            prepared_unix: Some(1),
            published_unix: None,
            processing_job_sha256: Some("job".into()),
            source_sha256: Some("source".into()),
            processing_fingerprint: Some("plan".into()),
            recovery_blob: Some("enc:v1:opaque".into()),
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains("output_path"));
        assert!(!json.contains("source_path"));
        assert!(!json.contains("patient"));
        assert!(!json.contains("fio"));
    }

    fn receipt_fixture(schema: u32, phase: Option<PublicationPhase>) -> PublicationReceipt {
        PublicationReceipt {
            schema,
            reservation_id: "reservation".into(),
            output_sha256: "ab".repeat(32),
            phase,
            prepared_unix: Some(1),
            published_unix: None,
            processing_job_sha256: Some("job".into()),
            source_sha256: Some("source".into()),
            processing_fingerprint: Some("plan".into()),
            recovery_blob: None,
        }
    }

    #[test]
    fn committed_generation_receipt_is_non_pii_and_bound_to_output() {
        let publication = PublicationReceipt {
            schema: RECEIPT_SCHEMA,
            reservation_id: "raw-reservation-secret".into(),
            output_sha256: "a".repeat(64),
            phase: Some(PublicationPhase::Published),
            prepared_unix: Some(1),
            published_unix: Some(2),
            processing_job_sha256: Some("b".repeat(64)),
            source_sha256: Some("c".repeat(64)),
            processing_fingerprint: Some("d".repeat(64)),
            recovery_blob: Some("must-never-copy-to-completion".into()),
        };
        let completion = completion_receipt_from_publication(&publication)
            .expect("published receipt creates committed completion proof");
        let json = serde_json::to_string(&completion).unwrap();
        assert_eq!(completion.status, "committed");
        assert_eq!(completion.output_sha256, "a".repeat(64));
        assert_eq!(completion.proof_contract, COMPLETION_PROOF_CONTRACT);
        assert_eq!(completion.verifier_contract, COMPLETION_VERIFIER_CONTRACT);
        assert!(!json.contains("raw-reservation-secret"));
        assert!(!json.contains("must-never-copy-to-completion"));
        assert!(!json.contains("stage_location"));
        assert!(!json.contains("replacement_target"));
    }

    #[test]
    fn committed_generation_receipt_is_idempotent_and_conflict_safe() {
        let root = temp_root("committed-receipt-idempotency");
        let publication = PublicationReceipt {
            schema: RECEIPT_SCHEMA,
            reservation_id: "stable-reservation".into(),
            output_sha256: "e".repeat(64),
            phase: Some(PublicationPhase::Published),
            prepared_unix: Some(1),
            published_unix: Some(2),
            processing_job_sha256: Some("f".repeat(64)),
            source_sha256: Some("1".repeat(64)),
            processing_fingerprint: Some("2".repeat(64)),
            recovery_blob: None,
        };
        let path = persist_generation_completion_receipt(&root, &publication)
            .expect("first committed receipt write");
        let mut existing: GenerationCompletionReceipt =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        existing.committed_unix = 123;
        let stable_bytes = serde_json::to_vec_pretty(&existing).unwrap();
        std::fs::write(&path, &stable_bytes).unwrap();
        let replayed = persist_generation_completion_receipt(&root, &publication)
            .expect("identical replay must reuse committed receipt");
        assert_eq!(replayed, path);
        assert_eq!(std::fs::read(&path).unwrap(), stable_bytes);
        let mut conflicting: GenerationCompletionReceipt =
            serde_json::from_slice(&stable_bytes).unwrap();
        conflicting.output_id = "0".repeat(64);
        std::fs::write(&path, serde_json::to_vec_pretty(&conflicting).unwrap()).unwrap();
        let error = persist_generation_completion_receipt(&root, &publication)
            .expect_err("conflicting committed receipt must fail closed");
        assert!(error.contains("Конфликт committed GenerationReceipt"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn only_schema_v1_may_omit_publication_phase() {
        let v1 = receipt_fixture(LEGACY_RECEIPT_SCHEMA_V1, None);
        assert_eq!(v1.effective_phase(), Some(PublicationPhase::Published));
        assert!(supported_receipt(&v1));
        for schema in [LEGACY_RECEIPT_SCHEMA_V2, RECEIPT_SCHEMA] {
            let missing_phase = receipt_fixture(schema, None);
            assert_eq!(missing_phase.effective_phase(), None);
            assert!(known_receipt_identity(&missing_phase));
            assert!(!supported_receipt(&missing_phase));
        }
    }

    #[test]
    fn explicit_new_schema_phases_remain_valid() {
        for schema in [LEGACY_RECEIPT_SCHEMA_V2, RECEIPT_SCHEMA] {
            for phase in [PublicationPhase::Prepared, PublicationPhase::Published] {
                let receipt = receipt_fixture(schema, Some(phase));
                assert_eq!(receipt.effective_phase(), Some(phase));
                assert!(supported_receipt(&receipt));
            }
        }
    }

    #[test]
    fn ambiguous_phase_still_blocks_same_plan_automatic_retry() {
        let root = temp_root("ambiguous-phase-guard");
        let receipts = root.join(RECEIPT_DIR);
        std::fs::create_dir_all(&receipts).unwrap();
        let receipt = receipt_fixture(RECEIPT_SCHEMA, None);
        write_receipt(&receipts.join("ambiguous.receipt.json"), &receipt).unwrap();
        assert!(plan_bound_publication_guard_exists(&root, "job", "source", "plan").unwrap());
        assert!(
            !plan_bound_publication_guard_exists(&root, "other-job", "source", "plan").unwrap()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_guard_files_are_the_only_safe_absence() {
        let root = temp_root("missing-guards");
        let source = root.join("Исходник.docx");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&source, b"source").unwrap();
        assert!(!local_completion_receipt_matches(&root, "job", "source", "plan").unwrap());
        assert!(!plan_bound_emergency_completion_exists(&source, "job").unwrap());
        assert!(!plan_bound_publication_guard_exists(&root, "job", "source", "plan").unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_local_completion_guard_fails_closed() {
        let root = temp_root("corrupt-local-guard");
        let path = local_completion_receipt(&root, "job");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"schema=1\nprocessing_job_sha256=job\n").unwrap();
        let error = local_completion_receipt_matches(&root, "job", "source", "plan")
            .expect_err("corrupt completion guard must not mean not-completed");
        assert!(error.contains("повреждена"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unreadable_publication_guard_namespace_fails_closed() {
        let root = temp_root("guard-root-is-file");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(RECEIPT_DIR), b"not a directory").unwrap();
        let error = plan_bound_publication_guard_exists(&root, "job", "source", "plan")
            .expect_err("invalid receipt namespace must stop automatic issuance");
        assert!(error.contains("недопустимый тип"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_publication_receipt_fails_closed() {
        let root = temp_root("malformed-receipt");
        let receipts = root.join(RECEIPT_DIR);
        std::fs::create_dir_all(&receipts).unwrap();
        std::fs::write(receipts.join("broken.receipt.json"), b"{not-json").unwrap();
        let error = plan_bound_publication_guard_exists(&root, "job", "source", "plan")
            .expect_err("malformed publication receipt must stop automatic issuance");
        assert!(error.contains("повреждён"), "{error}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn publication_digest_proof_rejects_any_post_prepare_change() {
        let root = temp_root("digest-proof");
        std::fs::create_dir_all(&root).unwrap();
        let published = root.join("Комплект");
        std::fs::create_dir_all(&published).unwrap();
        std::fs::write(published.join("Документ.docx"), b"exact prepared bytes").unwrap();
        let expected = output_digest(&published).unwrap();
        let receipt = PublicationReceipt {
            schema: RECEIPT_SCHEMA,
            reservation_id: "reservation".into(),
            output_sha256: expected,
            phase: Some(PublicationPhase::Prepared),
            prepared_unix: Some(1),
            published_unix: None,
            processing_job_sha256: None,
            source_sha256: None,
            processing_fingerprint: None,
            recovery_blob: None,
        };
        assert!(verify_published_output_digest(&receipt, &published).is_ok());
        std::fs::write(published.join("Документ.docx"), b"changed after prepare").unwrap();
        let error = verify_published_output_digest(&receipt, &published)
            .expect_err("changed published bytes must never confirm");
        assert!(
            error.contains("не совпал с подготовленным snapshot"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unverified_emergency_guard_is_plan_bound_and_blocks_only_same_job() {
        let root = temp_root("unverified-guard");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("Исходник.docx");
        std::fs::write(&source, b"source").unwrap();
        let job = "a".repeat(64);
        mark_plan_bound_emergency_guard(
            &source,
            &"b".repeat(64),
            &job,
            "unverified_publication_quarantined",
        )
        .unwrap();
        assert!(plan_bound_emergency_completion_exists(&source, &job).unwrap());
        assert!(!plan_bound_emergency_completion_exists(&source, &"c".repeat(64)).unwrap());
        assert!(
            mark_plan_bound_emergency_guard(&source, &"b".repeat(64), &job, "retryable").is_err()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dokkomplekt-publication-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn existing_stage_directory_proves_nonpublication() {
        let root = temp_root("stage-directory");
        let stage = root.join(".stage");
        std::fs::create_dir_all(&stage).unwrap();
        let context = PublicationRecoveryContext {
            stage_location: stage.display().to_string(),
            counter_reservations: Vec::new(),
            replacement_target: None,
            replacement_backup: None,
        };
        assert!(staged_output_definitely_unpublished(&context));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_replacement_restores_previous_directory() {
        let root = temp_root("replace-restore");
        let target = root.join("Комплект");
        let backup = root
            .join(".dokkomplekt-backups")
            .join("Комплект.backup-test");
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("old.docx"), b"old").unwrap();
        let context = PublicationRecoveryContext {
            stage_location: root.join(".stage").display().to_string(),
            counter_reservations: Vec::new(),
            replacement_target: Some(target.display().to_string()),
            replacement_backup: Some(backup.display().to_string()),
        };
        let mut report = PublicationReconciliationReport::default();
        restore_interrupted_replacement(&context, &mut report).unwrap();
        assert_eq!(std::fs::read(target.join("old.docx")).unwrap(), b"old");
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_replacement_never_follows_symlinked_backup_root() {
        use std::os::unix::fs::symlink;
        let root = temp_root("replace-symlink-recovery");
        let target = root.join("Комплект");
        let external = temp_root("replace-symlink-recovery-external");
        let external_backup = external.join("Комплект.backup-test");
        std::fs::create_dir_all(&external_backup).unwrap();
        std::fs::write(external_backup.join("old.docx"), b"old").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        symlink(&external, root.join(".dokkomplekt-backups")).unwrap();
        let backup = root
            .join(".dokkomplekt-backups")
            .join("Комплект.backup-test");
        let context = PublicationRecoveryContext {
            stage_location: root.join(".stage").display().to_string(),
            counter_reservations: Vec::new(),
            replacement_target: Some(target.display().to_string()),
            replacement_backup: Some(backup.display().to_string()),
        };
        let mut report = PublicationReconciliationReport::default();
        let error = restore_interrupted_replacement(&context, &mut report)
            .expect_err("recovery must not traverse symlinked backup root");
        assert!(error.contains("небезопасный тип"), "{error}");
        assert!(!target.exists());
        assert!(external_backup.join("old.docx").is_file());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(external);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn hard_link_marks_single_file_stage_as_ambiguous_after_publication() {
        let root = temp_root("hard-link-boundary");
        std::fs::create_dir_all(&root).unwrap();
        let stage = root.join(".stage.tmp");
        let published = root.join("document.docx");
        std::fs::write(&stage, b"document").unwrap();
        let context = PublicationRecoveryContext {
            stage_location: stage.display().to_string(),
            counter_reservations: Vec::new(),
            replacement_target: None,
            replacement_backup: None,
        };
        assert!(staged_output_definitely_unpublished(&context));
        std::fs::hard_link(&stage, &published).unwrap();
        assert!(!staged_output_definitely_unpublished(&context));
        let _ = std::fs::remove_dir_all(root);
    }
}
