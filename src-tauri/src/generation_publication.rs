use dokkomplekt_storage::{CounterValue, LocalRepository, UsageReservation};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

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
) -> bool {
    let Ok(body) =
        std::fs::read_to_string(local_completion_receipt(app_data, processing_job_sha256))
    else {
        return false;
    };
    let required = [
        format!("processing_job_sha256={processing_job_sha256}"),
        format!("source_sha256={source_sha256}"),
        format!("processing_fingerprint={processing_fingerprint}"),
    ];
    required
        .iter()
        .all(|expected| body.lines().any(|line| line.trim() == expected))
}

pub(crate) fn plan_bound_emergency_completion_exists(
    source: &Path,
    processing_job_sha256: &str,
) -> bool {
    crate::workspace_hygiene::processed_marker_candidates(source)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|body| {
            body.lines()
                .any(|line| line.trim() == format!("processing_job_sha256={processing_job_sha256}"))
                && body
                    .lines()
                    .any(|line| line.trim() == "status=published_completion_ledgers_failed")
        })
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
struct PublicationRecoveryContext {
    stage_location: String,
    counter_reservations: Vec<CounterValue>,
    #[serde(default)]
    replacement_target: Option<String>,
    #[serde(default)]
    replacement_backup: Option<String>,
}

impl PublicationReceipt {
    fn effective_phase(&self) -> PublicationPhase {
        // Schema v1 receipts were written only after filesystem publication.
        self.phase.unwrap_or(PublicationPhase::Published)
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
    // SAFETY: `file` owns a valid open HANDLE for the duration of the call and
    // `info` is a writable Win32 output structure. No handle ownership is transferred.
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

fn write_receipt(path: &Path, receipt: &PublicationReceipt) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?;
    crate::atomic_write_file(path, &bytes)
}

fn load_receipt(path: &Path) -> Result<PublicationReceipt, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice::<PublicationReceipt>(&bytes).map_err(|error| error.to_string())
}

