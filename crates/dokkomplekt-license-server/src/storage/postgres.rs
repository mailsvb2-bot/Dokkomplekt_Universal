use super::{
    order_status_after_payment, validate_access_recovery_input, validate_order_status_transition,
    ActivationIssueOutcome, AuditEventRecord, LicenseIssueOutcome, LicenseRecord, LicenseStore,
    PaymentEventRecord, PaymentEventStatus, PaymentEventWriteOutcome, StoreError,
};
use crate::state::{ActivationRecord, OrderRecord, OrderStatus};
use postgres::error::SqlState;
use postgres::{Client, GenericClient, NoTls, Row};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresStore {
    pool: Arc<PostgresPool>,
}

struct PostgresPool {
    clients: Vec<Mutex<Client>>,
    next: AtomicUsize,
}

impl PostgresStore {
    pub fn connect(database_url: &str) -> anyhow::Result<Self> {
        crate::config::validate_database_transport(
            database_url,
            crate::config::strict_runtime_required(),
        )?;
        let pool_size = configured_pool_size();
        let mut clients = Vec::with_capacity(pool_size);
        let mut bootstrap_client = Client::connect(database_url, NoTls)?;
        apply_migrations(&mut bootstrap_client)?;
        clients.push(Mutex::new(bootstrap_client));
        for _ in 1..pool_size {
            clients.push(Mutex::new(Client::connect(database_url, NoTls)?));
        }
        Ok(Self {
            pool: Arc::new(PostgresPool {
                clients,
                next: AtomicUsize::new(0),
            }),
        })
    }

    pub fn pool_size(&self) -> usize {
        self.pool.clients.len()
    }

    pub fn check_ready(&self) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let value: i32 = client.query_one("SELECT 1", &[]).map_err(pg_err)?.get(0);
        if value == 1 {
            Ok(())
        } else {
            Err(StoreError::Invalid("postgres_readiness_failed".to_string()))
        }
    }

    pub fn issue_license_for_paid_order(
        &self,
        record: LicenseRecord,
    ) -> Result<LicenseIssueOutcome, StoreError> {
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(pg_err)?;
        let row = tx.query_opt(
            "SELECT id, plan, amount_rub, status, machine_hash, access_token_hash, created_at FROM license_orders WHERE id = $1 FOR UPDATE",
            &[&record.order_id],
        ).map_err(pg_err)?.ok_or(StoreError::NotFound)?;
        let order = order_from_row(row)?;
        if !matches!(order.status, OrderStatus::Paid | OrderStatus::LicenseIssued) {
            return Err(StoreError::Conflict);
        }
        if let Some(row) = tx.query_opt(
            "SELECT id, order_id, license_id, document_json, issued_at, revoked_at FROM license_documents WHERE order_id = $1 AND revoked_at IS NULL ORDER BY issued_at DESC LIMIT 1",
            &[&record.order_id],
        ).map_err(pg_err)? {
            let existing = license_from_row(row);
            if super::license_documents_equivalent(
                &existing.document_json,
                &record.document_json,
            ) {
                insert_audit_event(
                    &mut tx,
                    &audit_event(record.order_id, "license_issue_reused", &existing.license_id, OffsetDateTime::now_utc()),
                )?;
                tx.commit().map_err(pg_err)?;
                return Ok(LicenseIssueOutcome { record: existing, reused: true });
            }
        }
        tx.execute(
            "INSERT INTO license_documents (id, order_id, license_id, document_json, issued_at, revoked_at) VALUES ($1, $2, $3, $4, $5, $6)",
            &[&record.id, &record.order_id, &record.license_id, &record.document_json, &record.issued_at, &record.revoked_at],
        ).map_err(pg_err)?;
        let issued = order_status_to_str(&OrderStatus::LicenseIssued);
        tx.execute(
            "UPDATE license_orders SET status = $2 WHERE id = $1",
            &[&record.order_id, &issued],
        )
        .map_err(pg_err)?;
        insert_audit_event(
            &mut tx,
            &audit_event(
                record.order_id,
                "license_issued",
                &record.license_id,
                record.issued_at,
            ),
        )?;
        tx.commit().map_err(pg_err)?;
        Ok(LicenseIssueOutcome {
            record,
            reused: false,
        })
    }

    fn client(&self) -> Result<MutexGuard<'_, Client>, StoreError> {
        let index = self.pool.next.fetch_add(1, Ordering::Relaxed) % self.pool.clients.len();
        self.pool.clients[index]
            .lock()
            .map_err(|_| StoreError::Poisoned)
    }
}

