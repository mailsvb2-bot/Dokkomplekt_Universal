#![allow(dead_code)]

use crate::providers::{
    CreatePaymentRequest, CreatePaymentResponse, PaymentProvider, ProviderError, ProviderEvent,
};

#[derive(Debug, Clone)]
pub struct SbpProvider {
    pub public_base_url: String,
}

impl PaymentProvider for SbpProvider {
    fn create_payment(
        &self,
        _request: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, ProviderError> {
        let _ = &self.public_base_url;
        Err(ProviderError::Unsupported)
    }

    fn parse_callback(&self, _raw_body: &[u8]) -> Result<ProviderEvent, ProviderError> {
        Err(ProviderError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sbp_stub_fails_closed_until_a_verified_bank_integration_exists() {
        let provider = SbpProvider {
            public_base_url: "https://licenses.example.org".into(),
        };
        let request = CreatePaymentRequest {
            order_id: Uuid::nil(),
            amount_rub: 1_490,
            description: "test".into(),
            return_url: None,
        };
        assert!(matches!(
            provider.create_payment(request),
            Err(ProviderError::Unsupported)
        ));
        assert!(matches!(
            provider.parse_callback(br#"{}"#),
            Err(ProviderError::Unsupported)
        ));
    }
}