fn supported_receipt(receipt: &PublicationReceipt) -> bool {
    matches!(
        receipt.schema,
        LEGACY_RECEIPT_SCHEMA_V1 | LEGACY_RECEIPT_SCHEMA_V2 | RECEIPT_SCHEMA
    ) && !receipt.reservation_id.trim().is_empty()
        && !receipt.output_sha256.trim().is_empty()
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
        || receipt.effective_phase() != PublicationPhase::Prepared
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

pub(crate) fn confirm_publication(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    published_output: &Path,
) -> Result<(), String> {
    use tauri::Manager as _;

    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let path = receipt_path(&app_data, &permit.reservation.reservation_id);
    let mut receipt = load_receipt(&path)?;
    if !supported_receipt(&receipt) || receipt.reservation_id != permit.reservation.reservation_id {
        return Err("Квитанция публикации не соответствует резервации генерации.".into());
    }
    let published_sha256 = output_digest(published_output)?;
    if published_sha256 != receipt.output_sha256 {
        return Err(
            "Опубликованный результат не совпал с подготовленным snapshot; автоматическая финализация остановлена."
                .into(),
        );
    }
    receipt.schema = RECEIPT_SCHEMA;
    receipt.phase = Some(PublicationPhase::Published);
    receipt.published_unix = Some(time::OffsetDateTime::now_utc().unix_timestamp());
    write_receipt(&path, &receipt)
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

fn receipt_phase_for_permit(
    app_data: &Path,
    permit: &crate::GenerationPermit,
) -> Result<PublicationPhase, String> {
    let receipt = load_receipt(&receipt_path(app_data, &permit.reservation.reservation_id))?;
    if !supported_receipt(&receipt) {
        return Err("Некорректная квитанция опубликованной генерации.".into());
    }
    Ok(receipt.effective_phase())
}

pub(crate) fn finalize_published_generation(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    retain_receipt_for_completion: bool,
) -> Vec<String> {
    use tauri::Manager as _;

    let app_data = app.path().app_data_dir();
    let phase = app_data
        .as_ref()
        .map_err(|error| error.to_string())
        .and_then(|app_data| receipt_phase_for_permit(app_data, permit));
    let accounting_result = crate::commit_generation_access(app, permit);

    if accounting_result.is_ok() {
        let mut warnings = Vec::new();
        match phase {
            Ok(PublicationPhase::Published) if !retain_receipt_for_completion => {
                if let Ok(app_data) = app_data {
                    remove_publication_receipt(&app_data, &permit.reservation.reservation_id);
                }
            }
            Ok(PublicationPhase::Prepared) => warnings.push(
                "Документ опубликован, но подтверждение границы публикации не записалось. Учёт зафиксирован, pre-publication квитанция сохранена как защита от двусмысленного повтора."
                    .to_string(),
            ),
            Err(error) => warnings.push(format!(
                "Документ опубликован и учёт зафиксирован, но квитанция публикации недоступна: {error}"
            )),
            _ => {}
        }
        return warnings;
    }

    let accounting_error = accounting_result
        .err()
        .unwrap_or_else(|| "unknown accounting error".into());
    let receipt_persisted = phase.is_ok();
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
) -> bool {
    let root = app_data.join(RECEIPT_DIR);
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_file()
            && load_receipt(&path)
                .ok()
                .filter(supported_receipt)
                .is_some_and(|receipt| {
                    receipt.plan_binding_matches(
                        processing_job_sha256,
                        source_sha256,
                        processing_fingerprint,
                    )
                })
    })
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
    let metadata = std::fs::symlink_metadata(backup)
        .map_err(|error| format!("Не удалось проверить резервную копию после сбоя: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Резервная копия после сбоя имеет небезопасный тип файла.".into());
    }
    if target.exists() {
        report.warnings.push(
            "После прерванной безопасной замены исходный backup сохранён: пользовательский путь уже занят и не был перезаписан."
                .into(),
        );
        return Ok(());
    }
    std::fs::create_dir_all(target_parent).map_err(|error| {
        format!("Не удалось подготовить папку для восстановления backup: {error}")
    })?;
    std::fs::rename(backup, target).map_err(|error| {
        format!("Не удалось восстановить предыдущий комплект после сбоя: {error}")
    })?;
    report.warnings.push(
        "После прерванной безопасной замены предыдущий пользовательский комплект автоматически восстановлен."
            .into(),
    );
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
            Ok(_) => {
                report.warnings.push(
                    "Некорректная квитанция опубликованной генерации оставлена для ручной проверки."
                        .into(),
                );
                continue;
            }
            Err(_) => {
                report.warnings.push(
                    "Повреждённая квитанция опубликованной генерации оставлена для ручной проверки."
                        .into(),
                );
                continue;
            }
        };

        if receipt.effective_phase() == PublicationPhase::Prepared
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
                    PublicationPhase::Prepared => {
                        report.ambiguous += 1;
                        report.warnings.push(
                            "Обнаружена pre-publication квитанция после прерывания процесса. Публикация не может быть доказанно исключена, поэтому резервация дофинализирована консервативно, а квитанция сохранена от бесплатного или двойного повтора."
                                .into(),
                        );
                    }
                    PublicationPhase::Published if receipt.has_complete_plan_binding() => {
                        let local_completion = mark_local_completion(
                            app_data,
                            receipt.processing_job_sha256.as_deref().unwrap_or_default(),
                            receipt.source_sha256.as_deref().unwrap_or_default(),
                            receipt.processing_fingerprint.as_deref().unwrap_or_default(),
                        );
                        if local_completion.is_ok() {
                            let _ = std::fs::remove_file(path);
                        } else {
                            report.warnings.push(
                                "Учёт опубликованного комплекта восстановлен, но plan-bound квитанцию завершения записать не удалось; publication guard сохранён."
                                    .into(),
                            );
                        }
                    }
                    PublicationPhase::Published => {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
            Ok(false) => report.warnings.push(
                "Квитанция опубликованной генерации не связана с известной резервацией лимита."
                    .into(),
            ),
            Err(_) => report.warnings.push(
                "Учёт опубликованной генерации пока не удалось финализировать; квитанция сохранена для следующего запуска."
                    .into(),
            ),
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
                ));
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
            eprintln!(
                "Обнаружено {} двусмысленных pre-publication состояний; повтор заблокирован до ручной проверки.",
                report.ambiguous
            );
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