impl LicenseStore for PostgresStore {
    fn create_order(&self, record: OrderRecord) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let amount = amount_to_i64(record.amount_rub)?;
        let status = order_status_to_str(&record.status);
        let machine_hash = record.machine_hash.as_deref();
        let access_token_hash = record.access_token_hash.as_deref();
        client.execute(
            "INSERT INTO license_orders (id, plan, amount_rub, status, machine_hash, access_token_hash, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &record.id,
                &record.plan,
                &amount,
                &status,
                &machine_hash,
                &access_token_hash,
                &record.created_at,
            ],
        )
        .map_err(pg_err)?;
        Ok(())
    }

    fn get_order(&self, order_id: Uuid) -> Result<Option<OrderRecord>, StoreError> {
        let mut client = self.client()?;
        client.query_opt(
            "SELECT id, plan, amount_rub, status, machine_hash, access_token_hash, created_at FROM license_orders WHERE id = $1",
            &[&order_id],
        ).map_err(pg_err)?.map(order_from_row).transpose()
    }

    fn recover_legacy_order_access(
        &self,
        order_id: Uuid,
        machine_hash: &str,
        access_token_hash: &str,
        bind_missing_machine: bool,
    ) -> Result<OrderRecord, StoreError> {
        let machine_hash = validate_access_recovery_input(machine_hash, access_token_hash)?;
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(pg_err)?;
        let row = tx
            .query_opt(
                "SELECT id, plan, amount_rub, status, machine_hash, access_token_hash, created_at FROM license_orders WHERE id = $1 FOR UPDATE",
                &[&order_id],
            )
            .map_err(pg_err)?
            .ok_or(StoreError::NotFound)?;
        let mut order = order_from_row(row)?;
        if order.access_token_hash.is_some() || matches!(order.status, OrderStatus::Cancelled) {
            return Err(StoreError::Conflict);
        }
        let bound_missing_machine = match order.machine_hash.as_deref().map(str::trim) {
            Some(expected) if expected == machine_hash => false,
            Some(_) => return Err(StoreError::Conflict),
            None if bind_missing_machine => true,
            None => {
                return Err(StoreError::Invalid(
                    "legacy_order_has_no_machine_binding".to_string(),
                ))
            }
        };
        let stored_machine_hash = if bound_missing_machine {
            machine_hash.clone()
        } else {
            order
                .machine_hash
                .clone()
                .unwrap_or_else(|| machine_hash.clone())
        };
        let updated = tx
            .execute(
                "UPDATE license_orders SET machine_hash = $2, access_token_hash = $3 WHERE id = $1 AND access_token_hash IS NULL",
                &[&order_id, &stored_machine_hash, &access_token_hash.trim()],
            )
            .map_err(pg_err)?;
        if updated != 1 {
            return Err(StoreError::Conflict);
        }
        insert_audit_event(
            &mut tx,
            &AuditEventRecord {
                id: Uuid::new_v4(),
                entity_id: order_id,
                event_type: "order_access_recovered".to_string(),
                happened_at: OffsetDateTime::now_utc(),
                details_json: serde_json::json!({
                    "bound_missing_machine": bound_missing_machine,
                })
                .to_string(),
            },
        )?;
        tx.commit().map_err(pg_err)?;
        order.machine_hash = Some(stored_machine_hash);
        order.access_token_hash = Some(access_token_hash.trim().to_string());
        Ok(order)
    }

    fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<(), StoreError> {
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(pg_err)?;
        let current: String = tx
            .query_opt(
                "SELECT status FROM license_orders WHERE id = $1 FOR UPDATE",
                &[&order_id],
            )
            .map_err(pg_err)?
            .ok_or(StoreError::NotFound)?
            .get(0);
        let current = order_status_from_str(&current)?;
        validate_order_status_transition(&current, &status)?;
        let status = order_status_to_str(&status);
        tx.execute(
            "UPDATE license_orders SET status = $2 WHERE id = $1",
            &[&order_id, &status],
        )
        .map_err(pg_err)?;
        tx.commit().map_err(pg_err)
    }

    fn create_activation(&self, record: ActivationRecord) -> Result<(), StoreError> {
        let mut client = self.client()?;
        client.execute(
            "INSERT INTO license_machines (id, order_id, machine_hash, created_at) VALUES ($1, $2, $3, $4)",
            &[&record.id, &record.order_id, &record.machine_hash, &record.created_at],
        ).map_err(pg_err)?;
        Ok(())
    }

    fn create_activation_for_order(
        &self,
        record: ActivationRecord,
        max_machines: u32,
    ) -> Result<ActivationIssueOutcome, StoreError> {
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(pg_err)?;
        let row = tx.query_opt(
            "SELECT id, plan, amount_rub, status, machine_hash, access_token_hash, created_at FROM license_orders WHERE id = $1 FOR UPDATE",
            &[&record.order_id],
        ).map_err(pg_err)?.ok_or(StoreError::NotFound)?;
        let order = order_from_row(row)?;
        if !matches!(order.status, OrderStatus::Paid | OrderStatus::LicenseIssued) {
            return Err(StoreError::Conflict);
        }
        if let Some(row) = tx
            .query_opt(
                "SELECT id, order_id, machine_hash, created_at FROM license_machines WHERE order_id = $1 AND machine_hash = $2",
                &[&record.order_id, &record.machine_hash],
            )
            .map_err(pg_err)?
        {
            let activation = activation_from_row(row);
            insert_audit_event(
                &mut tx,
                &audit_event(
                    record.order_id,
                    "machine_activation_reused",
                    &record.machine_hash,
                    OffsetDateTime::now_utc(),
                ),
            )?;
            tx.commit().map_err(pg_err)?;
            return Ok(ActivationIssueOutcome {
                order,
                activation,
                reused: true,
            });
        }
        let active_count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM license_machines WHERE order_id = $1",
                &[&record.order_id],
            )
            .map_err(pg_err)?
            .get(0);
        if active_count == 0
            && order
                .machine_hash
                .as_deref()
                .is_some_and(|expected| expected != record.machine_hash)
        {
            return Err(StoreError::Conflict);
        }
        if active_count < 0 || active_count as u32 >= max_machines {
            return Err(StoreError::Conflict);
        }
        tx.execute(
            "INSERT INTO license_machines (id, order_id, machine_hash, created_at) VALUES ($1, $2, $3, $4)",
            &[&record.id, &record.order_id, &record.machine_hash, &record.created_at],
        ).map_err(pg_err)?;
        insert_audit_event(
            &mut tx,
            &audit_event(
                record.order_id,
                "machine_activated",
                &record.machine_hash,
                record.created_at,
            ),
        )?;
        tx.commit().map_err(pg_err)?;
        Ok(ActivationIssueOutcome {
            order,
            activation: record,
            reused: false,
        })
    }

    fn activations_for_order(&self, order_id: Uuid) -> Result<Vec<ActivationRecord>, StoreError> {
        let mut client = self.client()?;
        let exists = client
            .query_opt("SELECT 1 FROM license_orders WHERE id = $1", &[&order_id])
            .map_err(pg_err)?
            .is_some();
        if !exists {
            return Err(StoreError::NotFound);
        }
        client
            .query(
                "SELECT id, order_id, machine_hash, created_at FROM license_machines WHERE order_id = $1 ORDER BY created_at ASC, machine_hash ASC",
                &[&order_id],
            )
            .map_err(pg_err)
            .map(|rows| rows.into_iter().map(activation_from_row).collect())
    }

    fn record_payment_event(&self, record: PaymentEventRecord) -> Result<(), StoreError> {
        let mut client = self.client()?;
        insert_payment_event(&mut *client, &record)
    }

    fn record_payment_event_for_order(
        &self,
        record: PaymentEventRecord,
    ) -> Result<PaymentEventWriteOutcome, StoreError> {
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(pg_err)?;
        let row = tx.query_opt(
            "SELECT id, plan, amount_rub, status, machine_hash, access_token_hash, created_at FROM license_orders WHERE id = $1 FOR UPDATE",
            &[&record.order_id],
        ).map_err(pg_err)?.ok_or(StoreError::NotFound)?;
        let order = order_from_row(row)?;
        if order.amount_rub != record.amount_rub {
            return Err(StoreError::Invalid("amount_mismatch".to_string()));
        }
        if !try_insert_payment_event(&mut tx, &record)? {
            let provider = payment_provider_to_str(&record.provider);
            let existing_order_id: Uuid = tx
                .query_one(
                    "SELECT order_id FROM billing_events WHERE provider = $1 AND provider_event_id = $2",
                    &[&provider, &record.provider_event_id],
                )
                .map_err(pg_err)?
                .get(0);
            if existing_order_id != record.order_id {
                return Err(StoreError::Invalid(
                    "provider_event_order_mismatch".to_string(),
                ));
            }
            insert_audit_event(
                &mut tx,
                &audit_event(
                    record.order_id,
                    "payment_duplicate",
                    &record.provider_event_id,
                    record.received_at,
                ),
            )?;
            tx.commit().map_err(pg_err)?;
            return Ok(PaymentEventWriteOutcome::Duplicate);
        }
        let next_status = order_status_after_payment(&order.status, &record.status)?;
        if next_status != order.status {
            let next_status = order_status_to_str(&next_status);
            tx.execute(
                "UPDATE license_orders SET status = $2 WHERE id = $1",
                &[&record.order_id, &next_status],
            )
            .map_err(pg_err)?;
        }
        insert_audit_event(
            &mut tx,
            &audit_event(
                record.order_id,
                "payment_recorded",
                &record.provider_event_id,
                record.received_at,
            ),
        )?;
        tx.commit().map_err(pg_err)?;
        Ok(PaymentEventWriteOutcome::Recorded)
    }

    fn store_license(&self, record: LicenseRecord) -> Result<(), StoreError> {
        let mut client = self.client()?;
        client.execute(
            "INSERT INTO license_documents (id, order_id, license_id, document_json, issued_at, revoked_at) VALUES ($1, $2, $3, $4, $5, $6)",
            &[&record.id, &record.order_id, &record.license_id, &record.document_json, &record.issued_at, &record.revoked_at],
        ).map_err(pg_err)?;
        Ok(())
    }

    fn audit(&self, record: AuditEventRecord) -> Result<(), StoreError> {
        let mut client = self.client()?;
        insert_audit_event(&mut *client, &record)
    }
}

