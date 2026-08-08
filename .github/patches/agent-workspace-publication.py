from pathlib import Path

path = Path("src-tauri/src/workspace_hygiene.rs")
text = path.read_text(encoding="utf-8")

text = text.replace(
    'const RECOVERED_SOURCE_PREFIX: &str = "ВОССТАНОВЛЕННЫЙ ИСХОДНИК";\n',
    'const RECOVERED_SOURCE_PREFIX: &str = "ВОССТАНОВЛЕННЫЙ ИСХОДНИК";\n'
    'const ARCHIVE_STAGE_PREFIX: &str = ".dokkomplekt-archive-stage-";\n'
    'const RECOVERY_STAGE_PREFIX: &str = ".dokkomplekt-recovery-stage-";\n'
    'const RECEIPT_STAGE_PREFIX: &str = ".dokkomplekt-receipt-stage-";\n',
    1,
)

text = text.replace(
    "    pub recovered_finalizing_sources: Vec<String>,\n    pub warnings: Vec<String>,\n",
    "    pub recovered_finalizing_sources: Vec<String>,\n"
    "    pub removed_stale_staging_files: Vec<String>,\n"
    "    pub warnings: Vec<String>,\n",
    1,
)

old_top = '''        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {\n            continue;\n        };\n        if is_finalizing_claim_name(name) {\n'''
new_top = '''        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {\n            continue;\n        };\n        if is_workspace_staging_name(name) {\n            if file_age(&path, now).is_some_and(|age| age >= FINALIZING_CLAIM_GRACE) {\n                match fs::remove_file(&path) {\n                    Ok(()) => report\n                        .removed_stale_staging_files\n                        .push(path.display().to_string()),\n                    Err(error) => report.warnings.push(format!(\n                        "Не удалось удалить stale workspace staging {}: {error}",\n                        path.display()\n                    )),\n                }\n            }\n            continue;\n        }\n        if is_finalizing_claim_name(name) {\n'''
if text.count(old_top) != 1:
    raise SystemExit(f"top-level staging insertion marker mismatch: {text.count(old_top)}")
text = text.replace(old_top, new_top, 1)

old_retention = '''    if policy.archived_source_retention_days > 0 && archive_root.exists() {\n        let retention =\n            Duration::from_secs(u64::from(policy.archived_source_retention_days) * 86_400);\n        match ensure_real_directory_below(folder, &archive_root) {\n            Ok(archive_root_canonical) => cleanup_expired_archive_files(\n                &archive_root,\n                &archive_root_canonical,\n                now,\n                retention,\n                &mut report,\n            )?,\n            Err(error) => report.warnings.push(error),\n        }\n    }\n'''
new_retention = '''    if archive_root.exists() {\n        let retention = (policy.archived_source_retention_days > 0).then(|| {\n            Duration::from_secs(u64::from(policy.archived_source_retention_days) * 86_400)\n        });\n        match ensure_real_directory_below(folder, &archive_root) {\n            Ok(archive_root_canonical) => cleanup_expired_archive_files(\n                &archive_root,\n                &archive_root_canonical,\n                now,\n                retention,\n                &mut report,\n            )?,\n            Err(error) => report.warnings.push(error),\n        }\n    }\n'''
if text.count(old_retention) != 1:
    raise SystemExit("archive retention block marker mismatch")
text = text.replace(old_retention, new_retention, 1)

text = text.replace(
    '''        let staging = folder.join(format!(\n            ".dokkomplekt-archive-stage-{}.pending",\n            Uuid::new_v4()\n        ));\n''',
    '''        let staging = staging_path(folder, ARCHIVE_STAGE_PREFIX);\n''',
    1,
)
text = text.replace(
    '''        let staging = parent.join(format!(\n            ".dokkomplekt-recovery-stage-{}.pending",\n            Uuid::new_v4()\n        ));\n''',
    '''        let staging = staging_path(parent, RECOVERY_STAGE_PREFIX);\n''',
    1,
)

move_start = "fn move_to_unique_folder(source: &Path, folder: &Path) -> Result<PathBuf, String> {\n"
receipt_start = "fn write_receipt(\n"
if text.count(move_start) != 1 or text.count(receipt_start) != 1:
    raise SystemExit("move/receipt markers are not unique")
