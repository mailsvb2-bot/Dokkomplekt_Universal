#![allow(dead_code)]

mod postgres;

pub use postgres::PostgresStore;

use crate::config::ServerConfig;
use crate::state::{ActivationRecord, MemoryStore, OrderRecord, OrderStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationIssueOutcome {
    pub order: OrderRecord,
    pub activation: ActivationRecord,
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentEventRecord {
    pub id: Uuid,
    pub order_id: Uuid,
    pub provider: PaymentProvider,
    pub provider_event_id: String,
    pub provider_payment_id: Option<String>,
    pub status: PaymentEventStatus,
    pub amount_rub: u64,
    pub received_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Manual,
    YooKassa,
    Sbp,
    BankInvoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentEventStatus {
    Pending,
    Succeeded,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentEventWriteOutcome {
    Recorded,
    Duplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicenseRecord {
    pub id: Uuid,
    pub order_id: Uuid,
    pub license_id: String,
    pub document_json: String,
    pub issued_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseIssueOutcome {
    pub record: LicenseRecord,
    pub reused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub event_type: String,
    pub happened_at: OffsetDateTime,
    pub details_json: String,
}

pub trait LicenseStore: Send + Sync + 'static {
    fn create_order(&self, record: OrderRecord) -> Result<(), StoreError>;
    fn get_order(&self, order_id: Uuid) -> Result<Option<OrderRecord>, StoreError>;
    fn recover_legacy_order_access(
        &self,
        order_id: Uuid,
        machine_hash: &str,
        access_token_hash: &str,
        bind_missing_machine: bool,
    ) -> Result<OrderRecord, StoreError>;
    fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<(), StoreError>;
    fn create_activation(&self, record: ActivationRecord) -> Result<(), StoreError>;
    fn create_activation_for_order(
        &self,
        record: ActivationRecord,
        max_machines: u32,
    ) -> Result<ActivationIssueOutcome, StoreError>;
    fn activations_for_order(&self, order_id: Uuid) -> Result<Vec<ActivationRecord>, StoreError>;
    fn record_payment_event(&self, record: PaymentEventRecord) -> Result<(), StoreError>;
    fn record_payment_event_for_order(
        &self,
        record: PaymentEventRecord,
    ) -> Result<PaymentEventWriteOutcome, StoreError>;
    fn store_license(&self, record: LicenseRecord) -> Result<(), StoreError>;
    fn audit(&self, record: AuditEventRecord) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Poisoned,
    NotFound,
    Conflict,
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StoreError {}

#[derive(Clone)]
pub enum StoreBackend {
    Memory(Arc<RwLock<MemoryStore>>),
    Postgres(PostgresStore),
}

impl StoreBackend {
    pub fn from_config(config: &ServerConfig) -> anyhow::Result<Self> {
        Ok(
            match config
                .database_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(url) => Self::Postgres(PostgresStore::connect(url)?),
                None => Self::Memory(Arc::new(RwLock::new(MemoryStore::default()))),
            },
        )
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Postgres(_) => "postgres",
        }
    }

    pub fn database_connected(&self) -> bool {
        matches!(self, Self::Postgres(_))
    }

    pub async fn database_ready_async(&self) -> bool {
        match self {
            Self::Memory(_) => false,
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.check_ready().is_ok())
                    .await
                    .unwrap_or(false)
            }
        }
    }

    pub async fn create_order_async(&self, record: OrderRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.create_order(record),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.create_order(record))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn get_order_async(&self, order_id: Uuid) -> Result<Option<OrderRecord>, StoreError> {
        match self {
            Self::Memory(store) => store.get_order(order_id),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.get_order(order_id))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn recover_legacy_order_access_async(
        &self,
        order_id: Uuid,
        machine_hash: String,
        access_token_hash: String,
        bind_missing_machine: bool,
    ) -> Result<OrderRecord, StoreError> {
        match self {
            Self::Memory(store) => store.recover_legacy_order_access(
                order_id,
                &machine_hash,
                &access_token_hash,
                bind_missing_machine,
            ),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || {
                    store.recover_legacy_order_access(
                        order_id,
                        &machine_hash,
                        &access_token_hash,
                        bind_missing_machine,
                    )
                })
                .await
                .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn update_order_status_async(
        &self,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.update_order_status(order_id, status),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.update_order_status(order_id, status))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn create_activation_for_order_async(
        &self,
        record: ActivationRecord,
        max_machines: u32,
    ) -> Result<ActivationIssueOutcome, StoreError> {
        match self {
            Self::Memory(store) => store.create_activation_for_order(record, max_machines),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || {
                    store.create_activation_for_order(record, max_machines)
                })
                .await
                .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn activations_for_order_async(
        &self,
        order_id: Uuid,
    ) -> Result<Vec<ActivationRecord>, StoreError> {
        match self {
            Self::Memory(store) => store.activations_for_order(order_id),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.activations_for_order(order_id))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn record_payment_event_for_order_async(
        &self,
        record: PaymentEventRecord,
    ) -> Result<PaymentEventWriteOutcome, StoreError> {
        match self {
            Self::Memory(store) => store.record_payment_event_for_order(record),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.record_payment_event_for_order(record))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn store_license_async(&self, record: LicenseRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.store_license(record),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.store_license(record))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }

    pub async fn issue_license_for_paid_order_async(
        &self,
        record: LicenseRecord,
    ) -> Result<LicenseIssueOutcome, StoreError> {
        match self {
            Self::Memory(store) => issue_license_for_memory(store, record),
            Self::Postgres(store) => {
                let store = store.clone();
                tokio::task::spawn_blocking(move || store.issue_license_for_paid_order(record))
                    .await
                    .map_err(|_| StoreError::Poisoned)?
            }
        }
    }
}

impl LicenseStore for StoreBackend {
    fn create_order(&self, record: OrderRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.create_order(record),
            Self::Postgres(store) => store.create_order(record),
        }
    }

    fn get_order(&self, order_id: Uuid) -> Result<Option<OrderRecord>, StoreError> {
        match self {
            Self::Memory(store) => store.get_order(order_id),
            Self::Postgres(store) => store.get_order(order_id),
        }
    }

    fn recover_legacy_order_access(
        &self,
        order_id: Uuid,
        machine_hash: &str,
        access_token_hash: &str,
        bind_missing_machine: bool,
    ) -> Result<OrderRecord, StoreError> {
        match self {
            Self::Memory(store) => store.recover_legacy_order_access(
                order_id,
                machine_hash,
                access_token_hash,
                bind_missing_machine,
            ),
            Self::Postgres(store) => store.recover_legacy_order_access(
                order_id,
                machine_hash,
                access_token_hash,
                bind_missing_machine,
            ),
        }
    }

    fn update_order_status(&self, order_id: Uuid, status: OrderStatus) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.update_order_status(order_id, status),
            Self::Postgres(store) => store.update_order_status(order_id, status),
        }
    }

    fn create_activation(&self, record: ActivationRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.create_activation(record),
            Self::Postgres(store) => store.create_activation(record),
        }
    }

    fn create_activation_for_order(
        &self,
        record: ActivationRecord,
        max_machines: u32,
    ) -> Result<ActivationIssueOutcome, StoreError> {
        match self {
            Self::Memory(store) => store.create_activation_for_order(record, max_machines),
            Self::Postgres(store) => store.create_activation_for_order(record, max_machines),
        }
    }

    fn activations_for_order(&self, order_id: Uuid) -> Result<Vec<ActivationRecord>, StoreError> {
        match self {
            Self::Memory(store) => store.activations_for_order(order_id),
            Self::Postgres(store) => store.activations_for_order(order_id),
        }
    }

    fn record_payment_event(&self, record: PaymentEventRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.record_payment_event(record),
            Self::Postgres(store) => store.record_payment_event(record),
        }
    }

    fn record_payment_event_for_order(
        &self,
        record: PaymentEventRecord,
    ) -> Result<PaymentEventWriteOutcome, StoreError> {
        match self {
            Self::Memory(store) => store.record_payment_event_for_order(record),
            Self::Postgres(store) => store.record_payment_event_for_order(record),
        }
    }

    fn store_license(&self, record: LicenseRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.store_license(record),
            Self::Postgres(store) => store.store_license(record),
        }
    }

    fn audit(&self, record: AuditEventRecord) -> Result<(), StoreError> {
        match self {
            Self::Memory(store) => store.audit(record),
            Self::Postgres(store) => store.audit(record),
        }
    }
}