fn apply_migrations(client: &mut Client) -> anyhow::Result<()> {
    client.query_one("SELECT pg_advisory_lock($1)", &[&MIGRATION_LOCK_ID])?;
    let result = apply_migrations_locked(client);
    let unlock_result = client.query_one("SELECT pg_advisory_unlock($1)", &[&MIGRATION_LOCK_ID]);
    result?;
    unlock_result?;
    Ok(())
}

fn apply_migrations_locked(client: &mut Client) -> anyhow::Result<()> {
    client.batch_execute(MIGRATION_LEDGER_SCHEMA)?;
    client.batch_execute(MIGRATION_LEDGER_CHECKSUM_COLUMN)?;
    apply_migration(client, SCHEMA_V1_VERSION, SCHEMA_V1)?;
    apply_migration(client, SCHEMA_V2_VERSION, SCHEMA_V2)?;
    Ok(())
}

fn apply_migration(client: &mut Client, version: &str, sql: &str) -> anyhow::Result<()> {
    let checksum = schema_checksum(sql);
    let existing = client.query_opt(
        "SELECT checksum FROM schema_migrations WHERE version = $1",
        &[&version],
    )?;
    if let Some(row) = existing {
        let stored_checksum: Option<String> = row.get(0);
        match stored_checksum.as_deref() {
            Some(value) if value == checksum => return Ok(()),
            Some(value) => anyhow::bail!(
                "migration checksum mismatch for {version}: stored {value}, expected {checksum}"
            ),
            None => {
                client.execute(
                    "UPDATE schema_migrations SET checksum = $2 WHERE version = $1",
                    &[&version, &checksum],
                )?;
                return Ok(());
            }
        }
    }
    client.batch_execute(sql)?;
    client.execute(
        "INSERT INTO schema_migrations (version, checksum, applied_at) VALUES ($1, $2, NOW())",
        &[&version, &checksum],
    )?;
    Ok(())
}

