#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentRequest {
    pub order_id: Uuid,
    pub amount_rub: u64,
    pub description: String,
    pub return_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentResponse {
    pub provider: ProviderKind,
    pub provider_payment_id: String,
    pub confirmation_url: String,
    pub qr_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEvent {
    pub provider: ProviderKind,
    pub provider_event_id: String,
    pub provider_payment_id: Option<String>,
    pub order_id: Uuid,
    pub status: ProviderPaymentStatus,
    pub amount_rub: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Manual,
    YooKassa,
    Sbp,
    BankInvoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPaymentStatus {
    Pending,
    Succeeded,
    Cancelled,
    Rejected,
}

pub trait PaymentProvider: Send + Sync + 'static {
    fn create_payment(
        &self,
        request: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, ProviderError>;
    fn parse_callback(&self, raw_body: &[u8]) -> Result<ProviderEvent, ProviderError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    BadRequest(String),
    BadSignature,
    Transport(String),
    Unsupported,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for ProviderError {}
