from pathlib import Path

workspace_path = Path("src-tauri/src/workspace_hygiene.rs")
workspace = workspace_path.read_text(encoding="utf-8")

workspace = workspace.replace(
    "use time::OffsetDateTime;\n",
    "use time::OffsetDateTime;\nuse uuid::Uuid;\n",
    1,
)

workspace = workspace.replace(
    'const UNREADABLE_SUFFIX: &str = " — НЕ ПРОЧИТАН.txt";\n',
    'const UNREADABLE_SUFFIX: &str = " — НЕ ПРОЧИТАН.txt";\n'
    'const FINALIZING_PREFIX: &str = ".dokkomplekt-finalizing-";\n'
    'const FINALIZING_SUFFIX: &str = ".pending";\n'
    'const FINALIZING_CLAIM_GRACE: Duration = Duration::from_secs(30 * 60);\n'
    'const RECOVERED_SOURCE_PREFIX: &str = "ВОССТАНОВЛЕННЫЙ ИСХОДНИК";\n',
    1,
)

workspace = workspace.replace(
    "    pub removed_queue_receipts: Vec<String>,\n    pub warnings: Vec<String>,\n",
    "    pub removed_queue_receipts: Vec<String>,\n"
    "    pub recovered_finalizing_sources: Vec<String>,\n"
    "    pub warnings: Vec<String>,\n",
    1,
)

archive_start = "pub fn archive_processed_source(\n"
cleanup_start = "pub fn cleanup_workspace_folder(\n"
if workspace.count(archive_start) != 1 or workspace.count(cleanup_start) != 1:
    raise SystemExit("archive/cleanup function markers are not unique")
prefix, remainder = workspace.split(archive_start, 1)
_, suffix = remainder.split(cleanup_start, 1)
new_archive = r'''pub fn archive_processed_source(
    source: &Path,
    source_sha256: &str,
    policy: &WorkspaceRetentionPolicy,
) -> Result<ProcessedSourceArchiveResult, String> {
    policy.validate()?;
    let parent = source
        .parent()
        .ok_or_else(|| "У источника нет родительской папки.".to_string())?;
    let marker = processed_marker_path(source);
    if !policy.archive_processed_sources {
        return Ok(ProcessedSourceArchiveResult {
            archived_source: None,
            receipt_path: None,
            marker_removed: false,
        });
    }

    let month_folder = archive_month_folder(parent, policy);
    create_real_directory_below(parent, &month_folder).map_err(|error| {
        format!(
            "Не удалось создать безопасную папку архива {}: {error}",
            month_folder.display()
        )
    })?;

    // Bind destructive cleanup to the file identity that exists at this instant.
    // A replacement created later at `source` is a different pathname and is never
    // touched by the archive/delete phase below.
    let claim = claim_matching_source(source, source_sha256)?;
    let destination = match copy_claim_to_unique_archive(&claim, &month_folder, source) {
        Ok(path) => path,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(error, recovery));
        }
    };
    let archived_sha256 = sha256_file(&destination)?;
    if archived_sha256 != claim.verified_sha256 {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            "Контрольная сумма архивной копии изменилась до фиксации квитанции.".into(),
            recovery,
        ));
    }

    let receipt = destination.with_file_name(format!(
        "{}.dokkomplekt-receipt.json",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source")
    ));
    if let Err(error) = write_receipt(&receipt, source, &destination, &archived_sha256) {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(error, recovery));
    }

    if let Err(error) = fs::remove_file(&claim.path) {
        let _ = fs::remove_file(&receipt);
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            format!(
                "Архивная копия подготовлена, но захваченный исходник {} не удалён: {error}",
                claim.path.display()
            ),
            recovery,
        ));
    }

    let marker_removed = if marker.exists() {
        fs::remove_file(&marker).is_ok()
    } else {
        false
    };

    Ok(ProcessedSourceArchiveResult {
        archived_source: Some(destination.display().to_string()),
        receipt_path: Some(receipt.display().to_string()),
        marker_removed,
    })
}

pub fn delete_processed_source_if_matches(
    source: &Path,
    source_sha256: &str,
) -> Result<(), String> {
    let claim = claim_matching_source(source, source_sha256)?;
    let final_sha256 = match sha256_file(&claim.path) {
        Ok(hash) => hash,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(
                format!("Не удалось повторно проверить захваченный исходник: {error}"),
                recovery,
            ));
        }
    };
    if final_sha256 != claim.verified_sha256 {
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            "Захваченный исходник изменился перед удалением; удаление отменено.".into(),
            recovery,
        ));
    }
    fs::remove_file(&claim.path).map_err(|error| {
        let recovery = recover_finalizing_claim(&claim.path);
        with_recovery_detail(
            format!(
                "Не удалось удалить проверенный захваченный исходник {}: {error}",
                claim.path.display()
            ),
            recovery,
        )
    })
}

'''
workspace = prefix + new_archive + cleanup_start + suffix

