use dokkomplekt_storage::LocalRepository;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

fn local_completion_receipt(app_data: &Path, processing_job_sha256: &str) -> PathBuf {
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

const RECEIPT_SCHEMA: u32 = 2;
const LEGACY_RECEIPT_SCHEMA: u32 = 1;
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
}

#[derive(Debug, Clone)]
pub(crate) struct PublicationPlanBinding {
    pub processing_job_sha256: String,
    pub source_sha256: String,
    pub processing_fingerprint: String,
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
    matches!(receipt.schema, LEGACY_RECEIPT_SCHEMA | RECEIPT_SCHEMA)
        && !receipt.reservation_id.trim().is_empty()
        && !receipt.output_sha256.trim().is_empty()
}

pub(crate) fn prepare_publication(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    staged_output: &Path,
    plan_binding: Option<&PublicationPlanBinding>,
) -> Result<(), String> {
    use tauri::Manager as _;

    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
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
    };
    write_receipt(
        &receipt_path(&app_data, &permit.reservation.reservation_id),
        &receipt,
    )
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

        match repo.finalize_published_usage(&receipt.reservation_id) {
            Ok(true) => {
                report.finalized += 1;
                match receipt.effective_phase() {
                    PublicationPhase::Prepared => {
                        report.ambiguous += 1;
                        report.warnings.push(
                            "Обнаружена pre-publication квитанция после прерывания процесса. Резервация дофинализирована консервативно, а квитанция сохранена, чтобы не допустить бесплатного или двойного повтора до ручной проверки."
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
        if report.ambiguous > 0 {
            eprintln!(
                "Обнаружено {} двусмысленных pre-publication состояний; повтор заблокирован до ручной проверки.",
                report.ambiguous
            );
        }
        for warning in report.warnings {
            eprintln!("Восстановление опубликованной генерации: {warning}");
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
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains("output_path"));
        assert!(!json.contains("source_path"));
        assert!(!json.contains("patient"));
        assert!(!json.contains("fio"));
    }
}
