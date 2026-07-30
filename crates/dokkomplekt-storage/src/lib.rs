//! SQLite storage seam for local profiles, buttons and remembered values.
//! No telemetry, no network, no document upload.
//!
//! Sensitive semantic-case JSON can be encrypted at rest with a caller-supplied
//! 256-bit key. The key is deliberately not stored in SQLite. Existing plaintext
//! rows remain readable and are migrated to encrypted form on the next save.

use aes::Aes256;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use dokkomplekt_core::{CorpusEntry, DocumentPack, SemanticCase};
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClauseBlockRecord {
    pub block_id: String,
    pub title: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageSnapshot {
    pub month_key: String,
    pub created_documents: u32,
    pub trial_documents_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageReservation {
    pub reservation_id: String,
    pub month_key: String,
    pub documents: u32,
    pub trial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterValue {
    pub counter_key: String,
    pub year: i32,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AutomationExceptionRecord {
    pub exception_id: String,
    pub category: String,
    pub source_path: String,
    pub message: String,
    pub details_json: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TemplateVersionRecord {
    pub version_id: String,
    pub document_id: String,
    pub version_number: u32,
    pub template_path: String,
    pub template_sha256: String,
    pub note: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CaseRunRecord {
    pub case_id: String,
    pub source_sha256: String,
    pub processing_fingerprint: String,
    pub source_path: String,
    pub status: String,
    pub request_json: String,
    pub output_root: String,
    pub patient_folder: Option<String>,
    pub created_files_json: String,
    pub missing_json: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

type CaseRunRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    String,
    String,
);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CaseDocumentRecord {
    pub case_id: String,
    pub document_id: String,
    pub input_fingerprint: String,
    pub output_path: String,
    pub output_sha256: String,
    pub output_size_bytes: u64,
    pub status: String,
    pub reused_from_case_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditEventRecord {
    pub event_id: String,
    pub event_type: String,
    pub object_hash: String,
    pub detail_json: String,
    pub previous_hash: String,
    pub event_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AutomationMetrics {
    pub processed_sources: u64,
    pub generated_documents: u64,
    pub blocked_sources: u64,
    pub failed_sources: u64,
    pub print_failures: u64,
    pub user_confirmations: u64,
    pub zero_touch_sources: u64,
    pub attention_resolutions: u64,
    pub model_grounding_rejections: u64,
    pub shadow_model_runs: u64,
    pub shadow_model_proposals: u64,
    pub shadow_model_agreements: u64,
    pub reused_documents: u64,
    pub rerendered_documents: u64,
    /// Measured wall-clock time spent in successfully published intake runs.
    pub processing_milliseconds: u64,
    /// Number of generated sets placed into the mandatory print-review queue.
    pub print_review_queued: u64,
    /// Number of generated sets that passed the automatic-print safety gate.
    pub automatic_print_approved: u64,
}
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;
const ENCRYPTED_PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 16;
const TAG_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("encrypted local data requires the configured privacy key")]
    EncryptionRequired,
    #[error("invalid encrypted local data: {0}")]
    Crypto(String),
}

pub type StorageResult<T> = Result<T, StorageError>;

pub struct LocalRepository {
    conn: Connection,
    sensitive_key: Option<[u8; 32]>,
}

impl LocalRepository {
    /// Opens a backwards-compatible plaintext repository.
    ///
    /// Deployments that store confidential professional data should use
    /// [`LocalRepository::open_with_key`] instead.
    pub fn open(path: &Path) -> StorageResult<Self> {
        Self::open_internal(path, None)
    }

    /// Opens a repository whose semantic-case payloads are authenticated and
    /// encrypted at rest. The 32-byte key is owned by the caller and is never
    /// written to the database.
    pub fn open_with_key(path: &Path, key: [u8; 32]) -> StorageResult<Self> {
        Self::open_internal(path, Some(key))
    }

    fn open_internal(path: &Path, sensitive_key: Option<[u8; 32]>) -> StorageResult<Self> {
        let conn = Connection::open(path)?;
        let repo = Self {
            conn,
            sensitive_key,
        };
        repo.init()?;
        Ok(repo)
    }

    pub fn init(&self) -> StorageResult<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS document_packs (
              pack_id TEXT PRIMARY KEY,
              json TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS semantic_cases (
              case_id TEXT PRIMARY KEY,
              json TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS corpus_entries (
              entry_id TEXT PRIMARY KEY,
              case_id TEXT NOT NULL,
              source_sha256 TEXT NOT NULL,
              domain_json TEXT NOT NULL,
              json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_corpus_entries_domain
              ON corpus_entries(domain_json, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_corpus_entries_source
              ON corpus_entries(source_sha256, created_at DESC);
            CREATE TABLE IF NOT EXISTS app_state (
              state_key TEXT PRIMARY KEY,
              json TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS clause_blocks (
              block_id TEXT PRIMARY KEY,
              title TEXT NOT NULL,
              content TEXT NOT NULL,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS document_counters (
              counter_key TEXT NOT NULL,
              counter_year INTEGER NOT NULL,
              value INTEGER NOT NULL,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              PRIMARY KEY(counter_key, counter_year)
            );
            CREATE TABLE IF NOT EXISTS commercial_usage (
              month_key TEXT PRIMARY KEY,
              created_documents INTEGER NOT NULL DEFAULT 0,
              trial_documents INTEGER NOT NULL DEFAULT 0,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS usage_reservations (
              reservation_id TEXT PRIMARY KEY,
              month_key TEXT NOT NULL,
              documents INTEGER NOT NULL,
              trial INTEGER NOT NULL,
              status TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS template_versions (
              version_id TEXT PRIMARY KEY,
              document_id TEXT NOT NULL,
              version_number INTEGER NOT NULL,
              template_path TEXT NOT NULL,
              template_sha256 TEXT NOT NULL,
              note TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              UNIQUE(document_id, version_number)
            );
            CREATE INDEX IF NOT EXISTS idx_template_versions_document
              ON template_versions(document_id, version_number DESC);
            CREATE TABLE IF NOT EXISTS case_runs (
              case_id TEXT PRIMARY KEY,
              source_sha256 TEXT NOT NULL,
              processing_fingerprint TEXT NOT NULL DEFAULT '',
              source_path TEXT NOT NULL,
              status TEXT NOT NULL,
              request_json TEXT NOT NULL,
              output_root TEXT NOT NULL,
              patient_folder TEXT,
              created_files_json TEXT NOT NULL,
              missing_json TEXT NOT NULL,
              last_error TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_case_runs_status
              ON case_runs(status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_case_runs_source_hash
              ON case_runs(source_sha256, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_case_runs_source_plan
              ON case_runs(source_sha256, processing_fingerprint, status);
            CREATE TABLE IF NOT EXISTS case_run_documents (
              case_id TEXT NOT NULL,
              document_id TEXT NOT NULL,
              input_fingerprint TEXT NOT NULL,
              output_path TEXT NOT NULL,
              output_sha256 TEXT NOT NULL DEFAULT '',
              output_size_bytes INTEGER NOT NULL DEFAULT 0,
              status TEXT NOT NULL,
              reused_from_case_id TEXT,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              PRIMARY KEY(case_id, document_id),
              FOREIGN KEY(case_id) REFERENCES case_runs(case_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_case_run_documents_fingerprint
              ON case_run_documents(document_id, input_fingerprint, status);
            CREATE TABLE IF NOT EXISTS automation_exceptions (
              exception_id TEXT PRIMARY KEY,
              category TEXT NOT NULL,
              source_path TEXT NOT NULL,
              message TEXT NOT NULL,
              details_json TEXT NOT NULL,
              status TEXT NOT NULL DEFAULT 'open',
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_automation_exceptions_status
              ON automation_exceptions(status, created_at DESC);
            CREATE TABLE IF NOT EXISTS audit_events (
              event_id TEXT PRIMARY KEY,
              event_type TEXT NOT NULL,
              object_hash TEXT NOT NULL,
              detail_json TEXT NOT NULL,
              previous_hash TEXT NOT NULL,
              event_hash TEXT NOT NULL UNIQUE,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS automation_metrics (
              metric_key TEXT PRIMARY KEY,
              value INTEGER NOT NULL DEFAULT 0,
              updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
        "#,
        )?;
        self.ensure_column(
            "case_run_documents",
            "output_sha256",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column(
            "case_run_documents",
            "output_size_bytes",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column(
            "case_runs",
            "processing_fingerprint",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> StorageResult<()> {
        let mut statement = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == column) {
            self.conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {definition}"
            ))?;
        }
        Ok(())
    }

    pub fn save_pack(&self, pack: &DocumentPack) -> StorageResult<()> {
        let json = serde_json::to_string_pretty(pack)?;
        self.conn.execute(
            "INSERT INTO document_packs(pack_id, json) VALUES (?1, ?2) ON CONFLICT(pack_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![pack.pack_id.as_str(), json],
        )?;
        Ok(())
    }

    pub fn load_pack(&self, pack_id: &str) -> StorageResult<Option<DocumentPack>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM document_packs WHERE pack_id=?1")?;
        let mut rows = stmt.query(params![pack_id])?;
        if let Some(row) = rows.next()? {
            let stored: String = row.get(0)?;
            let json = self.decode_sensitive(&stored)?;
            Ok(Some(serde_json::from_str(&json)?))
        } else {
            Ok(None)
        }
    }

    pub fn save_case(&self, case_id: &str, case: &SemanticCase) -> StorageResult<()> {
        let json = serde_json::to_string_pretty(case)?;
        let stored = self.encode_sensitive(&json)?;
        self.conn.execute(
            "INSERT INTO semantic_cases(case_id, json) VALUES (?1, ?2) ON CONFLICT(case_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![case_id, stored],
        )?;
        Ok(())
    }

    pub fn append_corpus_entry(&self, entry: &CorpusEntry) -> StorageResult<()> {
        let json = serde_json::to_string(entry)?;
        let domain_json = serde_json::to_string(&entry.domain)?;
        self.conn.execute(
            "INSERT INTO corpus_entries(entry_id,case_id,source_sha256,domain_json,json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                entry.entry_id.as_str(),
                entry.case_id.as_str(),
                entry.source_sha256.as_str(),
                domain_json,
                self.encode_sensitive(&json)?,
                entry.created_at.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn list_corpus_entries(&self, limit: usize) -> StorageResult<Vec<CorpusEntry>> {
        let limit = limit.clamp(1, 10_000) as i64;
        let mut statement = self.conn.prepare(
            "SELECT json FROM corpus_entries ORDER BY created_at DESC, rowid DESC LIMIT ?1",
        )?;
        let raw = statement
            .query_map(params![limit], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|stored| {
                let json = self.decode_sensitive(&stored)?;
                serde_json::from_str(&json).map_err(StorageError::from)
            })
            .collect()
    }

    pub fn corpus_entry_count(&self) -> StorageResult<u64> {
        let count = self
            .conn
            .query_row("SELECT COUNT(*) FROM corpus_entries", [], |row| {
                row.get::<_, i64>(0)
            })?;
        Ok(count.max(0) as u64)
    }

    pub fn delete_case(&self, case_id: &str) -> StorageResult<()> {
        self.conn.execute(
            "DELETE FROM semantic_cases WHERE case_id=?1",
            params![case_id],
        )?;
        Ok(())
    }

    pub fn load_case(&self, case_id: &str) -> StorageResult<Option<SemanticCase>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM semantic_cases WHERE case_id=?1")?;
        let mut rows = stmt.query(params![case_id])?;
        if let Some(row) = rows.next()? {
            let stored: String = row.get(0)?;
            let json = self.decode_sensitive(&stored)?;
            Ok(Some(serde_json::from_str(&json)?))
        } else {
            Ok(None)
        }
    }

    pub fn save_state_value<T: serde::Serialize + ?Sized>(
        &self,
        key: &str,
        value: &T,
    ) -> StorageResult<()> {
        let json = serde_json::to_string(value)?;
        let stored = self.encode_sensitive(&json)?;
        self.conn.execute(
            "INSERT INTO app_state(state_key, json) VALUES (?1, ?2) ON CONFLICT(state_key) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![key, stored],
        )?;
        Ok(())
    }

    pub fn load_state_value<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> StorageResult<Option<T>> {
        let mut stmt = self
            .conn
            .prepare("SELECT json FROM app_state WHERE state_key=?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let stored: String = row.get(0)?;
            let json = self.decode_sensitive(&stored)?;
            Ok(Some(serde_json::from_str(&json)?))
        } else {
            Ok(None)
        }
    }

    /// Atomically persists the user case and document pack. No caller can observe
    /// a new case paired with an old pack after a crash or power loss.
    pub fn save_case_and_pack_atomic(
        &self,
        case_id: &str,
        pack_id: &str,
        case: &SemanticCase,
        pack: &DocumentPack,
    ) -> StorageResult<()> {
        let case_json = serde_json::to_string_pretty(case)?;
        let case_stored = self.encode_sensitive(&case_json)?;
        let pack_json = serde_json::to_string_pretty(pack)?;
        let pack_stored = self.encode_sensitive(&pack_json)?;
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO semantic_cases(case_id, json) VALUES (?1, ?2) ON CONFLICT(case_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![case_id, case_stored],
        )?;
        transaction.execute(
            "INSERT INTO document_packs(pack_id, json) VALUES (?1, ?2) ON CONFLICT(pack_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![pack_id, pack_stored],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Atomically persists the complete desktop state, including the commercial
    /// state value. All serialization/encryption is completed before SQLite is
    /// locked so a fallible conversion can never leave a partial snapshot.
    pub fn save_desktop_snapshot<T: serde::Serialize + ?Sized>(
        &self,
        case_id: &str,
        pack_id: &str,
        case: &SemanticCase,
        pack: &DocumentPack,
        state_key: &str,
        state_value: &T,
    ) -> StorageResult<()> {
        let case_json = serde_json::to_string_pretty(case)?;
        let case_stored = self.encode_sensitive(&case_json)?;
        let pack_json = serde_json::to_string_pretty(pack)?;
        let pack_stored = self.encode_sensitive(&pack_json)?;
        let state_json = serde_json::to_string(state_value)?;
        let state_stored = self.encode_sensitive(&state_json)?;
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO semantic_cases(case_id, json) VALUES (?1, ?2) ON CONFLICT(case_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![case_id, case_stored],
        )?;
        transaction.execute(
            "INSERT INTO document_packs(pack_id, json) VALUES (?1, ?2) ON CONFLICT(pack_id) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![pack_id, pack_stored],
        )?;
        transaction.execute(
            "INSERT INTO app_state(state_key, json) VALUES (?1, ?2) ON CONFLICT(state_key) DO UPDATE SET json=excluded.json, updated_at=CURRENT_TIMESTAMP",
            params![state_key, state_stored],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Fails closed when SQLite reports corruption instead of allowing the
    /// desktop process to continue with a partially loaded in-memory snapshot.
    pub fn quick_integrity_check(&self) -> StorageResult<()> {
        let result: String = self
            .conn
            .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if result.trim().eq_ignore_ascii_case("ok") {
            Ok(())
        } else {
            Err(StorageError::Crypto(format!(
                "SQLite quick_check failed: {result}"
            )))
        }
    }

    pub fn delete_state_value(&self, key: &str) -> StorageResult<()> {
        self.conn
            .execute("DELETE FROM app_state WHERE state_key=?1", params![key])?;
        Ok(())
    }

    pub fn list_clause_blocks(&self) -> StorageResult<Vec<ClauseBlockRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT block_id,title,content,updated_at FROM clause_blocks ORDER BY block_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let raw = rows.collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(block_id, title, content, updated_at)| {
                Ok(ClauseBlockRecord {
                    block_id,
                    title: self.decode_sensitive(&title)?,
                    content: self.decode_sensitive(&content)?,
                    updated_at,
                })
            })
            .collect()
    }

    pub fn save_clause_block(
        &self,
        block_id: &str,
        title: &str,
        content: &str,
    ) -> StorageResult<()> {
        let encrypted_title = self.encode_sensitive(title)?;
        let encrypted_content = self.encode_sensitive(content)?;
        self.conn.execute("INSERT INTO clause_blocks(block_id,title,content) VALUES (?1,?2,?3) ON CONFLICT(block_id) DO UPDATE SET title=excluded.title,content=excluded.content,updated_at=CURRENT_TIMESTAMP",params![block_id,encrypted_title,encrypted_content])?;
        Ok(())
    }
    pub fn delete_clause_block(&self, block_id: &str) -> StorageResult<()> {
        self.conn.execute(
            "DELETE FROM clause_blocks WHERE block_id=?1",
            params![block_id],
        )?;
        Ok(())
    }
    pub fn clause_blocks_map(&self) -> StorageResult<std::collections::BTreeMap<String, String>> {
        Ok(self
            .list_clause_blocks()?
            .into_iter()
            .map(|b| (b.block_id, b.content))
            .collect())
    }

    pub fn usage_snapshot(&self, month_key: &str) -> StorageResult<UsageSnapshot> {
        let monthly: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT created_documents, trial_documents FROM commercial_usage WHERE month_key=?1",
                params![month_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let trial_total: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(trial_documents),0) FROM commercial_usage",
            [],
            |row| row.get(0),
        )?;
        let (created, _) = monthly.unwrap_or((0, 0));
        Ok(UsageSnapshot {
            month_key: month_key.to_string(),
            created_documents: created.max(0).try_into().unwrap_or(u32::MAX),
            trial_documents_total: trial_total.max(0).try_into().unwrap_or(u32::MAX),
        })
    }

    /// Reserves commercial usage under an IMMEDIATE transaction. This is the
    /// only supported writer path for UI and background processes, preventing
    /// lost updates and limit bypass by concurrent generation.
    pub fn reserve_usage(
        &mut self,
        month_key: &str,
        documents: u32,
        trial: bool,
        monthly_limit: u32,
        trial_total_limit: u32,
    ) -> StorageResult<UsageReservation> {
        if documents == 0 {
            return Err(StorageError::Crypto(
                "usage reservation cannot be empty".into(),
            ));
        }
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let monthly: Option<(i64, i64)> = tx
            .query_row(
                "SELECT created_documents, trial_documents FROM commercial_usage WHERE month_key=?1",
                params![month_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (created, _) = monthly.unwrap_or((0, 0));
        let trial_total: i64 = tx.query_row(
            "SELECT COALESCE(SUM(trial_documents),0) FROM commercial_usage",
            [],
            |row| row.get(0),
        )?;
        let requested = i64::from(documents);
        if created.saturating_add(requested) > i64::from(monthly_limit) {
            return Err(StorageError::Crypto(format!(
                "monthly document limit exceeded: {created}+{documents}>{monthly_limit}"
            )));
        }
        if trial && trial_total.saturating_add(requested) > i64::from(trial_total_limit) {
            return Err(StorageError::Crypto(format!(
                "trial document limit exceeded: {trial_total}+{documents}>{trial_total_limit}"
            )));
        }
        tx.execute(
            "INSERT INTO commercial_usage(month_key,created_documents,trial_documents) VALUES (?1,?2,?3) ON CONFLICT(month_key) DO UPDATE SET created_documents=created_documents+excluded.created_documents,trial_documents=trial_documents+excluded.trial_documents,updated_at=CURRENT_TIMESTAMP",
            params![month_key, requested, if trial { requested } else { 0 }],
        )?;
        let reservation_id = format!(
            "{}-{}-{}",
            std::process::id(),
            month_key,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        );
        tx.execute(
            "INSERT INTO usage_reservations(reservation_id,month_key,documents,trial,status) VALUES (?1,?2,?3,?4,'reserved')",
            params![reservation_id, month_key, requested, if trial { 1 } else { 0 }],
        )?;
        tx.commit()?;
        Ok(UsageReservation {
            reservation_id,
            month_key: month_key.to_string(),
            documents,
            trial,
        })
    }

    pub fn commit_usage(&mut self, reservation: &UsageReservation) -> StorageResult<bool> {
        let changed = self.conn.execute(
            "UPDATE usage_reservations SET status='committed',updated_at=CURRENT_TIMESTAMP WHERE reservation_id=?1 AND status='reserved'",
            params![reservation.reservation_id.as_str()],
        )?;
        Ok(changed == 1)
    }

    pub fn rollback_usage(&mut self, reservation: &UsageReservation) -> StorageResult<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let status: Option<String> = tx
            .query_row(
                "SELECT status FROM usage_reservations WHERE reservation_id=?1",
                params![reservation.reservation_id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if status.as_deref() != Some("reserved") {
            tx.commit()?;
            return Ok(false);
        }
        let count = i64::from(reservation.documents);
        tx.execute(
            "UPDATE commercial_usage SET created_documents=MAX(0,created_documents-?2),trial_documents=MAX(0,trial_documents-?3),updated_at=CURRENT_TIMESTAMP WHERE month_key=?1",
            params![reservation.month_key.as_str(), count, if reservation.trial { count } else { 0 }],
        )?;
        tx.execute(
            "UPDATE usage_reservations SET status='rolled_back',updated_at=CURRENT_TIMESTAMP WHERE reservation_id=?1",
            params![reservation.reservation_id.as_str()],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Rolls back reservations left by a process that could not complete. A long
    /// grace period prevents an active large batch from being reclaimed.
    pub fn recover_stale_usage_reservations(
        &mut self,
        max_age_minutes: u32,
    ) -> StorageResult<usize> {
        let modifier = format!("-{} minutes", max_age_minutes.max(60));
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let stale = {
            let mut statement = tx.prepare(
                "SELECT reservation_id,month_key,documents,trial FROM usage_reservations WHERE status='reserved' AND created_at <= datetime('now', ?1)",
            )?;
            let mapped = statement.query_map(params![modifier], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };
        for (reservation_id, month_key, documents, trial) in &stale {
            tx.execute(
                "UPDATE commercial_usage SET created_documents=MAX(0,created_documents-?2),trial_documents=MAX(0,trial_documents-?3),updated_at=CURRENT_TIMESTAMP WHERE month_key=?1",
                params![month_key, documents, if *trial != 0 { *documents } else { 0 }],
            )?;
            tx.execute(
                "UPDATE usage_reservations SET status='rolled_back_stale',updated_at=CURRENT_TIMESTAMP WHERE reservation_id=?1 AND status='reserved'",
                params![reservation_id],
            )?;
        }
        tx.commit()?;
        Ok(stale.len())
    }

    pub fn register_template_version(
        &mut self,
        document_id: &str,
        template_path: &str,
        template_sha256: &str,
        note: &str,
    ) -> StorageResult<TemplateVersionRecord> {
        if document_id.trim().is_empty() {
            return Err(StorageError::Crypto("document_id cannot be empty".into()));
        }
        if template_sha256.len() != 64
            || !template_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StorageError::Crypto(
                "template_sha256 must be lowercase SHA-256".into(),
            ));
        }
        // Encrypt before opening the SQLite transaction. This avoids borrowing
        // the repository immutably while its connection is mutably borrowed by
        // the transaction and keeps all fallible crypto work outside the lock.
        let version_id = random_record_id("tpl")?;
        let encrypted_path = self.encode_sensitive(template_path)?;
        let encrypted_note = self.encode_sensitive(note)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version_number),0)+1 FROM template_versions WHERE document_id=?1",
            params![document_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE template_versions SET status='archived' WHERE document_id=?1 AND status='published'",
            params![document_id],
        )?;
        tx.execute(
            "INSERT INTO template_versions(version_id,document_id,version_number,template_path,template_sha256,note,status) VALUES (?1,?2,?3,?4,?5,?6,'published')",
            params![version_id, document_id, next, encrypted_path, template_sha256, encrypted_note],
        )?;
        tx.commit()?;
        self.template_version_by_id(&version_id)?
            .ok_or_else(|| StorageError::Crypto("registered template version disappeared".into()))
    }

    pub fn template_version_by_id(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<TemplateVersionRecord>> {
        let raw = self
            .conn
            .query_row(
                "SELECT version_id,document_id,version_number,template_path,template_sha256,note,status,created_at FROM template_versions WHERE version_id=?1",
                params![version_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|row| self.decode_template_version_row(row))
            .transpose()
    }

    pub fn list_template_versions(
        &self,
        document_id: &str,
    ) -> StorageResult<Vec<TemplateVersionRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT version_id,document_id,version_number,template_path,template_sha256,note,status,created_at FROM template_versions WHERE document_id=?1 ORDER BY version_number DESC",
        )?;
        let raw = statement
            .query_map(params![document_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|row| self.decode_template_version_row(row))
            .collect()
    }

    fn decode_template_version_row(
        &self,
        row: (String, String, i64, String, String, String, String, String),
    ) -> StorageResult<TemplateVersionRecord> {
        Ok(TemplateVersionRecord {
            version_id: row.0,
            document_id: row.1,
            version_number: row.2.max(0).try_into().unwrap_or(u32::MAX),
            template_path: self.decode_sensitive(&row.3)?,
            template_sha256: row.4,
            note: self.decode_sensitive(&row.5)?,
            status: row.6,
            created_at: row.7,
        })
    }

    pub fn start_case_run(
        &self,
        source_sha256: &str,
        processing_fingerprint: &str,
        source_path: &str,
        request_json: &str,
        output_root: &str,
    ) -> StorageResult<CaseRunRecord> {
        if source_sha256.len() != 64
            || !source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StorageError::Crypto(
                "case source_sha256 must be lowercase SHA-256".into(),
            ));
        }
        if processing_fingerprint.len() != 64
            || !processing_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StorageError::Crypto(
                "case processing_fingerprint must be lowercase SHA-256".into(),
            ));
        }
        let case_id = random_record_id("case")?;
        self.conn.execute(
            "INSERT INTO case_runs(case_id,source_sha256,processing_fingerprint,source_path,status,request_json,output_root,patient_folder,created_files_json,missing_json,last_error) VALUES (?1,?2,?3,?4,'received',?5,?6,NULL,?7,?8,NULL)",
            params![
                case_id,
                source_sha256,
                processing_fingerprint,
                self.encode_sensitive(source_path)?,
                self.encode_sensitive(request_json)?,
                self.encode_sensitive(output_root)?,
                self.encode_sensitive("[]")?,
                self.encode_sensitive("[]")?,
            ],
        )?;
        self.case_run_by_id(&case_id)?
            .ok_or_else(|| StorageError::Crypto("created case run disappeared".into()))
    }

    pub fn update_case_run(
        &self,
        case_id: &str,
        status: &str,
        patient_folder: Option<&str>,
        created_files_json: &str,
        missing_json: &str,
        last_error: Option<&str>,
    ) -> StorageResult<bool> {
        const ALLOWED: &[&str] = &[
            "received",
            "normalizing",
            "recognizing",
            "checking",
            "attention",
            "ready",
            "generating",
            "publishing",
            "completed",
            "failed",
            "cancelled",
        ];
        if !ALLOWED.contains(&status) {
            return Err(StorageError::Crypto(format!(
                "unsupported case status: {status}"
            )));
        }
        let patient_folder = patient_folder
            .map(|value| self.encode_sensitive(value))
            .transpose()?;
        let last_error = last_error
            .map(|value| self.encode_sensitive(value))
            .transpose()?;
        let changed = self.conn.execute(
            "UPDATE case_runs SET status=?2,patient_folder=?3,created_files_json=?4,missing_json=?5,last_error=?6,updated_at=CURRENT_TIMESTAMP WHERE case_id=?1",
            params![
                case_id,
                status,
                patient_folder,
                self.encode_sensitive(created_files_json)?,
                self.encode_sensitive(missing_json)?,
                last_error,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn case_run_by_id(&self, case_id: &str) -> StorageResult<Option<CaseRunRecord>> {
        let raw = self
            .conn
            .query_row(
                "SELECT case_id,source_sha256,processing_fingerprint,source_path,status,request_json,output_root,patient_folder,created_files_json,missing_json,last_error,created_at,updated_at FROM case_runs WHERE case_id=?1",
                params![case_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|row| self.decode_case_run_row(row)).transpose()
    }

    pub fn update_case_run_source_path(
        &self,
        case_id: &str,
        source_path: &str,
    ) -> StorageResult<bool> {
        let encoded = self.encode_sensitive(source_path)?;
        Ok(self.conn.execute(
            "UPDATE case_runs SET source_path=?2,updated_at=CURRENT_TIMESTAMP WHERE case_id=?1",
            params![case_id, encoded],
        )? > 0)
    }

    /// Returns true only when this exact content hash already reached the
    /// terminal completed state. Attention/failed attempts remain retryable.
    pub fn completed_case_exists_for_source_hash(
        &self,
        source_sha256: &str,
    ) -> StorageResult<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM case_runs WHERE source_sha256=?1 AND status='completed' LIMIT 1)",
            params![source_sha256],
            |row| row.get(0),
        )?;
        Ok(exists != 0)
    }

    /// A completed source is reusable only for the exact automation plan that
    /// produced it. Template, workflow, reference-data or engine changes must
    /// create a new run even when the source bytes are unchanged.
    pub fn completed_case_exists_for_source_and_plan(
        &self,
        source_sha256: &str,
        processing_fingerprint: &str,
    ) -> StorageResult<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM case_runs WHERE source_sha256=?1 AND processing_fingerprint=?2 AND status='completed' LIMIT 1)",
            params![source_sha256, processing_fingerprint],
            |row| row.get(0),
        )?;
        Ok(exists != 0)
    }

    /// Marks case attempts that were left in a non-terminal state by a crash or
    /// power loss. The encrypted request remains available for an explicit,
    /// auditable retry; no document is silently treated as completed.
    pub fn recover_interrupted_case_runs(&self) -> StorageResult<usize> {
        let changed = self.conn.execute(
            "UPDATE case_runs SET status='failed',last_error=?1,updated_at=CURRENT_TIMESTAMP              WHERE status NOT IN ('completed','attention','failed','cancelled')",
            params![self.encode_sensitive(
                "Предыдущий запуск был прерван. Дело безопасно остановлено и может быть повторено из центра обработки."
            )?],
        )?;
        Ok(changed)
    }

    pub fn list_case_runs(&self, limit: usize) -> StorageResult<Vec<CaseRunRecord>> {
        let limit = limit.clamp(1, 500) as i64;
        let mut statement = self.conn.prepare(
            "SELECT case_id,source_sha256,processing_fingerprint,source_path,status,request_json,output_root,patient_folder,created_files_json,missing_json,last_error,created_at,updated_at FROM case_runs ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let raw = statement
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|row| self.decode_case_run_row(row))
            .collect()
    }

    pub fn upsert_case_document(&self, record: &CaseDocumentRecord) -> StorageResult<()> {
        if record.input_fingerprint.len() != 64
            || !record
                .input_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(StorageError::Crypto(
                "document input_fingerprint must be lowercase SHA-256".into(),
            ));
        }
        if record.output_sha256.len() != 64
            || !record
                .output_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || record.output_size_bytes == 0
        {
            return Err(StorageError::Crypto(
                "document output integrity metadata must contain lowercase SHA-256 and non-zero size".into(),
            ));
        }
        const ALLOWED: &[&str] = &["rendered", "published", "reused", "invalidated"];
        if !ALLOWED.contains(&record.status.as_str()) {
            return Err(StorageError::Crypto(format!(
                "unsupported case document status: {}",
                record.status
            )));
        }
        let output_path = self.encode_sensitive(&record.output_path)?;
        self.conn.execute(
            "INSERT INTO case_run_documents(case_id,document_id,input_fingerprint,output_path,output_sha256,output_size_bytes,status,reused_from_case_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(case_id,document_id) DO UPDATE SET input_fingerprint=excluded.input_fingerprint,output_path=excluded.output_path,output_sha256=excluded.output_sha256,output_size_bytes=excluded.output_size_bytes,status=excluded.status,reused_from_case_id=excluded.reused_from_case_id,updated_at=CURRENT_TIMESTAMP",
            params![
                &record.case_id,
                &record.document_id,
                &record.input_fingerprint,
                output_path,
                &record.output_sha256,
                i64::try_from(record.output_size_bytes).map_err(|_| StorageError::Crypto("document output is too large".into()))?,
                &record.status,
                &record.reused_from_case_id,
            ],
        )?;
        Ok(())
    }

    pub fn list_case_documents(&self, case_id: &str) -> StorageResult<Vec<CaseDocumentRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT case_id,document_id,input_fingerprint,output_path,output_sha256,output_size_bytes,status,reused_from_case_id,created_at,updated_at FROM case_run_documents WHERE case_id=?1 ORDER BY document_id",
        )?;
        let rows = statement
            .query_map(params![case_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|row| {
                Ok(CaseDocumentRecord {
                    case_id: row.0,
                    document_id: row.1,
                    input_fingerprint: row.2,
                    output_path: self.decode_sensitive(&row.3)?,
                    output_sha256: row.4,
                    output_size_bytes: u64::try_from(row.5).map_err(|_| {
                        StorageError::Crypto("negative document output size".into())
                    })?,
                    status: row.6,
                    reused_from_case_id: row.7,
                    created_at: row.8,
                    updated_at: row.9,
                })
            })
            .collect()
    }

    fn decode_case_run_row(&self, row: CaseRunRow) -> StorageResult<CaseRunRecord> {
        Ok(CaseRunRecord {
            case_id: row.0,
            source_sha256: row.1,
            processing_fingerprint: row.2,
            source_path: self.decode_sensitive(&row.3)?,
            status: row.4,
            request_json: self.decode_sensitive(&row.5)?,
            output_root: self.decode_sensitive(&row.6)?,
            patient_folder: row
                .7
                .map(|value| self.decode_sensitive(&value))
                .transpose()?,
            created_files_json: self.decode_sensitive(&row.8)?,
            missing_json: self.decode_sensitive(&row.9)?,
            last_error: row
                .10
                .map(|value| self.decode_sensitive(&value))
                .transpose()?,
            created_at: row.11,
            updated_at: row.12,
        })
    }

    pub fn create_exception(
        &self,
        category: &str,
        source_path: &str,
        message: &str,
        details_json: &str,
    ) -> StorageResult<AutomationExceptionRecord> {
        let exception_id = random_record_id("exc")?;
        let encrypted_source = self.encode_sensitive(source_path)?;
        let encrypted_message = self.encode_sensitive(message)?;
        let encrypted_details = self.encode_sensitive(details_json)?;
        self.conn.execute(
            "INSERT INTO automation_exceptions(exception_id,category,source_path,message,details_json,status) VALUES (?1,?2,?3,?4,?5,'open')",
            params![exception_id, category, encrypted_source, encrypted_message, encrypted_details],
        )?;
        self.exception_by_id(&exception_id)?
            .ok_or_else(|| StorageError::Crypto("created exception disappeared".into()))
    }

    pub fn list_exceptions(
        &self,
        include_resolved: bool,
    ) -> StorageResult<Vec<AutomationExceptionRecord>> {
        let sql = if include_resolved {
            "SELECT exception_id,category,source_path,message,details_json,status,created_at,updated_at FROM automation_exceptions ORDER BY created_at DESC"
        } else {
            "SELECT exception_id,category,source_path,message,details_json,status,created_at,updated_at FROM automation_exceptions WHERE status='open' ORDER BY created_at DESC"
        };
        let mut statement = self.conn.prepare(sql)?;
        let raw = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|row| self.decode_exception_row(row))
            .collect()
    }

    pub fn resolve_exception(&self, exception_id: &str, resolution: &str) -> StorageResult<bool> {
        let Some(existing) = self.exception_by_id(exception_id)? else {
            return Ok(false);
        };
        if existing.status != "open" {
            return Ok(false);
        }
        let original_details = serde_json::from_str::<serde_json::Value>(&existing.details_json)
            .unwrap_or_else(|_| serde_json::json!({ "text": existing.details_json }));
        let merged = serde_json::json!({
            "original": original_details,
            "resolution": {
                "text": resolution,
                "resolved_at_unix": unix_timestamp_string(),
            }
        });
        let encoded = self.encode_sensitive(&serde_json::to_string(&merged)?)?;
        let changed = self.conn.execute(
            "UPDATE automation_exceptions SET status='resolved',details_json=?2,updated_at=CURRENT_TIMESTAMP WHERE exception_id=?1 AND status='open'",
            params![exception_id, encoded],
        )?;
        Ok(changed == 1)
    }

    pub fn append_audit_event(
        &mut self,
        event_type: &str,
        object_hash: &str,
        detail_json: &str,
    ) -> StorageResult<AuditEventRecord> {
        let event_id = random_record_id("audit")?;
        let created_at = unix_timestamp_string();
        let encrypted_detail = self.encode_sensitive(detail_json)?;
        // Copy the key before opening the mutable SQLite transaction. This keeps
        // audit hashing independent from `self` while `self.conn` is mutably
        // borrowed by rusqlite.
        let sensitive_key = self.sensitive_key;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous_hash: String = tx
            .query_row(
                "SELECT event_hash FROM audit_events ORDER BY rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default();
        let material = format!(
            "{}|{}|{}|{}|{}|{}",
            previous_hash, event_id, event_type, object_hash, detail_json, created_at
        );
        let event_hash =
            authenticated_audit_hash_with_key(sensitive_key.as_ref(), material.as_bytes())?;
        tx.execute(
            "INSERT INTO audit_events(event_id,event_type,object_hash,detail_json,previous_hash,event_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![event_id, event_type, object_hash, encrypted_detail, previous_hash, event_hash, created_at],
        )?;
        tx.commit()?;
        Ok(AuditEventRecord {
            event_id,
            event_type: event_type.to_string(),
            object_hash: object_hash.to_string(),
            detail_json: detail_json.to_string(),
            previous_hash,
            event_hash,
            created_at,
        })
    }

    pub fn verify_audit_chain(&self) -> StorageResult<bool> {
        let mut statement = self.conn.prepare(
            "SELECT event_id,event_type,object_hash,detail_json,previous_hash,event_hash,created_at FROM audit_events ORDER BY rowid ASC",
        )?;
        let raw = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut expected_previous = String::new();
        for (event_id, event_type, object_hash, detail, previous_hash, event_hash, created_at) in
            raw
        {
            if previous_hash != expected_previous {
                return Err(StorageError::Crypto(format!(
                    "audit chain predecessor mismatch at event {event_id}"
                )));
            }
            let detail_json = self.decode_sensitive(&detail)?;
            let material = format!(
                "{}|{}|{}|{}|{}|{}",
                previous_hash, event_id, event_type, object_hash, detail_json, created_at
            );
            let expected_hash = if event_hash.starts_with("hmac:v1:") {
                self.authenticated_audit_hash(material.as_bytes())?
            } else {
                // Backwards-compatible validation for pre-18.2.2 audit rows.
                sha256_hex(material.as_bytes())
            };
            if event_hash != expected_hash {
                return Err(StorageError::Crypto(format!(
                    "audit event authentication failed at event {event_id}"
                )));
            }
            expected_previous = event_hash;
        }
        Ok(true)
    }

    pub fn list_audit_events(&self, limit: usize) -> StorageResult<Vec<AuditEventRecord>> {
        self.verify_audit_chain()?;
        let limit = limit.clamp(1, 1_000) as i64;
        let mut statement = self.conn.prepare(
            "SELECT event_id,event_type,object_hash,detail_json,previous_hash,event_hash,created_at FROM audit_events ORDER BY rowid DESC LIMIT ?1",
        )?;
        let raw = statement
            .query_map(params![limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(
                |(
                    event_id,
                    event_type,
                    object_hash,
                    detail,
                    previous_hash,
                    event_hash,
                    created_at,
                )| {
                    Ok(AuditEventRecord {
                        event_id,
                        event_type,
                        object_hash,
                        detail_json: self.decode_sensitive(&detail)?,
                        previous_hash,
                        event_hash,
                        created_at,
                    })
                },
            )
            .collect()
    }

    pub fn increment_metric(&self, metric_key: &str, amount: u64) -> StorageResult<()> {
        let amount = i64::try_from(amount)
            .map_err(|_| StorageError::Crypto("metric increment out of range".into()))?;
        self.conn.execute(
            "INSERT INTO automation_metrics(metric_key,value) VALUES (?1,?2) ON CONFLICT(metric_key) DO UPDATE SET value=value+excluded.value,updated_at=CURRENT_TIMESTAMP",
            params![metric_key, amount],
        )?;
        Ok(())
    }

    pub fn automation_metrics(&self) -> StorageResult<AutomationMetrics> {
        let mut statement = self
            .conn
            .prepare("SELECT metric_key,value FROM automation_metrics")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut metrics = AutomationMetrics::default();
        for (key, value) in rows {
            let value = value.max(0) as u64;
            match key.as_str() {
                "processed_sources" => metrics.processed_sources = value,
                "generated_documents" => metrics.generated_documents = value,
                "blocked_sources" => metrics.blocked_sources = value,
                "failed_sources" => metrics.failed_sources = value,
                "print_failures" => metrics.print_failures = value,
                "user_confirmations" => metrics.user_confirmations = value,
                "zero_touch_sources" => metrics.zero_touch_sources = value,
                "attention_resolutions" => metrics.attention_resolutions = value,
                "model_grounding_rejections" => metrics.model_grounding_rejections = value,
                "shadow_model_runs" => metrics.shadow_model_runs = value,
                "shadow_model_proposals" => metrics.shadow_model_proposals = value,
                "shadow_model_agreements" => metrics.shadow_model_agreements = value,
                "reused_documents" => metrics.reused_documents = value,
                "rerendered_documents" => metrics.rerendered_documents = value,
                "processing_milliseconds" => metrics.processing_milliseconds = value,
                "print_review_queued" => metrics.print_review_queued = value,
                "automatic_print_approved" => metrics.automatic_print_approved = value,
                _ => {}
            }
        }
        Ok(metrics)
    }

    fn exception_by_id(
        &self,
        exception_id: &str,
    ) -> StorageResult<Option<AutomationExceptionRecord>> {
        let raw = self
            .conn
            .query_row(
                "SELECT exception_id,category,source_path,message,details_json,status,created_at,updated_at FROM automation_exceptions WHERE exception_id=?1",
                params![exception_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|row| self.decode_exception_row(row)).transpose()
    }

    fn decode_exception_row(
        &self,
        row: (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    ) -> StorageResult<AutomationExceptionRecord> {
        Ok(AutomationExceptionRecord {
            exception_id: row.0,
            category: row.1,
            source_path: self.decode_sensitive(&row.2)?,
            message: self.decode_sensitive(&row.3)?,
            details_json: self.decode_sensitive(&row.4)?,
            status: row.5,
            created_at: row.6,
            updated_at: row.7,
        })
    }

    /// Atomically increment a document counter inside an immediate transaction.
    pub fn next_counter(&mut self, counter_key: &str, year: i32) -> StorageResult<CounterValue> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT value FROM document_counters WHERE counter_key=?1 AND counter_year=?2",
                params![counter_key, year],
                |r| r.get(0),
            )
            .optional()?;
        let next = current
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| StorageError::Crypto("counter overflow".into()))?;
        tx.execute("INSERT INTO document_counters(counter_key,counter_year,value) VALUES (?1,?2,?3) ON CONFLICT(counter_key,counter_year) DO UPDATE SET value=excluded.value,updated_at=CURRENT_TIMESTAMP",params![counter_key,year,next])?;
        tx.commit()?;
        Ok(CounterValue {
            counter_key: counter_key.to_string(),
            year,
            value: next as u64,
        })
    }
    pub fn peek_counter(&self, counter_key: &str, year: i32) -> StorageResult<CounterValue> {
        let current: Option<i64> = self
            .conn
            .query_row(
                "SELECT value FROM document_counters WHERE counter_key=?1 AND counter_year=?2",
                params![counter_key, year],
                |r| r.get(0),
            )
            .optional()?;
        Ok(CounterValue {
            counter_key: counter_key.to_string(),
            year,
            value: current.unwrap_or(0).max(0) as u64,
        })
    }

    /// Roll back a reservation only while it is still the latest value.
    ///
    /// This deliberately refuses to decrement a counter after another writer has
    /// advanced it, because reusing an already observed number would be worse than
    /// leaving a harmless gap after a failed generation.
    pub fn rollback_counter(&mut self, reservation: &CounterValue) -> StorageResult<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT value FROM document_counters WHERE counter_key=?1 AND counter_year=?2",
                params![reservation.counter_key.as_str(), reservation.year],
                |row| row.get(0),
            )
            .optional()?;
        let expected = i64::try_from(reservation.value)
            .map_err(|_| StorageError::Crypto("counter value out of range".into()))?;
        if current != Some(expected) {
            tx.commit()?;
            return Ok(false);
        }
        if expected <= 1 {
            tx.execute(
                "DELETE FROM document_counters WHERE counter_key=?1 AND counter_year=?2",
                params![reservation.counter_key.as_str(), reservation.year],
            )?;
        } else {
            tx.execute(
                "UPDATE document_counters SET value=?3, updated_at=CURRENT_TIMESTAMP WHERE counter_key=?1 AND counter_year=?2",
                params![reservation.counter_key.as_str(), reservation.year, expected - 1],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    fn authenticated_audit_hash(&self, material: &[u8]) -> StorageResult<String> {
        authenticated_audit_hash_with_key(self.sensitive_key.as_ref(), material)
    }

    fn encode_sensitive(&self, plaintext: &str) -> StorageResult<String> {
        let Some(master_key) = self.sensitive_key.as_ref() else {
            return Ok(plaintext.to_string());
        };
        let (enc_key, mac_key) = derive_keys(master_key);
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce)
            .map_err(|error| StorageError::Crypto(format!("nonce generation failed: {error}")))?;
        let mut ciphertext = plaintext.as_bytes().to_vec();
        apply_aes256_ctr(&enc_key, &nonce, &mut ciphertext);

        let mut mac = <HmacSha256 as Mac>::new_from_slice(&mac_key)
            .map_err(|_| StorageError::Crypto("invalid authentication key".into()))?;
        mac.update(b"dokkomplekt-storage-v1");
        mac.update(&nonce);
        mac.update(&ciphertext);
        let tag = mac.finalize().into_bytes();

        let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len() + TAG_LEN);
        payload.extend_from_slice(&nonce);
        payload.extend_from_slice(&ciphertext);
        payload.extend_from_slice(&tag);
        Ok(format!(
            "{ENCRYPTED_PREFIX}{}",
            BASE64_STANDARD.encode(payload)
        ))
    }

    fn decode_sensitive(&self, stored: &str) -> StorageResult<String> {
        let Some(encoded) = stored.strip_prefix(ENCRYPTED_PREFIX) else {
            return Ok(stored.to_string());
        };
        let master_key = self
            .sensitive_key
            .as_ref()
            .ok_or(StorageError::EncryptionRequired)?;
        let payload = BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| StorageError::Crypto(format!("base64: {error}")))?;
        if payload.len() < NONCE_LEN + TAG_LEN {
            return Err(StorageError::Crypto("payload is truncated".into()));
        }
        let (nonce, remainder) = payload.split_at(NONCE_LEN);
        let (ciphertext, tag) = remainder.split_at(remainder.len() - TAG_LEN);
        let (enc_key, mac_key) = derive_keys(master_key);

        let mut mac = <HmacSha256 as Mac>::new_from_slice(&mac_key)
            .map_err(|_| StorageError::Crypto("invalid authentication key".into()))?;
        mac.update(b"dokkomplekt-storage-v1");
        mac.update(nonce);
        mac.update(ciphertext);
        mac.verify_slice(tag)
            .map_err(|_| StorageError::Crypto("authentication failed".into()))?;

        let mut plaintext = ciphertext.to_vec();
        let mut nonce_array = [0u8; NONCE_LEN];
        nonce_array.copy_from_slice(nonce);
        apply_aes256_ctr(&enc_key, &nonce_array, &mut plaintext);
        String::from_utf8(plaintext)
            .map_err(|error| StorageError::Crypto(format!("utf-8: {error}")))
    }
}

fn authenticated_audit_hash_with_key(
    master_key: Option<&[u8; 32]>,
    material: &[u8],
) -> StorageResult<String> {
    let Some(master_key) = master_key else {
        return Ok(sha256_hex(material));
    };
    let (_, mac_key) = derive_keys(master_key);
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&mac_key)
        .map_err(|_| StorageError::Crypto("invalid audit authentication key".into()))?;
    mac.update(b"dokkomplekt-audit-chain-v1");
    mac.update(material);
    Ok(format!(
        "hmac:v1:{}",
        hex_bytes(&mac.finalize().into_bytes())
    ))
}

fn random_record_id(prefix: &str) -> StorageResult<String> {
    let mut random = [0u8; 16];
    getrandom::getrandom(&mut random)
        .map_err(|error| StorageError::Crypto(format!("random id generation failed: {error}")))?;
    Ok(format!("{}-{}", prefix, hex_bytes(&random)))
}

fn unix_timestamp_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn derive_keys(master_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut enc_hasher = Sha256::new();
    enc_hasher.update(b"dokkomplekt-storage-encryption-v1");
    enc_hasher.update(master_key);
    let enc_key: [u8; 32] = enc_hasher.finalize().into();

    let mut mac_hasher = Sha256::new();
    mac_hasher.update(b"dokkomplekt-storage-authentication-v1");
    mac_hasher.update(master_key);
    let mac_key: [u8; 32] = mac_hasher.finalize().into();
    (enc_key, mac_key)
}

fn apply_aes256_ctr(key: &[u8; 32], nonce: &[u8; NONCE_LEN], data: &mut [u8]) {
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut counter = *nonce;
    for chunk in data.chunks_mut(16) {
        let mut stream = GenericArray::clone_from_slice(&counter);
        cipher.encrypt_block(&mut stream);
        for (byte, key_byte) in chunk.iter_mut().zip(stream.iter()) {
            *byte ^= key_byte;
        }
        increment_counter(&mut counter);
    }
}

fn increment_counter(counter: &mut [u8; NONCE_LEN]) {
    for byte in counter.iter_mut().rev() {
        let (next, overflow) = byte.overflowing_add(1);
        *byte = next;
        if !overflow {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dokkomplekt_core::{SemanticValue, ValueSource};
    use rusqlite::OptionalExtension;
    use std::collections::BTreeMap;

    fn temp_db(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dokkomplekt-storage-{label}-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ))
    }

    fn confidential_case() -> SemanticCase {
        SemanticCase {
            values: BTreeMap::from([(
                "person.full_name".to_string(),
                SemanticValue {
                    field_id: "person.full_name".to_string(),
                    value: "Иванов Иван Иванович".to_string(),
                    source: ValueSource::UserConfirmed,
                    confidence: 1.0,
                    evidence: Vec::new(),
                },
            )]),
            active_domains: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn encrypted_case_is_not_visible_in_raw_sqlite_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("dokkomplekt-storage-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("encrypted.sqlite");
        let _ = std::fs::remove_file(&path);
        let key = [7u8; 32];
        let repo = LocalRepository::open_with_key(&path, key).expect("repo");
        let case = confidential_case();
        repo.save_case("current", &case).expect("save");
        assert_eq!(repo.load_case("current").expect("load"), Some(case));

        let raw: Option<String> = repo
            .conn
            .query_row(
                "SELECT json FROM semantic_cases WHERE case_id='current'",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("raw");
        let raw = raw.expect("row");
        assert!(raw.starts_with(ENCRYPTED_PREFIX));
        assert!(!raw.contains("Иванов"));
        drop(repo);

        let plaintext_repo = LocalRepository::open(&path).expect("repo without key");
        assert!(matches!(
            plaintext_repo.load_case("current"),
            Err(StorageError::EncryptionRequired)
        ));
        let wrong_repo = LocalRepository::open_with_key(&path, [8u8; 32]).expect("wrong repo");
        assert!(matches!(
            wrong_repo.load_case("current"),
            Err(StorageError::Crypto(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn plaintext_database_remains_backwards_compatible() {
        let path = std::env::temp_dir().join(format!(
            "dokkomplekt-storage-plaintext-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let case = confidential_case();
        let repo = LocalRepository::open(&path).expect("repo");
        repo.save_case("current", &case).expect("save");
        assert_eq!(repo.load_case("current").expect("load"), Some(case));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn counter_rollback_is_safe_for_latest_and_stale_reservations() {
        let path = temp_db("counter-rollback");
        let mut repo = LocalRepository::open(&path).unwrap();
        let first = repo.next_counter("contract.number", 2026).unwrap();
        let second = repo.next_counter("contract.number", 2026).unwrap();
        assert!(!repo.rollback_counter(&first).unwrap());
        assert!(repo.rollback_counter(&second).unwrap());
        assert_eq!(repo.peek_counter("contract.number", 2026).unwrap().value, 1);
        assert!(repo.rollback_counter(&first).unwrap());
        assert_eq!(repo.peek_counter("contract.number", 2026).unwrap().value, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn usage_reservation_is_atomic_and_rollback_is_idempotent() {
        let path = temp_db("usage-reservation");
        let mut repo = LocalRepository::open(&path).unwrap();
        let reservation = repo.reserve_usage("2026-07", 3, true, 30, 30).unwrap();
        let snapshot = repo.usage_snapshot("2026-07").unwrap();
        assert_eq!(snapshot.created_documents, 3);
        assert_eq!(snapshot.trial_documents_total, 3);
        assert!(repo.rollback_usage(&reservation).unwrap());
        assert!(!repo.rollback_usage(&reservation).unwrap());
        assert_eq!(repo.usage_snapshot("2026-07").unwrap().created_documents, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn usage_reservation_fails_closed_at_limit() {
        let path = temp_db("usage-limit");
        let mut repo = LocalRepository::open(&path).unwrap();
        repo.reserve_usage("2026-07", 3, true, 3, 3).unwrap();
        assert!(repo.reserve_usage("2026-07", 1, true, 3, 3).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stale_usage_reservations_are_rolled_back_atomically() {
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
        assert_eq!(repo.usage_snapshot("2026-07").unwrap().created_documents, 0);
        assert!(!repo.rollback_usage(&reservation).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exception_resolution_preserves_original_encrypted_details() {
        let path = temp_db("exceptions");
        let repo = LocalRepository::open_with_key(&path, [11u8; 32]).unwrap();
        let created = repo
            .create_exception(
                "confidence",
                "C:/secret/patient.docx",
                "Требуется подтверждение",
                r#"{"field":"person.full_name"}"#,
            )
            .unwrap();
        let raw: String = repo
            .conn
            .query_row(
                "SELECT source_path || message || details_json FROM automation_exceptions WHERE exception_id=?1",
                params![created.exception_id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("patient"));
        assert!(!raw.contains("person.full_name"));
        assert!(repo
            .resolve_exception(&created.exception_id, "Проверено специалистом")
            .unwrap());
        let resolved = repo.list_exceptions(true).unwrap().remove(0);
        assert_eq!(resolved.status, "resolved");
        let details: serde_json::Value = serde_json::from_str(&resolved.details_json).unwrap();
        assert_eq!(details["original"]["field"], "person.full_name");
        assert_eq!(details["resolution"]["text"], "Проверено специалистом");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn template_registry_versions_and_archives_previous_publication() {
        let path = temp_db("template-versions");
        let mut repo = LocalRepository::open_with_key(&path, [14u8; 32]).unwrap();
        let first = repo
            .register_template_version(
                "invoice",
                "C:/secret/invoice-v1.docx",
                &"a".repeat(64),
                "Первая публикация",
            )
            .unwrap();
        let second = repo
            .register_template_version(
                "invoice",
                "C:/secret/invoice-v2.docx",
                &"b".repeat(64),
                "Исправлена таблица",
            )
            .unwrap();
        assert_eq!(first.version_number, 1);
        assert_eq!(second.version_number, 2);
        let versions = repo.list_template_versions("invoice").unwrap();
        assert_eq!(versions[0].status, "published");
        assert_eq!(versions[1].status, "archived");
        let raw: String = repo
            .conn
            .query_row(
                "SELECT template_path || note FROM template_versions WHERE version_id=?1",
                params![second.version_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("invoice-v2"));
        assert!(!raw.contains("таблица"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn case_run_state_machine_is_encrypted_and_resumable() {
        let path = temp_db("case-runs");
        let repo = LocalRepository::open_with_key(&path, [13u8; 32]).unwrap();
        let created = repo
            .start_case_run(
                &"a".repeat(64),
                &"d".repeat(64),
                "C:/secret/Иванов.docx",
                r#"{"sourcePath":"C:/secret/Иванов.docx"}"#,
                "C:/secret/output",
            )
            .unwrap();
        assert_eq!(created.status, "received");
        assert!(repo
            .update_case_run(
                &created.case_id,
                "attention",
                None,
                "[]",
                r#"["person.full_name"]"#,
                Some("Требуется подтверждение"),
            )
            .unwrap());
        let restored = repo.case_run_by_id(&created.case_id).unwrap().unwrap();
        assert_eq!(restored.status, "attention");
        assert!(restored.missing_json.contains("person.full_name"));
        assert!(!repo
            .completed_case_exists_for_source_hash(&"a".repeat(64))
            .unwrap());
        assert!(repo
            .update_case_run(
                &created.case_id,
                "completed",
                Some("C:/secret/result"),
                r#"["result.docx"]"#,
                "[]",
                None,
            )
            .unwrap());
        assert!(repo
            .completed_case_exists_for_source_hash(&"a".repeat(64))
            .unwrap());
        let raw: String = repo
            .conn
            .query_row(
                "SELECT source_path || request_json || output_root || COALESCE(patient_folder, '') || created_files_json || missing_json || COALESCE(last_error, '') FROM case_runs WHERE case_id=?1",
                params![created.case_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("Иванов"));
        assert!(!raw.contains("person.full_name"));
        let interrupted = repo
            .start_case_run(
                &"b".repeat(64),
                &"d".repeat(64),
                "C:/secret/Петров.docx",
                r#"{"sourcePath":"C:/secret/Петров.docx"}"#,
                "C:/secret/output",
            )
            .unwrap();
        assert!(repo
            .update_case_run(&interrupted.case_id, "normalizing", None, "[]", "[]", None)
            .unwrap());
        assert_eq!(repo.list_case_runs(10).unwrap().len(), 2);
        assert_eq!(repo.recover_interrupted_case_runs().unwrap(), 1);
        let recovered = repo.case_run_by_id(&interrupted.case_id).unwrap().unwrap();
        assert_eq!(recovered.status, "failed");
        assert!(recovered.last_error.unwrap().contains("прерван"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn per_document_resume_records_are_encrypted_and_queryable() {
        let path = temp_db("case-document-resume");
        let repo = LocalRepository::open_with_key(&path, [15u8; 32]).unwrap();
        let case = repo
            .start_case_run(
                &"c".repeat(64),
                &"d".repeat(64),
                "C:/secret/source.docx",
                r#"{"source_path":"C:/secret/source.docx"}"#,
                "C:/secret/output",
            )
            .unwrap();
        repo.upsert_case_document(&CaseDocumentRecord {
            case_id: case.case_id.clone(),
            document_id: "contract".into(),
            input_fingerprint: "d".repeat(64),
            output_path: "C:/secret/checkpoints/contract.docx".into(),
            output_sha256: "e".repeat(64),
            output_size_bytes: 123,
            status: "rendered".into(),
            reused_from_case_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        })
        .unwrap();
        let records = repo.list_case_documents(&case.case_id).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].document_id, "contract");
        assert!(records[0].output_path.contains("checkpoints"));
        let raw: String = repo
            .conn
            .query_row(
                "SELECT output_path FROM case_run_documents WHERE case_id=?1",
                params![case.case_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("checkpoints"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn audit_chain_is_verified_and_tampering_is_detected() {
        let path = temp_db("audit-chain");
        let mut repo = LocalRepository::open_with_key(&path, [12u8; 32]).unwrap();
        repo.append_audit_event("source_received", "hash-a", r#"{"name":"A"}"#)
            .unwrap();
        repo.append_audit_event("documents_generated", "hash-b", r#"{"count":2}"#)
            .unwrap();
        assert!(repo.verify_audit_chain().unwrap());
        assert_eq!(repo.list_audit_events(10).unwrap().len(), 2);
        repo.conn
            .execute(
                "UPDATE audit_events SET object_hash='tampered' WHERE event_type='source_received'",
                [],
            )
            .unwrap();
        assert!(matches!(
            repo.verify_audit_chain(),
            Err(StorageError::Crypto(_))
        ));
        assert!(repo.list_audit_events(10).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn corpus_entries_are_encrypted_and_round_trip_without_raw_values() {
        use dokkomplekt_core::{
            build_corpus_entry, CorpusAcceptanceSource, CorpusEntryRequest, DomainKind,
            SemanticValue, ValueSource,
        };

        let path = temp_db("corpus");
        let repo = LocalRepository::open_with_key(&path, [22u8; 32]).unwrap();
        let mut final_case = SemanticCase::default();
        final_case.values.insert(
            "subject.name".into(),
            SemanticValue::new(
                "subject.name",
                "Иванов Иван Иванович",
                ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let source_sha256 = "c".repeat(64);
        let model_case = SemanticCase::default();
        let deterministic_case = SemanticCase::default();
        let entry = build_corpus_entry(CorpusEntryRequest {
            entry_id: "entry-storage-1".into(),
            case_id: "case-storage-1".into(),
            source_sha256: &source_sha256,
            fingerprint_key: &[22u8; 32],
            input_text: "ФИО: Иванов Иван Иванович",
            domain: DomainKind::Hr,
            pack_id: Some("hr-pack".into()),
            cluster_id: Some("employment-intake".into()),
            model_case: &model_case,
            deterministic_case: &deterministic_case,
            final_case: &final_case,
            field_acceptance_source: CorpusAcceptanceSource::SpecialistConfirmed,
            proposed_kit_documents: vec!["employment_contract".into()],
            kit_proposal_source: Some("curated-router".into()),
            kit_documents: vec!["employment_contract".into()],
            kit_acceptance_source: CorpusAcceptanceSource::SpecialistConfirmed,
            created_at: "2026-07-21T12:00:00Z".into(),
        })
        .unwrap();
        repo.append_corpus_entry(&entry).unwrap();
        assert_eq!(repo.corpus_entry_count().unwrap(), 1);
        assert_eq!(repo.list_corpus_entries(10).unwrap(), vec![entry]);
        let raw: String = repo
            .conn
            .query_row("SELECT json FROM corpus_entries LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(raw.starts_with(ENCRYPTED_PREFIX));
        assert!(!raw.contains("Иванов"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn automation_metrics_accumulate_known_keys() {
        let path = temp_db("metrics");
        let repo = LocalRepository::open(&path).unwrap();
        repo.increment_metric("processed_sources", 2).unwrap();
        repo.increment_metric("processed_sources", 3).unwrap();
        repo.increment_metric("generated_documents", 4).unwrap();
        repo.increment_metric("reused_documents", 2).unwrap();
        repo.increment_metric("rerendered_documents", 1).unwrap();
        repo.increment_metric("processing_milliseconds", 12_345)
            .unwrap();
        repo.increment_metric("print_review_queued", 3).unwrap();
        repo.increment_metric("automatic_print_approved", 1)
            .unwrap();
        repo.increment_metric("unknown_future_metric", 9).unwrap();
        let metrics = repo.automation_metrics().unwrap();
        assert_eq!(metrics.processed_sources, 5);
        assert_eq!(metrics.generated_documents, 4);
        assert_eq!(metrics.reused_documents, 2);
        assert_eq!(metrics.rerendered_documents, 1);
        assert_eq!(metrics.processing_milliseconds, 12_345);
        assert_eq!(metrics.print_review_queued, 3);
        assert_eq!(metrics.automatic_print_approved, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn clause_blocks_crud_and_counters_are_partitioned() {
        let path = temp_db("blocks-counters");
        let mut repo = LocalRepository::open(&path).unwrap();
        repo.save_clause_block("requisites", "Реквизиты", "{{org.name}}")
            .unwrap();
        assert_eq!(
            repo.list_clause_blocks().unwrap()[0].content,
            "{{org.name}}"
        );
        assert_eq!(repo.next_counter("contract.number", 2026).unwrap().value, 1);
        assert_eq!(repo.next_counter("contract.number", 2026).unwrap().value, 2);
        assert_eq!(repo.next_counter("contract.number", 2027).unwrap().value, 1);
        repo.delete_clause_block("requisites").unwrap();
        assert!(repo.list_clause_blocks().unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn desktop_snapshot_round_trips_case_pack_and_commercial_state_atomically() {
        let path = temp_db("desktop-snapshot");
        let repo = LocalRepository::open_with_key(&path, [31u8; 32]).unwrap();
        let mut case = SemanticCase::default();
        case.values.insert(
            "subject.name".into(),
            dokkomplekt_core::SemanticValue::new(
                "subject.name",
                "Иванов Иван Иванович",
                dokkomplekt_core::ValueSource::UserConfirmed,
                1.0,
            ),
        );
        let pack = DocumentPack {
            pack_id: "atomic-pack".into(),
            ..DocumentPack::default()
        };
        let commercial = serde_json::json!({"plan":"doctor_pro","active":true});

        repo.save_desktop_snapshot(
            "current",
            "default",
            &case,
            &pack,
            "license_document",
            &commercial,
        )
        .unwrap();

        assert_eq!(repo.load_case("current").unwrap(), Some(case));
        assert_eq!(repo.load_pack("default").unwrap(), Some(pack));
        assert_eq!(
            repo.load_state_value::<serde_json::Value>("license_document")
                .unwrap(),
            Some(commercial)
        );
        assert!(repo.quick_integrity_check().is_ok());
        let _ = std::fs::remove_file(path);
    }
}