old_loop = r'''    for entry in entries.flatten() {
        let path = entry.path();
        if path == archive_root || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let age = file_age(&path, now);
'''
new_loop = r'''    for entry in entries.flatten() {
        let path = entry.path();
        if path == archive_root {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if is_finalizing_claim_name(name) {
            match finalizing_claim_timestamp(name) {
                Some(claimed_at) if finalizing_claim_is_stale(claimed_at, now) => {
                    match recover_finalizing_claim(&path) {
                        Ok(recovered) => report
                            .recovered_finalizing_sources
                            .push(recovered.display().to_string()),
                        Err(error) => report.warnings.push(error),
                    }
                }
                Some(_) => {}
                None => report.warnings.push(format!(
                    "Пропущен malformed finalization claim: {}",
                    path.display()
                )),
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let age = file_age(&path, now);
'''
if workspace.count(old_loop) != 1:
    raise SystemExit(f"workspace cleanup loop marker mismatch: {workspace.count(old_loop)}")
workspace = workspace.replace(old_loop, new_loop, 1)

helper_marker = "fn validate_archive_folder_name(name: &str) -> Result<(), String> {\n"
if workspace.count(helper_marker) != 1:
    raise SystemExit("helper insertion marker mismatch")
helpers = r'''#[derive(Debug)]
struct FinalizingSourceClaim {
    path: PathBuf,
    verified_sha256: String,
}

fn normalize_expected_sha256(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Ожидаемый SHA-256 источника имеет неверный формат.".into());
    }
    Ok(normalized)
}

fn finalizing_claim_path(source: &Path) -> Result<PathBuf, String> {
    let parent = source
        .parent()
        .ok_or_else(|| "У источника нет родительской папки.".to_string())?;
    let timestamp = OffsetDateTime::now_utc().unix_timestamp();
    let base = format!("{FINALIZING_PREFIX}{timestamp}-{}", Uuid::new_v4());
    let name = match source.extension().and_then(|value| value.to_str()) {
        Some(extension) if !extension.is_empty() => {
            format!("{base}.{extension}{FINALIZING_SUFFIX}")
        }
        _ => format!("{base}{FINALIZING_SUFFIX}"),
    };
    Ok(parent.join(name))
}

fn claim_matching_source(source: &Path, expected_sha256: &str) -> Result<FinalizingSourceClaim, String> {
    let expected_sha256 = normalize_expected_sha256(expected_sha256)?;
    let before = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "Не удалось проверить исходник перед безопасной финализацией {}: {error}",
            source.display()
        )
    })?;
    if metadata_is_link_or_reparse(&before) || !before.is_file() {
        return Err(format!(
            "Небезопасный исходник заблокирован перед финализацией: {}",
            source.display()
        ));
    }

    let claim_path = finalizing_claim_path(source)?;
    if claim_path.exists() {
        return Err("Не удалось подобрать уникальное имя для finalization claim.".into());
    }
    fs::rename(source, &claim_path).map_err(|error| {
        format!(
            "Не удалось атомарно захватить исходник {} для финализации: {error}",
            source.display()
        )
    })?;

    let claimed_metadata = fs::symlink_metadata(&claim_path).map_err(|error| {
        format!(
            "Исходник захвачен как {}, но не удалось проверить его тип: {error}",
            claim_path.display()
        )
    })?;
    if metadata_is_link_or_reparse(&claimed_metadata) || !claimed_metadata.is_file() {
        return Err(format!(
            "Захваченный исходник небезопасен; он сохранён без удаления: {}",
            claim_path.display()
        ));
    }

    let actual_sha256 = match sha256_file(&claim_path) {
        Ok(hash) => hash,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim_path);
            return Err(with_recovery_detail(
                format!("Не удалось проверить SHA-256 захваченного исходника: {error}"),
                recovery,
            ));
        }
    };
    if actual_sha256 != expected_sha256 {
        let recovery = recover_finalizing_claim(&claim_path);
        return Err(with_recovery_detail(
            format!(
                "Исходник был заменён между проверкой и финализацией: ожидался {expected_sha256}, захвачен {actual_sha256}."
            ),
            recovery,
        ));
    }
    Ok(FinalizingSourceClaim {
        path: claim_path,
        verified_sha256: actual_sha256,
    })
}

fn copy_claim_to_unique_archive(
    claim: &FinalizingSourceClaim,
    folder: &Path,
    original_source: &Path,
) -> Result<PathBuf, String> {
    for _ in 0..=10_000u32 {
        let destination = unique_destination(folder, original_source, &claim.verified_sha256)?;
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось безопасно создать архивный файл {}: {error}",
                    destination.display()
                ));
            }
        };
        let before_sha256 = sha256_file(&claim.path)?;
        if before_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Захваченный исходник изменился до архивирования.".into());
        }
        let mut input = fs::File::open(&claim.path).map_err(|error| error.to_string())?;
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            let _ = fs::remove_file(&destination);
            return Err(format!("Не удалось скопировать захваченный исходник в архив: {error}"));
        }
        if let Err(error) = output.sync_all() {
            let _ = fs::remove_file(&destination);
            return Err(format!("Не удалось синхронизировать архивную копию: {error}"));
        }
        drop(output);
        let after_sha256 = sha256_file(&claim.path)?;
        let copied_sha256 = sha256_file(&destination)?;
        if after_sha256 != claim.verified_sha256 || copied_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Контрольная сумма изменилась во время безопасного архивирования.".into());
        }
        return Ok(destination);
    }
    Err("Не удалось подобрать уникальное имя для архивного источника.".into())
}

fn is_finalizing_claim_name(name: &str) -> bool {
    name.starts_with(FINALIZING_PREFIX) && name.ends_with(FINALIZING_SUFFIX)
}

fn finalizing_claim_timestamp(name: &str) -> Option<i64> {
    let body = name
        .strip_prefix(FINALIZING_PREFIX)?
        .strip_suffix(FINALIZING_SUFFIX)?;
    let (timestamp, _) = body.split_once('-')?;
    timestamp.parse().ok()
}

fn finalizing_claim_is_stale(claimed_at: i64, now: SystemTime) -> bool {
    let Ok(now_since_epoch) = now.duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let Ok(claimed_at) = u64::try_from(claimed_at) else {
        return true;
    };
    now_since_epoch
        .as_secs()
        .saturating_sub(claimed_at)
        >= FINALIZING_CLAIM_GRACE.as_secs()
}

fn finalizing_claim_extension(claim: &Path) -> Option<String> {
    let name = claim.file_name()?.to_str()?;
    let without_pending = name.strip_suffix(FINALIZING_SUFFIX)?;
    Path::new(without_pending)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn recover_finalizing_claim(claim: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(claim).map_err(|error| {
        format!(
            "Не удалось проверить finalization claim {}: {error}",
            claim.display()
        )
    })?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Небезопасный finalization claim сохранён без обработки: {}",
            claim.display()
        ));
    }
    let parent = claim
        .parent()
        .ok_or_else(|| "У finalization claim нет родительской папки.".to_string())?;
    let extension = finalizing_claim_extension(claim);
    for _ in 0..256u16 {
        let stem = format!("{RECOVERED_SOURCE_PREFIX} {}", Uuid::new_v4());
        let name = extension
            .as_deref()
            .map(|ext| format!("{stem}.{ext}"))
            .unwrap_or(stem);
        let destination = parent.join(name);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось создать recovery-файл {}: {error}",
                    destination.display()
                ));
            }
        };
        let before_sha256 = sha256_file(claim)?;
        let mut input = fs::File::open(claim).map_err(|error| error.to_string())?;
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            let _ = fs::remove_file(&destination);
            return Err(format!("Не удалось сохранить recovery-копию: {error}"));
        }
        if let Err(error) = output.sync_all() {
            let _ = fs::remove_file(&destination);
            return Err(format!("Не удалось синхронизировать recovery-копию: {error}"));
        }
        drop(output);
        let after_sha256 = sha256_file(claim)?;
        let recovered_sha256 = sha256_file(&destination)?;
        if before_sha256 != after_sha256 || before_sha256 != recovered_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Finalization claim изменился во время recovery; исходник оставлен нетронутым.".into());
        }
        if let Err(error) = fs::remove_file(claim) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Recovery-копия подготовлена, но claim {} не удалён: {error}",
                claim.display()
            ));
        }
        return Ok(destination);
    }
    Err("Не удалось подобрать уникальное имя для recovery-файла.".into())
}

fn with_recovery_detail(message: String, recovery: Result<PathBuf, String>) -> String {
    match recovery {
        Ok(path) => format!("{message} Файл сохранён для повторной обработки: {}", path.display()),
        Err(error) => format!("{message} Recovery: {error}"),
    }
}

'''
workspace = workspace.replace(helper_marker, helpers + helper_marker, 1)