pub(crate) fn validate_access_recovery_input(
    machine_hash: &str,
    access_token_hash: &str,
) -> Result<String, StoreError> {
    let machine_hash = machine_hash.trim();
    if machine_hash.is_empty()
        || machine_hash.len() > 256
        || machine_hash.chars().any(char::is_control)
    {
        return Err(StoreError::Invalid("invalid_machine_hash".to_string()));
    }
    let access_token_hash = access_token_hash.trim();
    if access_token_hash.len() != 64
        || !access_token_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(StoreError::Invalid("invalid_access_token_hash".to_string()));
    }
    Ok(machine_hash.to_string())
}

pub(crate) fn order_status_after_payment(
    current: &OrderStatus,
    payment: &PaymentEventStatus,
) -> Result<OrderStatus, StoreError> {
    match payment {
        PaymentEventStatus::Succeeded => match current {
            OrderStatus::Draft | OrderStatus::WaitingPayment => Ok(OrderStatus::Paid),
            OrderStatus::Paid => Ok(OrderStatus::Paid),
            OrderStatus::LicenseIssued => Ok(OrderStatus::LicenseIssued),
            OrderStatus::Cancelled => Err(StoreError::Invalid(
                "payment_succeeded_after_cancellation".to_string(),
            )),
        },
        PaymentEventStatus::Pending => Ok(current.clone()),
        PaymentEventStatus::Cancelled | PaymentEventStatus::Rejected => match current {
            OrderStatus::Draft | OrderStatus::WaitingPayment => Ok(OrderStatus::Cancelled),
            OrderStatus::Paid => Ok(OrderStatus::Paid),
            OrderStatus::LicenseIssued => Ok(OrderStatus::LicenseIssued),
            OrderStatus::Cancelled => Ok(OrderStatus::Cancelled),
        },
    }
}

