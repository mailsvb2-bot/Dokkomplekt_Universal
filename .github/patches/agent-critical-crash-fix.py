from pathlib import Path


def replace_between(text: str, start: str, end: str, replacement: str) -> str:
    start_index = text.index(start)
    end_index = text.index(end, start_index)
    return text[:start_index] + replacement + text[end_index:]


storage_path = Path("crates/dokkomplekt-storage/src/lib.rs")
storage = storage_path.read_text(encoding="utf-8")

storage = replace_between(
    storage,
    "    pub fn rollback_usage(&mut self, reservation: &UsageReservation) -> StorageResult<bool> {",
    "    /// Rolls back reservations left by a process that could not complete.",
    '''    pub fn rollback_usage(&mut self, reservation: &UsageReservation) -> StorageResult<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // The SQLite reservation row is authoritative. Never trust the caller's
        // month/count/trial fields when refunding usage: a stale or malformed
        // in-memory object must not be able to decrement unrelated accounting.
        let persisted: Option<(String, i64, i64, String)> = tx
            .query_row(
                "SELECT month_key,documents,trial,status FROM usage_reservations WHERE reservation_id=?1",
                params![reservation.reservation_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((month_key, documents, trial, status)) = persisted else {
            tx.commit()?;
            return Ok(false);
        };
        if status != "reserved" {
            tx.commit()?;
            return Ok(false);
        }
        if documents <= 0 {
            return Err(StorageError::Crypto(
                "persisted usage reservation has invalid document count".into(),
            ));
        }
        tx.execute(
            "UPDATE commercial_usage SET created_documents=MAX(0,created_documents-?2),trial_documents=MAX(0,trial_documents-?3),updated_at=CURRENT_TIMESTAMP WHERE month_key=?1",
            params![month_key, documents, if trial != 0 { documents } else { 0 }],
        )?;
        tx.execute(
            "UPDATE usage_reservations SET status='rolled_back',updated_at=CURRENT_TIMESTAMP WHERE reservation_id=?1 AND status='reserved'",
            params![reservation.reservation_id.as_str()],
        )?;
        tx.commit()?;
        Ok(true)
    }

''',
)

storage = replace_between(
    storage,
    "    /// Rolls back reservations left by a process that could not complete.",
    "    pub fn register_template_version(",
    '''    /// Finalizes old ambiguous reservations conservatively after a hard crash.
    ///
    /// Usage is incremented when a reservation is created. A process can die after
    /// publishing a complete document but before it flips the reservation to
    /// `committed`; automatically subtracting such a row later creates a quota-bypass
    /// window. Explicit, observed generation failures still call `rollback_usage` and
    /// are refunded. Ambiguous crash leftovers are therefore finalized without a refund.
    pub fn recover_stale_usage_reservations(
        &mut self,
        max_age_minutes: u32,
    ) -> StorageResult<usize> {
        let modifier = format!("-{} minutes", max_age_minutes.max(60));
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE usage_reservations SET status='committed_after_crash',updated_at=CURRENT_TIMESTAMP WHERE status='reserved' AND created_at <= datetime('now', ?1)",
            params![modifier],
        )?;
        tx.commit()?;
        Ok(changed)
    }

''',
)

storage = replace_between(
    storage,
    "    #[test]\n    fn stale_usage_reservations_are_rolled_back_atomically() {",
    "    #[test]\n    fn exception_resolution_preserves_original_encrypted_details() {",
    '''    #[test]
    fn stale_usage_reservations_are_finalized_without_ambiguous_refund() {
        let path = temp_db("usage-stale");
        let mut repo = LocalRepository::open(&path).unwrap();
        let reservation = repo.reserve_usage("2026-07", 2, true, 30, 30).unwrap();
        repo.conn
            .execute(
                "UPDATE usage_reservations SET created_at=datetime('now','-2 hours') WHERE reservation_id=?1",
                params![reservation.reservation_id.as_str()],
            )
            .unwrap();
        assert_eq!(repo.recover_stale_usage_reservations(30).unwrap(), 1);
        assert_eq!(repo.usage_snapshot("2026-07").unwrap().created_documents, 2);
        let status: String = repo
            .conn
            .query_row(
                "SELECT status FROM usage_reservations WHERE reservation_id=?1",
                params![reservation.reservation_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "committed_after_crash");
        assert!(!repo.rollback_usage(&reservation).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn usage_rollback_uses_persisted_reservation_fields_as_source_of_truth() {
        let path = temp_db("usage-persisted-source-of-truth");
        let mut repo = LocalRepository::open(&path).unwrap();
        let reservation = repo.reserve_usage("2026-07", 3, true, 30, 30).unwrap();
        let forged = UsageReservation {
            reservation_id: reservation.reservation_id.clone(),
            month_key: "2099-12".into(),
            documents: 30,
            trial: false,
        };
        assert!(repo.rollback_usage(&forged).unwrap());
        let july = repo.usage_snapshot("2026-07").unwrap();
        assert_eq!(july.created_documents, 0);
        assert_eq!(july.trial_documents_total, 0);
        assert_eq!(repo.usage_snapshot("2099-12").unwrap().created_documents, 0);
        let _ = std::fs::remove_file(path);
    }

''',
)
storage_path.write_text(storage, encoding="utf-8")