# Add regressions before the final archive-folder validation test.
test_marker = r'''    #[test]
    fn archive_folder_cannot_escape_working_directory() {
'''
if workspace.count(test_marker) != 1:
    raise SystemExit("workspace test insertion marker mismatch")
new_tests = r'''    fn hash_bytes(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn recovered_files(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with(RECOVERED_SOURCE_PREFIX))
            })
            .collect()
    }

    #[test]
    fn archive_never_archives_replacement_under_stale_sha() {
        let root = temp_root("archive-replacement");
        let source = root.join("case.docx");
        fs::write(&source, b"replacement").unwrap();
        let error = archive_processed_source(
            &source,
            &hash_bytes(b"processed-old-version"),
            &WorkspaceRetentionPolicy::default(),
        )
        .unwrap_err();
        assert!(error.contains("заменён"));
        let recovered = recovered_files(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(fs::read(&recovered[0]).unwrap(), b"replacement");
        let receipts = fs::read_dir(root.join("_обработано"))
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_file())
            .count();
        assert_eq!(receipts, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_never_removes_replacement_under_stale_sha() {
        let root = temp_root("delete-replacement");
        let source = root.join("case.docx");
        fs::write(&source, b"replacement").unwrap();
        let error = delete_processed_source_if_matches(&source, &hash_bytes(b"old"))
            .unwrap_err();
        assert!(error.contains("заменён"));
        let recovered = recovered_files(&root);
        assert_eq!(recovered.len(), 1);
        assert_eq!(fs::read(&recovered[0]).unwrap(), b"replacement");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_removes_only_matching_claimed_source() {
        let root = temp_root("delete-matching");
        let source = root.join("case.docx");
        fs::write(&source, b"processed").unwrap();
        let hash = sha256_file(&source).unwrap();
        delete_processed_source_if_matches(&source, &hash).unwrap();
        assert!(!source.exists());
        assert!(recovered_files(&root).is_empty());
        assert!(fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(FINALIZING_PREFIX)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_receipt_uses_verified_archived_sha() {
        let root = temp_root("archive-receipt-sha");
        let source = root.join("case.docx");
        fs::write(&source, b"processed").unwrap();
        let hash = sha256_file(&source).unwrap();
        let result = archive_processed_source(
            &source,
            &hash,
            &WorkspaceRetentionPolicy::default(),
        )
        .unwrap();
        let archived = PathBuf::from(result.archived_source.unwrap());
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(result.receipt_path.unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["sha256"].as_str(), Some(sha256_file(&archived).unwrap().as_str()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_recovers_stale_finalization_claim_as_supported_source() {
        let root = temp_root("stale-finalization-claim");
        let claim = root.join(format!(
            "{FINALIZING_PREFIX}1-{}.docx{FINALIZING_SUFFIX}",
            Uuid::new_v4()
        ));
        fs::write(&claim, b"survives-crash").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);
        let report = cleanup_workspace_folder(
            &root,
            &WorkspaceRetentionPolicy::default(),
            now,
        )
        .unwrap();
        assert_eq!(report.recovered_finalizing_sources.len(), 1);
        assert!(!claim.exists());
        let recovered = PathBuf::from(&report.recovered_finalizing_sources[0]);
        assert_eq!(recovered.extension().and_then(|value| value.to_str()), Some("docx"));
        assert_eq!(fs::read(&recovered).unwrap(), b"survives-crash");
        let _ = fs::remove_dir_all(root);
    }

'''
workspace = workspace.replace(test_marker, new_tests + test_marker, 1)
workspace_path.write_text(workspace, encoding="utf-8")