prefix, rest = text.split(move_start, 1)
_, suffix = rest.split(receipt_start, 1)
new_move = r'''fn move_to_unique_folder(source: &Path, folder: &Path) -> Result<PathBuf, String> {
    // Service files use the same identity-safe claim protocol as processed sources.
    // A replacement that reuses `source` after the initial hash is recovered rather
    // than deleted, and the visible archive destination is create-if-absent.
    let hash = sha256_file(source)?;
    let claim = claim_matching_source(source, &hash)?;
    let destination = match copy_claim_to_unique_archive(&claim, folder, source) {
        Ok(path) => path,
        Err(error) => {
            let recovery = recover_finalizing_claim(&claim.path);
            return Err(with_recovery_detail(error, recovery));
        }
    };
    if let Err(error) = fs::remove_file(&claim.path) {
        let _ = fs::remove_file(&destination);
        let recovery = recover_finalizing_claim(&claim.path);
        return Err(with_recovery_detail(
            format!(
                "Архивная копия служебного файла подготовлена, но захваченный источник {} не удалён: {error}",
                claim.path.display()
            ),
            recovery,
        ));
    }
    Ok(destination)
}

'''
text = prefix + new_move + receipt_start + suffix

# Replace write_receipt through marker_sha256, preserving marker_sha256 itself.
write_start = "fn write_receipt(\n"
write_end = "fn marker_sha256(marker: &Path) -> Option<String> {\n"
if text.count(write_start) != 1 or text.count(write_end) != 1:
    raise SystemExit("receipt/marker markers are not unique")
prefix, rest = text.split(write_start, 1)
_, suffix = rest.split(write_end, 1)
new_write = r'''fn write_receipt(
    receipt: &Path,
    original: &Path,
    archived: &Path,
    source_sha256: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "schema": 1,
        "original_name": original.file_name().and_then(|value| value.to_str()).unwrap_or("source"),
        "archived_name": archived.file_name().and_then(|value| value.to_str()).unwrap_or("source"),
        "sha256": source_sha256,
        "archived_at_unix": OffsetDateTime::now_utc().unix_timestamp(),
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?;
    publish_bytes_create_new(receipt, &bytes, RECEIPT_STAGE_PREFIX)
}

fn publish_bytes_create_new(
    destination: &Path,
    bytes: &[u8],
    stage_prefix: &str,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "У публикуемого файла нет родительской папки.".to_string())?;
    let staging = staging_path(parent, stage_prefix);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|error| format!("Не удалось создать скрытый staging {}: {error}", staging.display()))?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(&staging);
        return Err(format!("Не удалось записать скрытый staging: {error}"));
    }
    if let Err(error) = file.sync_all() {
        drop(file);
        let _ = fs::remove_file(&staging);
        return Err(format!("Не удалось синхронизировать скрытый staging: {error}"));
    }
    drop(file);

    let staged = match fs::read(&staging) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&staging);
            return Err(format!("Не удалось проверить скрытый staging: {error}"));
        }
    };
    if staged != bytes {
        let _ = fs::remove_file(&staging);
        return Err("Содержимое staging изменилось до публикации.".into());
    }

    match fs::hard_link(&staging, destination) {
        Ok(()) => {}
        Err(error) => {
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Не удалось атомарно опубликовать {} без перезаписи существующего файла: {error}",
                destination.display()
            ));
        }
    }
    if let Err(error) = fs::remove_file(&staging) {
        let _ = fs::remove_file(destination);
        return Err(format!(
            "Файл опубликован, но staging {} не удалён; публикация отменена: {error}",
            staging.display()
        ));
    }
    let published = match fs::read(destination) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(destination);
            return Err(format!("Не удалось проверить опубликованный файл: {error}"));
        }
    };
    if published != bytes {
        let _ = fs::remove_file(destination);
        return Err("Опубликованный файл не совпадает с проверенным staging.".into());
    }
    Ok(())
}

fn staging_path(folder: &Path, prefix: &str) -> PathBuf {
    folder.join(format!("{prefix}{}{FINALIZING_SUFFIX}", Uuid::new_v4()))
}

fn is_workspace_staging_name(name: &str) -> bool {
    name.ends_with(FINALIZING_SUFFIX)
        && [ARCHIVE_STAGE_PREFIX, RECOVERY_STAGE_PREFIX, RECEIPT_STAGE_PREFIX]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

'''
text = prefix + new_write + write_end + suffix