pub(crate) fn validate_order_status_transition(
    current: &OrderStatus,
    requested: &OrderStatus,
) -> Result<(), StoreError> {
    let allowed = current == requested
        || matches!(
            (current, requested),
            (OrderStatus::Draft, OrderStatus::WaitingPayment)
                | (OrderStatus::Draft, OrderStatus::Cancelled)
                | (OrderStatus::WaitingPayment, OrderStatus::Paid)
                | (OrderStatus::WaitingPayment, OrderStatus::Cancelled)
                | (OrderStatus::Paid, OrderStatus::LicenseIssued)
        );
    allowed
        .then_some(())
        .ok_or_else(|| StoreError::Invalid("non_monotonic_order_status_transition".to_string()))
}

fn issue_license_for_memory(
    store: &Arc<RwLock<MemoryStore>>,
    record: LicenseRecord,
) -> Result<LicenseIssueOutcome, StoreError> {
    let mut store = store.write().map_err(|_| StoreError::Poisoned)?;
    let status = store
        .orders
        .get(&record.order_id)
        .ok_or(StoreError::NotFound)?
        .status
        .clone();
    if !matches!(status, OrderStatus::Paid | OrderStatus::LicenseIssued) {
        return Err(StoreError::Conflict);
    }
    if let Some(existing) = store
        .licenses
        .values()
        .filter(|license| license.order_id == record.order_id && license.revoked_at.is_none())
        .max_by_key(|license| license.issued_at)
        .cloned()
    {
        if license_documents_equivalent(&existing.document_json, &record.document_json) {
            return Ok(LicenseIssueOutcome {
                record: existing,
                reused: true,
            });
        }
    }
    if store.licenses.contains_key(&record.id)
        || store
            .licenses
            .values()
            .any(|existing| existing.license_id == record.license_id)
    {
        return Err(StoreError::Conflict);
    }
    let order = store
        .orders
        .get_mut(&record.order_id)
        .ok_or(StoreError::NotFound)?;
    order.status = OrderStatus::LicenseIssued;
    store.licenses.insert(record.id, record.clone());
    Ok(LicenseIssueOutcome {
        record,
        reused: false,
    })
}

fn license_documents_equivalent(existing_json: &str, requested_json: &str) -> bool {
    match (
        license_machine_set(existing_json),
        license_machine_set(requested_json),
    ) {
        (Some(existing), Some(requested)) => existing == requested,
        (None, None) => canonical_json(existing_json) == canonical_json(requested_json),
        _ => false,
    }
}

fn canonical_json(document_json: &str) -> Option<serde_json::Value> {
    serde_json::from_str(document_json).ok()
}