runtime_path = Path("src-tauri/src/subsystems/automation_runtime.rs")
runtime = runtime_path.read_text(encoding="utf-8")
old_delete = r'''    if privacy.copy_source_to_output {
        match std::fs::remove_file(source) {
            Ok(()) => {
                let _ = std::fs::remove_file(&marker);
                Ok(serde_json::json!({
                    "action": "source_deleted_after_copy",
                    "marker_removed": true,
                }))
            }
            Err(error) => {
                std::fs::write(
                    &marker,
                    format!(
                        "sha256={source_sha256}\nstatus=published_source_delete_delayed\nerror_kind={:?}\n",
                        error.kind()
                    ),
                )
                .map_err(|marker_error| {
                    format!(
                        "Комплект создан, но исходник не удалён ({error}) и маркер не записан ({marker_error})."
                    )
                })?;
                Ok(serde_json::json!({
                    "action": "source_delete_delayed",
                    "marker": marker.display().to_string(),
                    "error_kind": format!("{:?}", error.kind()),
                }))
            }
        }
'''
new_delete = r'''    if privacy.copy_source_to_output {
        match workspace_hygiene::delete_processed_source_if_matches(source, source_sha256) {
            Ok(()) => {
                let _ = std::fs::remove_file(&marker);
                Ok(serde_json::json!({
                    "action": "source_deleted_after_copy",
                    "marker_removed": true,
                }))
            }
            Err(error) => {
                std::fs::write(
                    &marker,
                    format!(
                        "sha256={source_sha256}\nstatus=published_source_delete_delayed\nerror={error}\n"
                    ),
                )
                .map_err(|marker_error| {
                    format!(
                        "Комплект создан, но исходник не удалён ({error}) и маркер не записан ({marker_error})."
                    )
                })?;
                Ok(serde_json::json!({
                    "action": "source_delete_delayed",
                    "marker": marker.display().to_string(),
                    "error": error,
                }))
            }
        }
'''
if runtime.count(old_delete) != 1:
    raise SystemExit(f"runtime destructive delete marker mismatch: {runtime.count(old_delete)}")