fn schema_checksum(sql: &str) -> String {
    let digest = Sha256::digest(sql.as_bytes());
    hex::encode(digest)
}

fn configured_pool_size() -> usize {
    configured_pool_size_from(std::env::var(POSTGRES_POOL_SIZE_ENV).ok().as_deref())
}

fn configured_pool_size_from(value: Option<&str>) -> usize {
    value
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_POSTGRES_POOL_SIZE)
        .clamp(MIN_POSTGRES_POOL_SIZE, MAX_POSTGRES_POOL_SIZE)
}

fn try_insert_payment_event(
    client: &mut impl GenericClient,
    record: &PaymentEventRecord,
) -> Result<bool, StoreError> {
    let provider = payment_provider_to_str(&record.provider);
    let status = payment_status_to_str(&record.status);
    let amount = amount_to_i64(record.amount_rub)?;
    let provider_ref = record.provider_payment_id.as_deref();
    let inserted = client
        .execute(
            "INSERT INTO billing_events (id, order_id, provider, provider_event_id, provider_reference_id, status, amount_rub, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (provider, provider_event_id) DO NOTHING",
            &[&record.id, &record.order_id, &provider, &record.provider_event_id, &provider_ref, &status, &amount, &record.received_at],
        )
        .map_err(pg_err)?;
    Ok(inserted == 1)
}

