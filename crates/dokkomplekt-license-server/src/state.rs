use crate::config::ServerConfig;
use crate::storage::{AuditEventRecord, LicenseRecord, PaymentEventRecord, StoreBackend};
use crate::traffic_guard::TrafficGuard;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config: ServerConfig,
    pub store: StoreBackend,
    pub traffic_guard: TrafficGuard,
    pub provider_gate: Arc<Semaphore>,
}

impl AppState {
    pub fn try_new(config: ServerConfig) -> anyhow::Result<Self> {
        let store = StoreBackend::from_config(&config)?;
        let provider_gate = Arc::new(Semaphore::new(config.provider_concurrency_limit));
        Ok(Self {
            config,
            store,
            traffic_guard: TrafficGuard::default(),
            provider_gate,
        })
    }
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    pub orders: BTreeMap<Uuid, OrderRecord>,
    pub activations: BTreeMap<Uuid, ActivationRecord>,
    pub payment_events: BTreeMap<Uuid, PaymentEventRecord>,
    pub licenses: BTreeMap<Uuid, LicenseRecord>,
    pub audit_events: BTreeMap<Uuid, AuditEventRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub id: Uuid,
    pub plan: String,
    pub amount_rub: u64,
    pub status: OrderStatus,
    pub machine_hash: Option<String>,
    pub access_token_hash: Option<String>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Draft,
    WaitingPayment,
    Paid,
    LicenseIssued,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationRecord {
    pub id: Uuid,
    pub order_id: Uuid,
    pub machine_hash: String,
    pub created_at: OffsetDateTime,
}