runtime_path.write_text(runtime.replace(old_delete, new_delete, 1), encoding="utf-8")

intake_path = Path("src-tauri/src/universal_intake.rs")
intake = intake_path.read_text(encoding="utf-8")
intake_marker = '        || name.contains(".dokkomplekt-processing")\n        || name.contains(".dokkomplekt-processed")\n'
intake_replacement = '        || name.contains(".dokkomplekt-processing")\n        || name.contains(".dokkomplekt-processed")\n        || name.contains(".dokkomplekt-finalizing-")\n'
if intake.count(intake_marker) != 1:
    raise SystemExit("temporary-source marker mismatch")
intake_path.write_text(intake.replace(intake_marker, intake_replacement, 1), encoding="utf-8")

docs_path = Path("docs/CRASH_CONSISTENCY.md")
docs = docs_path.read_text(encoding="utf-8")
old_docs = "After a successful publication, destructive archive/delete hygiene is skipped when the live source no longer matches the processed SHA-256, so a newly replaced source is never deleted as if it were the old case."
new_docs = "After a successful publication, destructive archive/delete hygiene first atomically renames the live pathname to a private same-directory `.pending` claim and then verifies the claimed bytes against the processed SHA-256. Destruction and archive receipts are bound to that verified claim rather than the reusable live pathname; a replacement created under the original name is never touched. A mismatched claimed file is recovered under a visible unique name, while stale `.pending` claims left by a process/OS crash are recovered by workspace hygiene after a bounded grace period."
if docs.count(old_docs) != 1:
    raise SystemExit("crash consistency documentation marker mismatch")