# Make recursive archive traversal always clean stale hidden staging while making
# ordinary archived-file retention optional (0 still means indefinite retention).
text = text.replace(
    "    retention: Duration,\n    report: &mut WorkspaceHygieneReport,\n) -> Result<(), String> {\n",
    "    retention: Option<Duration>,\n    report: &mut WorkspaceHygieneReport,\n) -> Result<(), String> {\n",
    1,
)

old_file_cleanup = '''        if metadata.is_file() && file_age(&canonical, now).is_some_and(|age| age >= retention) {\n            match fs::remove_file(&canonical) {\n                Ok(()) => report\n                    .removed_expired_archived_files\n                    .push(canonical.display().to_string()),\n                Err(error) => report.warnings.push(format!(\n                    "Не удалось удалить архивный файл {}: {error}",\n                    canonical.display()\n                )),\n            }\n        }\n'''
new_file_cleanup = '''        if metadata.is_file() {\n            let name = canonical\n                .file_name()\n                .and_then(|value| value.to_str())\n                .unwrap_or("");\n            if is_workspace_staging_name(name)\n                && file_age(&canonical, now)\n                    .is_some_and(|age| age >= FINALIZING_CLAIM_GRACE)\n            {\n                match fs::remove_file(&canonical) {\n                    Ok(()) => report\n                        .removed_stale_staging_files\n                        .push(canonical.display().to_string()),\n                    Err(error) => report.warnings.push(format!(\n                        "Не удалось удалить stale archive staging {}: {error}",\n                        canonical.display()\n                    )),\n                }\n                continue;\n            }\n            if retention.is_some_and(|retention| {\n                file_age(&canonical, now).is_some_and(|age| age >= retention)\n            }) {\n                match fs::remove_file(&canonical) {\n                    Ok(()) => report\n                        .removed_expired_archived_files\n                        .push(canonical.display().to_string()),\n                    Err(error) => report.warnings.push(format!(\n                        "Не удалось удалить архивный файл {}: {error}",\n                        canonical.display()\n                    )),\n                }\n            }\n        }\n'''
if text.count(old_file_cleanup) != 1:
    raise SystemExit("recursive cleanup marker mismatch")
text = text.replace(old_file_cleanup, new_file_cleanup, 1)

# Add focused Rust regressions before the closing test-module brace.
test_marker = '''    #[test]\n    fn archive_receipt_uses_verified_archived_sha() {\n'''
if text.count(test_marker) != 1:
    raise SystemExit("test insertion marker mismatch")
new_tests = r'''    #[test]
    fn receipt_publication_never_overwrites_existing_final_file() {
        let root = temp_root("receipt-no-overwrite");
        let receipt = root.join("receipt.json");
        let original = root.join("case.docx");
        let archived = root.join("archived.docx");
        fs::write(&receipt, b"existing-receipt").unwrap();
        let error = write_receipt(&receipt, &original, &archived, &hash_bytes(b"document"))
            .unwrap_err();
        assert!(error.contains("без перезаписи"));
        assert_eq!(fs::read(&receipt).unwrap(), b"existing-receipt");
        assert!(!fs::read_dir(&root).unwrap().filter_map(Result::ok).any(|entry| {
            entry.file_name().to_string_lossy().starts_with(RECEIPT_STAGE_PREFIX)
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn receipt_publication_is_complete_and_leaves_no_staging_file() {
        let root = temp_root("receipt-complete");
        let receipt = root.join("receipt.json");
        let original = root.join("case.docx");
        let archived = root.join("archived.docx");
        write_receipt(&receipt, &original, &archived, &hash_bytes(b"document")).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        assert_eq!(parsed["schema"], 1);
        assert!(!fs::read_dir(&root).unwrap().filter_map(Result::ok).any(|entry| {
            entry.file_name().to_string_lossy().starts_with(RECEIPT_STAGE_PREFIX)
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_archive_preserves_existing_destination_without_overwrite() {
        let root = temp_root("service-no-overwrite");
        let source = root.join("case_ТРЕБУЕТ_ВНИМАНИЯ.txt");
        fs::write(&source, b"new-service-note").unwrap();
        let folder = root.join("_обработано").join("_служебные").join(current_month());
        fs::create_dir_all(&folder).unwrap();
        let existing = folder.join(source.file_name().unwrap());
        fs::write(&existing, b"existing-service-note").unwrap();

        let archived = move_to_unique_folder(&source, &folder).unwrap();
        assert_eq!(fs::read(&existing).unwrap(), b"existing-service-note");
        assert_eq!(fs::read(&archived).unwrap(), b"new-service-note");
        assert_ne!(archived, existing);
        assert!(!source.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_nested_staging_is_removed_even_with_indefinite_archive_retention() {
        let root = temp_root("stale-stage-indefinite");
        let archive = root.join("_обработано").join("2026-08");
        fs::create_dir_all(&archive).unwrap();
        let staging = archive.join(format!(
            "{RECEIPT_STAGE_PREFIX}old{FINALIZING_SUFFIX}"
        ));
        let retained = archive.join("keep.docx");
        fs::write(&staging, b"stale").unwrap();
        fs::write(&retained, b"keep").unwrap();
        let mut policy = WorkspaceRetentionPolicy::default();
        policy.archived_source_retention_days = 0;
        let now = UNIX_EPOCH + Duration::from_secs(4_000_000_000);

        let report = cleanup_workspace_folder(&root, &policy, now).unwrap();
        assert!(!staging.exists());
        assert!(retained.exists());
        assert_eq!(report.removed_stale_staging_files.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

'''
text = text.replace(test_marker, new_tests + test_marker, 1)

