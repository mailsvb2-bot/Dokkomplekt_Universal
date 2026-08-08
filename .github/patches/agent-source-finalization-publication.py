from pathlib import Path

workspace_path = Path("src-tauri/src/workspace_hygiene.rs")
workspace = workspace_path.read_text(encoding="utf-8")

start = "fn copy_claim_to_unique_archive(\n"
end = "fn is_finalizing_claim_name(name: &str) -> bool {\n"
if workspace.count(start) != 1 or workspace.count(end) != 1:
    raise SystemExit("archive publication function markers are not unique")
prefix, remainder = workspace.split(start, 1)
_, suffix = remainder.split(end, 1)

replacement = r'''fn copy_claim_to_unique_archive(
    claim: &FinalizingSourceClaim,
    folder: &Path,
    original_source: &Path,
) -> Result<PathBuf, String> {
    for _ in 0..=10_000u32 {
        let destination = unique_destination(folder, original_source, &claim.verified_sha256)?;
        let staging = folder.join(format!(
            ".dokkomplekt-archive-stage-{}.pending",
            Uuid::new_v4()
        ));
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось создать скрытый staging архива {}: {error}",
                    staging.display()
                ));
            }
        };

        let before_sha256 = match sha256_file(&claim.path) {
            Ok(hash) => hash,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if before_sha256 != claim.verified_sha256 {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err("Захваченный исходник изменился до архивирования.".into());
        }

        let mut input = match fs::File::open(&claim.path) {
            Ok(file) => file,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Не удалось открыть захваченный исходник для архивирования: {error}"
                ));
            }
        };
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось скопировать захваченный исходник в staging архива: {error}"));
        }
        if let Err(error) = output.sync_all() {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось синхронизировать staging архива: {error}"));
        }
        drop(output);

        let after_sha256 = match sha256_file(&claim.path) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let staged_sha256 = match sha256_file(&staging) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if after_sha256 != claim.verified_sha256 || staged_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&staging);
            return Err("Контрольная сумма изменилась во время безопасного архивирования.".into());
        }

        match fs::hard_link(&staging, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging);
                continue;
            }
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Файловая система не позволила атомарно опубликовать архив {}: {error}",
                    destination.display()
                ));
            }
        }
        if let Err(error) = fs::remove_file(&staging) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Архив опубликован, но staging {} не удалён; публикация отменена: {error}",
                staging.display()
            ));
        }
        let published_sha256 = match sha256_file(&destination) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
        };
        if published_sha256 != claim.verified_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Опубликованный архив не совпадает с проверенным SHA-256 исходника.".into());
        }
        return Ok(destination);
    }
    Err("Не удалось подобрать уникальное имя для архивного источника.".into())
}

'''
workspace = prefix + replacement + end + suffix

recover_start = "fn recover_finalizing_claim(claim: &Path) -> Result<PathBuf, String> {\n"
recover_end = "fn with_recovery_detail(message: String, recovery: Result<PathBuf, String>) -> String {\n"
if workspace.count(recover_start) != 1 or workspace.count(recover_end) != 1:
    raise SystemExit("recovery publication function markers are not unique")
prefix, remainder = workspace.split(recover_start, 1)
_, suffix = remainder.split(recover_end, 1)

recover = r'''fn recover_finalizing_claim(claim: &Path) -> Result<PathBuf, String> {
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
        let staging = parent.join(format!(
            ".dokkomplekt-recovery-stage-{}.pending",
            Uuid::new_v4()
        ));
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Не удалось создать скрытый staging recovery {}: {error}",
                    staging.display()
                ));
            }
        };

        let before_sha256 = match sha256_file(claim) {
            Ok(hash) => hash,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let mut input = match fs::File::open(claim) {
            Ok(file) => file,
            Err(error) => {
                drop(output);
                let _ = fs::remove_file(&staging);
                return Err(format!("Не удалось открыть finalization claim для recovery: {error}"));
            }
        };
        if let Err(error) = std::io::copy(&mut input, &mut output) {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось сохранить recovery staging: {error}"));
        }
        if let Err(error) = output.sync_all() {
            drop(output);
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось синхронизировать recovery staging: {error}"));
        }
        drop(output);

        let after_sha256 = match sha256_file(claim) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        let staged_sha256 = match sha256_file(&staging) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(error);
            }
        };
        if before_sha256 != after_sha256 || before_sha256 != staged_sha256 {
            let _ = fs::remove_file(&staging);
            return Err("Finalization claim изменился во время recovery; исходник оставлен нетронутым.".into());
        }

        match fs::hard_link(&staging, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging);
                continue;
            }
            Err(error) => {
                let _ = fs::remove_file(&staging);
                return Err(format!(
                    "Файловая система не позволила атомарно опубликовать recovery {}: {error}",
                    destination.display()
                ));
            }
        }
        if let Err(error) = fs::remove_file(&staging) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Recovery опубликован, но staging {} не удалён; публикация отменена: {error}",
                staging.display()
            ));
        }
        let recovered_sha256 = match sha256_file(&destination) {
            Ok(hash) => hash,
            Err(error) => {
                let _ = fs::remove_file(&destination);
                return Err(error);
            }
        };
        if recovered_sha256 != before_sha256 {
            let _ = fs::remove_file(&destination);
            return Err("Опубликованный recovery-файл не совпадает с finalization claim.".into());
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

'''
workspace = prefix + recover + recover_end + suffix

# Strengthen the source-level contract: final visible archive/recovery names must
# only appear after a hidden staging copy has been fully hashed.
test_path = Path("tests/test_v18_4_7_source_finalization_identity.py")
test_text = test_path.read_text(encoding="utf-8")
needle = '''    assert "create_new(true)" in text\n'''
replacement_contract = '''    assert "create_new(true)" in text\n    assert ".dokkomplekt-archive-stage-" in text\n    assert "fs::hard_link(&staging, &destination)" in text\n    assert text.index("staged_sha256") < text.index("fs::hard_link(&staging, &destination)")\n'''
if test_text.count(needle) != 1:
    raise SystemExit("archive contract marker mismatch")
test_text = test_text.replace(needle, replacement_contract, 1)
needle2 = '''    assert "recover_finalizing_claim(&path)" in hygiene\n'''
replacement2 = '''    assert "recover_finalizing_claim(&path)" in hygiene\n    assert ".dokkomplekt-recovery-stage-" in hygiene\n'''
if test_text.count(needle2) != 1:
    raise SystemExit("recovery contract marker mismatch")
test_path.write_text(test_text.replace(needle2, replacement2, 1), encoding="utf-8")

workspace_path.write_text(workspace, encoding="utf-8")