docs_path.write_text(docs.replace(old_docs, new_docs, 1), encoding="utf-8")

test_path = Path("tests/test_v18_4_7_source_finalization_identity.py")
if test_path.exists():
    raise SystemExit("source finalization identity contract already exists")
test_path.write_text(r'''from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "src-tauri" / "src" / "subsystems" / "automation_runtime.rs"
HYGIENE = ROOT / "src-tauri" / "src" / "workspace_hygiene.rs"
INTAKE = ROOT / "src-tauri" / "src" / "universal_intake.rs"
DOCS = ROOT / "docs" / "CRASH_CONSISTENCY.md"


def _finalizer() -> str:
    text = RUNTIME.read_text(encoding="utf-8")
    return text.split("fn finalize_processed_source(", 1)[1].split(
        "fn automation_plan_fingerprint", 1
    )[0]


def test_finalizer_has_no_path_based_delete_after_hash_precheck() -> None:
    body = _finalizer()
    assert "std::fs::remove_file(source)" not in body
    assert "delete_processed_source_if_matches(source, source_sha256)" in body
    assert "archive_processed_source(" in body


def test_hygiene_claims_path_before_post_claim_hash_and_destructive_actions() -> None:
    text = HYGIENE.read_text(encoding="utf-8")
    claim = text.split("fn claim_matching_source(", 1)[1].split(
        "fn copy_claim_to_unique_archive", 1
    )[0]
    assert "fs::rename(source, &claim_path)" in claim
    assert claim.index("fs::rename(source, &claim_path)") < claim.index("sha256_file(&claim_path)")
    assert "recover_finalizing_claim(&claim_path)" in claim
    delete = text.split("pub fn delete_processed_source_if_matches(", 1)[1].split(
        "pub fn cleanup_workspace_folder", 1
    )[0]
    assert "claim_matching_source(source, source_sha256)" in delete
    assert "fs::remove_file(&claim.path)" in delete


def test_archive_receipt_uses_verified_archived_bytes_not_caller_metadata() -> None:
    text = HYGIENE.read_text(encoding="utf-8")
    archive = text.split("pub fn archive_processed_source(", 1)[1].split(
        "pub fn delete_processed_source_if_matches", 1
    )[0]
    assert "let archived_sha256 = sha256_file(&destination)?;" in archive
    assert "write_receipt(&receipt, source, &destination, &archived_sha256)" in archive
    assert "create_new(true)" in text


def test_pending_claim_is_not_intakeable_and_stale_claims_are_recovered() -> None:
    hygiene = HYGIENE.read_text(encoding="utf-8")
    intake = INTAKE.read_text(encoding="utf-8")
    assert 'const FINALIZING_SUFFIX: &str = ".pending";' in hygiene
    assert 'name.contains(".dokkomplekt-finalizing-")' in intake
    assert "recovered_finalizing_sources" in hygiene
    assert "finalizing_claim_is_stale" in hygiene
    assert "recover_finalizing_claim(&path)" in hygiene


def test_crash_consistency_contract_documents_identity_claim() -> None:
    docs = DOCS.read_text(encoding="utf-8")
    assert "atomically renames the live pathname" in docs
    assert "verified claim rather than the reusable live pathname" in docs
    assert "stale `.pending` claims" in docs
''', encoding="utf-8")