path.write_text(text, encoding="utf-8")

# Extend crash-consistency documentation.
doc = Path("docs/CRASH_CONSISTENCY.md")
doc_text = doc.read_text(encoding="utf-8")
append = r'''

## Workspace archive and receipt publication

Workspace housekeeping follows the same no-partial-final rule as generated documents. Service-note moves first claim the source identity and then publish a verified archive copy with create-if-absent semantics. Archive receipts are written to hidden same-directory staging files, flushed and byte-verified before a visible `.dokkomplekt-receipt.json` name is created. Existing destinations are never overwritten. Crash-left hidden staging files are disposable copies and are removed after the finalization grace period, including when normal archive retention is configured as indefinite.
'''
if "## Workspace archive and receipt publication" not in doc_text:
    doc.write_text(doc_text.rstrip() + append + "\n", encoding="utf-8")

# Source-level regression contract.
contract = Path("tests/test_v18_4_8_workspace_publication.py")
contract.write_text(r'''from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HYGIENE = ROOT / "src-tauri" / "src" / "workspace_hygiene.rs"


def _section(text: str, start: str, end: str) -> str:
    return text.split(start, 1)[1].split(end, 1)[0]


def test_service_file_move_reuses_identity_safe_claim_and_atomic_archive_publish():
    text = HYGIENE.read_text(encoding="utf-8")
    move = _section(text, "fn move_to_unique_folder(", "fn write_receipt(")
    assert "claim_matching_source(source, &hash)" in move
    assert "copy_claim_to_unique_archive(&claim, folder, source)" in move
    assert "fn move_file_safely" not in text
    assert "fs::copy(source, destination)" not in text
    assert "fs::rename(source, destination)" not in text


def test_receipt_is_hidden_until_complete_and_never_overwrites_final_name():
    text = HYGIENE.read_text(encoding="utf-8")
    publisher = _section(text, "fn publish_bytes_create_new(", "fn staging_path(")
    assert "RECEIPT_STAGE_PREFIX" in text
    assert ".create_new(true)" in publisher
    assert "file.sync_all()" in publisher
    assert "staged != bytes" in publisher
    assert "fs::hard_link(&staging, destination)" in publisher
    assert publisher.index("staged != bytes") < publisher.index("fs::hard_link(&staging, destination)")
    assert "published != bytes" in publisher


def test_stale_hidden_staging_is_cleaned_independently_of_archive_retention():
    text = HYGIENE.read_text(encoding="utf-8")
    assert "removed_stale_staging_files" in text
    assert "is_workspace_staging_name(name)" in text
    assert "let retention = (policy.archived_source_retention_days > 0).then" in text
    recursive = _section(text, "fn cleanup_expired_archive_files(", "fn sha256_file(")
    assert "retention: Option<Duration>" in recursive
    assert "FINALIZING_CLAIM_GRACE" in recursive
    assert "removed_stale_staging_files" in recursive
''', encoding="utf-8")
