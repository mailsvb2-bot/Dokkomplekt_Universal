#![allow(dead_code)]

use crate::providers::{
    CreatePaymentRequest, CreatePaymentResponse, PaymentProvider, ProviderError, ProviderEvent,
    ProviderKind, ProviderPaymentStatus,
};

#[derive(Debug, Clone)]
pub struct ManualProvider {
    pub public_base_url: String,
}

impl PaymentProvider for ManualProvider {
    fn create_payment(
        &self,
        request: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, ProviderError> {
        let payment_id = format!("manual-{}", request.order_id);
        Ok(CreatePaymentResponse {
            provider: ProviderKind::Manual,
            provider_payment_id: payment_id,
            confirmation_url: format!("{}/pay/manual/{}", self.public_base_url, request.order_id),
            qr_url: None,
        })
    }

    fn parse_callback(&self, raw_body: &[u8]) -> Result<ProviderEvent, ProviderError> {
        serde_json::from_slice::<ProviderEvent>(raw_body)
            .map_err(|err| ProviderError::BadRequest(err.to_string()))
    }
}

pub fn manual_succeeded_event(order_id: uuid::Uuid, amount_rub: u64) -> ProviderEvent {
    ProviderEvent {
        provider: ProviderKind::Manual,
        provider_event_id: format!("manual-event-{}", order_id),
        provider_payment_id: Some(format!("manual-{}", order_id)),
        order_id,
        status: ProviderPaymentStatus::Succeeded,
        amount_rub,
    }
}
