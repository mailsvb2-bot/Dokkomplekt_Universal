use dokkomplekt_storage::LocalRepository;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

const RECEIPT_SCHEMA: u32 = 1;
const RECEIPT_DIR: &str = "generation-publication-receipts";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicationReceipt {
    schema: u32,
    reservation_id: String,
    output_sha256: String,
    published_unix: i64,
}

#[derive(Debug, Default)]
pub(crate) struct PublicationReconciliationReport {
    pub finalized: usize,
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
        format!("Не удалось прочитать опубликованный файл для квитанции: {error}")
    })?;
    hasher.update(b"file\0");
    hasher.update(Sha256::digest(bytes));
    Ok(())
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = std::fs::read_dir(current)
        .map_err(|error| format!("Не удалось проверить опубликованный комплект: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Не удалось проверить опубликованный комплект: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let ty = entry
            .file_type()
            .map_err(|error| format!("Не удалось проверить тип опубликованного файла: {error}"))?;
        if ty.is_symlink() {
            return Err("Опубликованный комплект неожиданно содержит символическую ссылку.".into());
        }
        if ty.is_dir() {
            collect_files(root, &path, files)?;
        } else if ty.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Опубликованный файл вышел за границы комплекта.".to_string())?;
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
        return Err("Опубликованный результат не найден для квитанции.".into());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn write_publication_receipt(
    app_data: &Path,
    reservation_id: &str,
    output: &Path,
) -> Result<PathBuf, String> {
    let receipt = PublicationReceipt {
        schema: RECEIPT_SCHEMA,
        reservation_id: reservation_id.to_string(),
        output_sha256: output_digest(output)?,
        published_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
    };
    let path = receipt_path(app_data, reservation_id);
    let bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?;
    crate::atomic_write_file(&path, &bytes)?;
    Ok(path)
}

pub(crate) fn remove_publication_receipt(app_data: &Path, reservation_id: &str) {
    let path = receipt_path(app_data, reservation_id);
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn finalize_published_generation(
    app: &tauri::AppHandle,
    permit: &crate::GenerationPermit,
    published_output: &Path,
) -> Vec<String> {
    use tauri::Manager as _;

    let app_data = app.path().app_data_dir();
    let receipt_result = app_data
        .as_ref()
        .map_err(|error| error.to_string())
        .and_then(|app_data| {
            write_publication_receipt(
                app_data,
                &permit.reservation.reservation_id,
                published_output,
            )
        });
    let accounting_result = crate::commit_generation_access(app, permit);
    if accounting_result.is_ok() {
        if let Ok(app_data) = app_data {
            remove_publication_receipt(&app_data, &permit.reservation.reservation_id);
        }
        if let Err(error) = receipt_result {
            let _ = crate::append_audit_event(
                app,
                "publication_receipt_degraded_after_accounting_commit",
                "",
                &serde_json::json!({
                    "reservation_id": permit.reservation.reservation_id,
                    "error": error,
                }),
            );
        }
        return Vec::new();
    }

    let accounting_error = accounting_result
        .err()
        .unwrap_or_else(|| "unknown accounting error".into());
    let receipt_persisted = receipt_result.is_ok();
    let warning = if receipt_persisted {
        "Документ опубликован. Учёт лимита будет автоматически дофинализирован по защищённой квитанции при следующем запуске.".to_string()
    } else {
        "Документ опубликован. Учёт лимита временно не дофинализирован; зарезервированная квота сохранена и не возвращена, чтобы исключить бесплатную повторную выдачу.".to_string()
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
        let receipt = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<PublicationReceipt>(&bytes)
                    .map_err(|error| error.to_string())
            });
        let receipt = match receipt {
            Ok(receipt)
                if receipt.schema == RECEIPT_SCHEMA
                    && !receipt.reservation_id.trim().is_empty() =>
            {
                receipt
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
        match repo.finalize_published_usage(&receipt.reservation_id) {
            Ok(true) => {
                report.finalized += 1;
                let _ = std::fs::remove_file(path);
            }
            Ok(false) => report.warnings.push(
                "Квитанция опубликованной генерации не связана с известной резервацией лимита.".into(),
            ),
            Err(_) => report.warnings.push(
                "Учёт опубликованной генерации пока не удалось финализировать; квитанция сохранена для следующего запуска.".into(),
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
            published_unix: 1,
        };
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains("output_path"));
        assert!(!json.contains("source_path"));
        assert!(!json.contains("patient"));
        assert!(!json.contains("fio"));
    }
}