fn insert_payment_event(
    client: &mut impl GenericClient,
    record: &PaymentEventRecord,
) -> Result<(), StoreError> {
    let provider = payment_provider_to_str(&record.provider);
    let status = payment_status_to_str(&record.status);
    let amount = amount_to_i64(record.amount_rub)?;
    let provider_ref = record.provider_payment_id.as_deref();
    client.execute(
        "INSERT INTO billing_events (id, order_id, provider, provider_event_id, provider_reference_id, status, amount_rub, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &[&record.id, &record.order_id, &provider, &record.provider_event_id, &provider_ref, &status, &amount, &record.received_at],
    ).map_err(pg_err)?;
    Ok(())
}

fn insert_audit_event(
    client: &mut impl GenericClient,
    record: &AuditEventRecord,
) -> Result<(), StoreError> {
    client.execute(
        "INSERT INTO license_audit_events (id, entity_id, event_type, happened_at, details_json) VALUES ($1, $2, $3, $4, $5)",
        &[&record.id, &record.entity_id, &record.event_type, &record.happened_at, &record.details_json],
    ).map_err(pg_err)?;
    Ok(())
}

fn audit_event(
    entity_id: Uuid,
    event_type: &str,
    detail_value: &str,
    happened_at: OffsetDateTime,
) -> AuditEventRecord {
    AuditEventRecord {
        id: Uuid::new_v4(),
        entity_id,
        event_type: event_type.to_string(),
        happened_at,
        details_json: serde_json::json!({"value": detail_value}).to_string(),
    }
}

fn order_from_row(row: Row) -> Result<OrderRecord, StoreError> {
    let amount: i64 = row.get("amount_rub");
    let status: String = row.get("status");
    Ok(OrderRecord {
        id: row.get("id"),
        plan: row.get("plan"),
        amount_rub: amount_from_i64(amount)?,
        status: order_status_from_str(&status)?,
        machine_hash: row.get("machine_hash"),
        access_token_hash: row.get("access_token_hash"),
        created_at: row.get("created_at"),
    })
}

fn activation_from_row(row: Row) -> ActivationRecord {
    ActivationRecord {
        id: row.get("id"),
        order_id: row.get("order_id"),
        machine_hash: row.get("machine_hash"),
        created_at: row.get("created_at"),
    }
}

fn license_from_row(row: Row) -> LicenseRecord {
    LicenseRecord {
        id: row.get("id"),
        order_id: row.get("order_id"),
        license_id: row.get("license_id"),
        document_json: row.get("document_json"),
        issued_at: row.get("issued_at"),
        revoked_at: row.get("revoked_at"),
    }
}