fn license_machine_set(document_json: &str) -> Option<BTreeSet<String>> {
    let machines = serde_json::from_str::<serde_json::Value>(document_json)
        .ok()?
        .pointer("/license/payload/allowed_machines")?
        .as_array()?
        .iter()
        .filter_map(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    (!machines.is_empty()).then_some(machines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ActivationRecord, OrderRecord, OrderStatus};

    fn order_record(id: Uuid, status: OrderStatus) -> OrderRecord {
        OrderRecord {
            id,
            plan: "doctor_pro".to_string(),
            amount_rub: 3900,
            status,
            machine_hash: None,
            access_token_hash: Some("a".repeat(64)),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    fn payment_event(order_id: Uuid, provider_event_id: String) -> PaymentEventRecord {
        PaymentEventRecord {
            id: Uuid::new_v4(),
            order_id,
            provider: PaymentProvider::Manual,
            provider_event_id,
            provider_payment_id: None,
            status: PaymentEventStatus::Succeeded,
            amount_rub: 3900,
            received_at: OffsetDateTime::now_utc(),
        }
    }

    fn activation(order_id: Uuid, machine_hash: &str) -> ActivationRecord {
        ActivationRecord {
            id: Uuid::new_v4(),
            order_id,
            machine_hash: machine_hash.to_string(),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    fn license_record(order_id: Uuid, license_id: &str) -> LicenseRecord {
        LicenseRecord {
            id: Uuid::new_v4(),
            order_id,
            license_id: license_id.to_string(),
            document_json: "{}".to_string(),
            issued_at: OffsetDateTime::now_utc(),
            revoked_at: None,
        }
    }

    fn assert_license_store_contract(store: StoreBackend) {
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::WaitingPayment))
            .unwrap();
        assert!(matches!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::WaitingPayment
        ));

        let event_id = format!("evt-{order_id}");
        let event = payment_event(order_id, event_id);
        assert_eq!(
            store.record_payment_event_for_order(event.clone()).unwrap(),
            PaymentEventWriteOutcome::Recorded
        );
        assert_eq!(
            store
                .record_payment_event_for_order(PaymentEventRecord {
                    id: Uuid::new_v4(),
                    ..event
                })
                .unwrap(),
            PaymentEventWriteOutcome::Duplicate,
        );
        assert!(matches!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::Paid
        ));

        let first = store
            .create_activation_for_order(activation(order_id, "machine-a"), 1)
            .unwrap();
        assert!(!first.reused);
        let repeated = store
            .create_activation_for_order(activation(order_id, "machine-a"), 1)
            .unwrap();
        assert!(repeated.reused);
        assert_eq!(repeated.activation.id, first.activation.id);
        assert_eq!(store.activations_for_order(order_id).unwrap().len(), 1);
        assert_eq!(
            store
                .create_activation_for_order(activation(order_id, "machine-b"), 1)
                .unwrap_err(),
            StoreError::Conflict
        );

        let unpaid_order_id = Uuid::new_v4();
        store
            .create_order(order_record(unpaid_order_id, OrderStatus::WaitingPayment))
            .unwrap();
        assert_eq!(
            store
                .create_activation_for_order(activation(unpaid_order_id, "machine-c"), 1)
                .unwrap_err(),
            StoreError::Conflict
        );

        let license_id = format!("license-{order_id}");
        let license = license_record(order_id, &license_id);
        store.store_license(license.clone()).unwrap();
        assert_eq!(
            store
                .store_license(LicenseRecord {
                    id: Uuid::new_v4(),
                    ..license
                })
                .unwrap_err(),
            StoreError::Conflict,
        );

        let audit = AuditEventRecord {
            id: Uuid::new_v4(),
            entity_id: order_id,
            event_type: "license_store_contract".to_string(),
            happened_at: OffsetDateTime::now_utc(),
            details_json: "{}".to_string(),
        };
        store.audit(audit.clone()).unwrap();
        assert_eq!(store.audit(audit).unwrap_err(), StoreError::Conflict);
    }

    #[test]
    fn memory_backend_obeys_license_store_contract() {
        assert_license_store_contract(StoreBackend::Memory(Arc::new(RwLock::new(
            MemoryStore::default(),
        ))));
    }

    #[test]
    fn memory_license_issue_is_atomic_and_idempotent() {
        let store = Arc::new(RwLock::new(MemoryStore::default()));
        let order_id = Uuid::new_v4();
        store
            .create_order(order_record(order_id, OrderStatus::Paid))
            .unwrap();
        let issued =
            issue_license_for_memory(&store, license_record(order_id, "license-issued")).unwrap();
        assert!(!issued.reused);
        assert_eq!(issued.record.license_id, "license-issued");
        assert!(matches!(
            store.get_order(order_id).unwrap().unwrap().status,
            OrderStatus::LicenseIssued
        ));
        let reused =
            issue_license_for_memory(&store, license_record(order_id, "license-new-but-ignored"))
                .unwrap();
        assert!(reused.reused);
        assert_eq!(reused.record.license_id, "license-issued");
    }

    #[test]
    fn postgres_backend_obeys_license_store_contract_when_database_url_is_present() {
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            return;
        };
        let store = StoreBackend::Postgres(PostgresStore::connect(&database_url).unwrap());
        assert_license_store_contract(store);
    }
}