main_path = Path("src-tauri/src/main.rs")
main = main_path.read_text(encoding="utf-8")
main = replace_between(
    main,
    "struct UniqueFileReservation {",
    "fn publish_stage_to_unique_directory(stage: &Path, desired: &Path) -> Result<PathBuf, String> {",
    '''struct UniqueFileReservation {
    /// Hidden staging file used by the renderer. It is never the user-visible name.
    path: PathBuf,
    desired_path: PathBuf,
    committed: bool,
}

impl UniqueFileReservation {
    fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        for _ in 0..128 {
            let staging = parent.join(format!(
                ".dokkomplekt-file-stage-{}-{}.tmp",
                std::process::id(),
                Uuid::new_v4()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)
            {
                Ok(_) => {
                    return Ok(Self {
                        path: staging,
                        desired_path: path.to_path_buf(),
                        committed: false,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Не удалось создать скрытый staging-файл результата: {error}"
                    ))
                }
            }
        }
        Err("Не удалось создать уникальный staging-файл результата.".into())
    }

    /// Atomically exposes a fully-rendered file under a unique final name.
    ///
    /// `hard_link` is used as an atomic create-if-absent primitive. If the
    /// destination filesystem cannot provide this guarantee, publication fails
    /// closed instead of leaving a partial/corrupt file under a final DOCX name.
    fn commit(mut self) -> Result<PathBuf, String> {
        for index in 1..=10_000 {
            let candidate = numbered_candidate(&self.desired_path, index);
            match std::fs::hard_link(&self.path, &candidate) {
                Ok(()) => {
                    self.committed = true;
                    let _ = std::fs::remove_file(&self.path);
                    return Ok(candidate);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "Файловая система не поддержала безопасную атомарную публикацию результата: {error}"
                    ))
                }
            }
        }
        Err("Не удалось подобрать уникальное имя после 10000 попыток.".into())
    }
}

impl Drop for UniqueFileReservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

''',
)

test_marker = "    #[test]\n    fn path_resolution_rejects_parent_traversal_components() {"
if test_marker not in main:
    raise SystemExit("main test insertion marker not found")
main = main.replace(
    test_marker,
    '''    #[test]
    fn unique_file_reservation_hides_incomplete_output_until_commit() {
        let root = std::env::temp_dir().join(format!(
            "dokkomplekt-unique-file-reservation-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let desired = root.join("result.docx");
        let reservation = super::UniqueFileReservation::acquire(&desired).unwrap();
        assert!(!desired.exists(), "final name must stay invisible while rendering");
        assert!(reservation
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with(".dokkomplekt-file-stage-")));
        std::fs::write(&reservation.path, b"complete-docx-bytes").unwrap();
        let published = reservation.commit().unwrap();
        assert_eq!(published, desired);
        assert_eq!(std::fs::read(&published).unwrap(), b"complete-docx-bytes");
        let _ = std::fs::remove_dir_all(root);
    }

''' + test_marker,
    1,
)
main_path.write_text(main, encoding="utf-8")

commands_path = Path("src-tauri/src/subsystems/document_commands.rs")
commands = commands_path.read_text(encoding="utf-8")
old_single = "    let output_path = reservation.commit();\n    if let Err(error) = commit_generation_access(&app, &permit) {"
new_single = '''    let output_path = match reservation.commit() {
        Ok(path) => path,
        Err(error) => {
            rollback_counter_reservations(&app, &hydrated.counter_reservations);
            rollback_generation_access(&app, &state, &permit);
            return Err(error);
        }
    };
    if let Err(error) = commit_generation_access(&app, &permit) {'''
if commands.count(old_single) != 1:
    raise SystemExit(f"unexpected single-render commit match count: {commands.count(old_single)}")
commands = commands.replace(old_single, new_single, 1)
old_batch = "            paths.push(reservation.commit());"
if commands.count(old_batch) != 1:
    raise SystemExit(f"unexpected batch commit match count: {commands.count(old_batch)}")
commands = commands.replace(old_batch, "            paths.push(reservation.commit()?);", 1)
commands_path.write_text(commands, encoding="utf-8")