fn amount_to_i64(amount: u64) -> Result<i64, StoreError> {
    i64::try_from(amount).map_err(|_| StoreError::Invalid("amount_overflow".to_string()))
}

fn amount_from_i64(amount: i64) -> Result<u64, StoreError> {
    u64::try_from(amount).map_err(|_| StoreError::Invalid("amount_negative".to_string()))
}

fn order_status_to_str(status: &OrderStatus) -> &'static str {
    match status {
        OrderStatus::Draft => "draft",
        OrderStatus::WaitingPayment => "waiting_payment",
        OrderStatus::Paid => "paid",
        OrderStatus::LicenseIssued => "license_issued",
        OrderStatus::Cancelled => "cancelled",
    }
}

fn order_status_from_str(value: &str) -> Result<OrderStatus, StoreError> {
    match value {
        "draft" => Ok(OrderStatus::Draft),
        "waiting_payment" => Ok(OrderStatus::WaitingPayment),
        "paid" => Ok(OrderStatus::Paid),
        "license_issued" => Ok(OrderStatus::LicenseIssued),
        "cancelled" => Ok(OrderStatus::Cancelled),
        other => Err(StoreError::Invalid(format!("unknown_order_status:{other}"))),
    }
}

fn payment_provider_to_str(provider: &super::PaymentProvider) -> &'static str {
    match provider {
        super::PaymentProvider::Manual => "manual",
        super::PaymentProvider::YooKassa => "yookassa",
        super::PaymentProvider::Sbp => "sbp",
        super::PaymentProvider::BankInvoice => "bank_invoice",
    }
}

fn payment_status_to_str(status: &PaymentEventStatus) -> &'static str {
    match status {
        PaymentEventStatus::Pending => "pending",
        PaymentEventStatus::Succeeded => "succeeded",
        PaymentEventStatus::Cancelled => "cancelled",
        PaymentEventStatus::Rejected => "rejected",
    }
}

fn pg_err(error: postgres::Error) -> StoreError {
    if let Some(db_error) = error.as_db_error() {
        if db_error.code() == &SqlState::UNIQUE_VIOLATION {
            return StoreError::Conflict;
        }
        if db_error.code() == &SqlState::FOREIGN_KEY_VIOLATION {
            return StoreError::NotFound;
        }
    }
    StoreError::Invalid(error.to_string())
}

const POSTGRES_POOL_SIZE_ENV: &str = "DKK_LICENSE_POSTGRES_POOL_SIZE";
const DEFAULT_POSTGRES_POOL_SIZE: usize = 4;
const MIN_POSTGRES_POOL_SIZE: usize = 1;
const MAX_POSTGRES_POOL_SIZE: usize = 16;
const SCHEMA_V1_VERSION: &str = "0001_license_schema";
const SCHEMA_V2_VERSION: &str = "0002_order_access_token";
const MIGRATION_LOCK_ID: i64 = 4_207_301_001;
const MIGRATION_LEDGER_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY, checksum TEXT, applied_at TIMESTAMPTZ NOT NULL);";
const MIGRATION_LEDGER_CHECKSUM_COLUMN: &str =
    "ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS checksum TEXT;";
