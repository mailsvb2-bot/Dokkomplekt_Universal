#![allow(dead_code)]

use crate::provider_yookassa::YooKassaProvider;
use crate::providers::{
    CreatePaymentRequest, CreatePaymentResponse, PaymentProvider, ProviderError, ProviderEvent,
    ProviderKind,
};

#[derive(Debug, Clone)]
pub struct SbpProvider {
    pub public_base_url: String,
    pub api_base_url: String,
    pub shop_id: String,
    pub secret_key: String,
}

impl SbpProvider {
    fn yookassa(&self) -> YooKassaProvider {
        YooKassaProvider {
            public_base_url: self.public_base_url.clone(),
            api_base_url: self.api_base_url.clone(),
            shop_id: self.shop_id.clone(),
            secret_key: self.secret_key.clone(),
        }
    }
}

impl PaymentProvider for SbpProvider {
    fn create_payment(
        &self,
        request: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, ProviderError> {
        self.yookassa().create_sbp_payment(request)
    }

    fn parse_callback(&self, raw_body: &[u8]) -> Result<ProviderEvent, ProviderError> {
        let mut event = self.yookassa().parse_callback(raw_body)?;
        event.provider = ProviderKind::Sbp;
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn provider() -> SbpProvider {
        SbpProvider {
            public_base_url: "https://licenses.example.org".into(),
            api_base_url: "https://api.yookassa.ru".into(),
            shop_id: String::new(),
            secret_key: String::new(),
        }
    }

    #[test]
    fn sbp_fails_closed_without_yookassa_credentials() {
        let request = CreatePaymentRequest {
            order_id: Uuid::nil(),
            amount_rub: 1_490,
            description: "test".into(),
            return_url: None,
        };
        assert!(matches!(
            provider().create_payment(request),
            Err(ProviderError::Transport(_))
        ));
    }

    #[test]
    fn parsed_yookassa_notification_is_classified_as_sbp() {
        let body = br#"{"type":"notification","event":"payment.succeeded","object":{"id":"payment-sbp","status":"succeeded","amount":{"value":"1490.00","currency":"RUB"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}}"#;
        let event = provider().parse_callback(body).expect("parse SBP callback");
        assert_eq!(event.provider, ProviderKind::Sbp);
    }
}