const SCHEMA_V1: &str = include_str!("../../migrations/0001_license_schema.sql");
const SCHEMA_V2: &str = include_str!("../../migrations/0002_order_access_token.sql");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_pool_size_uses_default_and_clamps_bounds() {
        assert_eq!(configured_pool_size_from(None), DEFAULT_POSTGRES_POOL_SIZE);
        assert_eq!(
            configured_pool_size_from(Some("")),
            DEFAULT_POSTGRES_POOL_SIZE
        );
        assert_eq!(
            configured_pool_size_from(Some("invalid")),
            DEFAULT_POSTGRES_POOL_SIZE
        );
        assert_eq!(configured_pool_size_from(Some("0")), MIN_POSTGRES_POOL_SIZE);
        assert_eq!(configured_pool_size_from(Some("1")), MIN_POSTGRES_POOL_SIZE);
        assert_eq!(configured_pool_size_from(Some("4")), 4);
        assert_eq!(
            configured_pool_size_from(Some("64")),
            MAX_POSTGRES_POOL_SIZE
        );
    }

    #[test]
    fn migration_checksum_is_hex_sha256() {
        let checksum = schema_checksum(SCHEMA_V1);
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[test]
    fn postgres_legacy_recovery_is_serialized_when_configured() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(database_url) = crate::config::postgres_test_database_url() else {
            return;
        };
        let store = PostgresStore::connect(&database_url).unwrap();
        let order_id = Uuid::new_v4();
        store
            .create_order(OrderRecord {
                id: order_id,
                plan: "doctor_pro".to_string(),
                amount_rub: 3_900,
                status: OrderStatus::Paid,
                machine_hash: Some("legacy-machine".to_string()),
                access_token_hash: None,
                created_at: OffsetDateTime::now_utc(),
            })
            .unwrap();

        let workers = 12;
        let barrier = Arc::new(Barrier::new(workers));
        let handles = (0..workers)
            .map(|index| {
                let store = store.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store
                        .recover_legacy_order_access(
                            order_id,
                            "legacy-machine",
                            &format!("{index:064x}"),
                            false,
                        )
                        .is_ok()
                })
            })
            .collect::<Vec<_>>();
        let successes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|success| *success)
            .count();
        assert_eq!(successes, 1);
        assert!(store
            .get_order(order_id)
            .unwrap()
            .unwrap()
            .access_token_hash
            .is_some());
    }

    #[test]
    fn postgres_duplicate_payment_event_is_idempotent_across_real_connections() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(database_url) = crate::config::postgres_test_database_url() else {
            return;
        };
        let store = PostgresStore::connect(&database_url).unwrap();
        let order_id = Uuid::new_v4();
        store
            .create_order(OrderRecord {
                id: order_id,
                plan: "doctor_pro".to_string(),
                amount_rub: 3_900,
                status: OrderStatus::WaitingPayment,
                machine_hash: Some("payment-race-machine".to_string()),
                access_token_hash: Some("b".repeat(64)),
                created_at: OffsetDateTime::now_utc(),
            })
            .unwrap();
        let provider_event_id = format!("evt-{}", Uuid::new_v4());
        let workers = 12;
        let barrier = Arc::new(Barrier::new(workers));
        let handles = (0..workers)
            .map(|index| {
                let store = store.clone();
                let barrier = barrier.clone();
                let provider_event_id = provider_event_id.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store.record_payment_event_for_order(PaymentEventRecord {
                        id: Uuid::new_v4(),
                        order_id,
                        provider: super::super::PaymentProvider::YooKassa,
                        provider_event_id,
                        provider_payment_id: Some(format!("payment-{index}")),
                        status: PaymentEventStatus::Succeeded,
                        amount_rub: 3_900,
                        received_at: OffsetDateTime::now_utc(),
                    })
                })
            })
            .collect::<Vec<_>>();
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, PaymentEventWriteOutcome::Recorded))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, PaymentEventWriteOutcome::Duplicate))
                .count(),
            workers - 1
        );
        assert!(matches!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::Paid
        ));
    }

    #[test]
    fn postgres_slot_limit_is_serialized_across_real_connections_when_configured() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let Some(database_url) = crate::config::postgres_test_database_url() else {
            return;
        };
        let store = PostgresStore::connect(&database_url).unwrap();
        let order_id = Uuid::new_v4();
        store
            .create_order(OrderRecord {
                id: order_id,
                plan: "doctor_pro".to_string(),
                amount_rub: 3_900,
                status: OrderStatus::Paid,
                machine_hash: None,
                access_token_hash: Some("a".repeat(64)),
                created_at: OffsetDateTime::now_utc(),
            })
            .unwrap();

        let workers = 12;
        let barrier = Arc::new(Barrier::new(workers));
        let mut handles = Vec::new();
        for index in 0..workers {
            let store = store.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                store.create_activation_for_order(
                    ActivationRecord {
                        id: Uuid::new_v4(),
                        order_id,
                        machine_hash: format!("contender-{index}"),
                        created_at: OffsetDateTime::now_utc(),
                    },
                    1,
                )
            }));
        }
        let successes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(Result::is_ok)
            .count();
        assert_eq!(successes, 1);
        assert_eq!(store.activations_for_order(order_id).unwrap().len(), 1);
    }
}
