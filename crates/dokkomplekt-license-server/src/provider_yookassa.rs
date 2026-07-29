#![allow(dead_code)]

//! YooKassa integration.
//!
//! Webhook notifications are treated only as a prompt to verify a payment.
//! The authoritative status, amount, currency and `metadata.order_id` are fetched
//! from YooKassa's authenticated API before any local order is changed. Optional
//! IP allow-listing at the reverse proxy remains defence in depth, not correctness.
//!
//! Outbound payment creation uses YooKassa's official HTTPS REST endpoint with
//! HTTP Basic authentication and an idempotence key derived from the internal
//! order UUID. Credentials are supplied only through server configuration.

use crate::providers::{
    CreatePaymentRequest, CreatePaymentResponse, PaymentProvider, ProviderError, ProviderEvent,
    ProviderKind, ProviderPaymentStatus,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct YooKassaProvider {
    pub public_base_url: String,
    pub api_base_url: String,
    pub shop_id: String,
    pub secret_key: String,
}

#[derive(Debug, Serialize)]
struct CreateYooKassaPayment<'a> {
    amount: CreateYooKassaAmount<'a>,
    capture: bool,
    confirmation: CreateYooKassaConfirmation<'a>,
    description: &'a str,
    metadata: CreateYooKassaMetadata,
}

#[derive(Debug, Serialize)]
struct CreateYooKassaAmount<'a> {
    value: String,
    currency: &'a str,
}

#[derive(Debug, Serialize)]
struct CreateYooKassaConfirmation<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    return_url: String,
}

#[derive(Debug, Serialize)]
struct CreateYooKassaMetadata {
    order_id: String,
}

#[derive(Debug, Deserialize)]
struct CreatedYooKassaPayment {
    id: String,
    confirmation: Option<CreatedYooKassaConfirmation>,
}

#[derive(Debug, Deserialize)]
struct CreatedYooKassaConfirmation {
    confirmation_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YooKassaNotification {
    #[serde(rename = "type")]
    kind: String,
    event: String,
    object: YooKassaPayment,
}

#[derive(Debug, Deserialize)]
struct YooKassaPayment {
    id: String,
    status: String,
    amount: YooKassaAmount,
    #[serde(default)]
    metadata: YooKassaMetadata,
}

#[derive(Debug, Deserialize)]
struct YooKassaAmount {
    value: String,
    currency: String,
}

#[derive(Debug, Default, Deserialize)]
struct YooKassaMetadata {
    #[serde(default)]
    order_id: Option<String>,
}

/// Parse a RUB amount string like "1490.00" into whole roubles.
/// Fractional kopecks are rejected: license plans are priced in whole roubles,
/// and a mismatch is more likely tampering than a legitimate payment.
fn parse_rub_amount(value: &str) -> Result<u64, ProviderError> {
    let trimmed = value.trim();
    let (rub, kop) = match trimmed.split_once('.') {
        Some((rub, kop)) => (rub, kop),
        None => (trimmed, "0"),
    };
    let rub: u64 = rub
        .parse()
        .map_err(|_| ProviderError::BadRequest(format!("bad amount value: {value}")))?;
    let kop: u64 = if kop.is_empty() {
        0
    } else {
        kop.parse()
            .map_err(|_| ProviderError::BadRequest(format!("bad amount value: {value}")))?
    };
    if kop != 0 {
        return Err(ProviderError::BadRequest(format!(
            "non-integer rouble amount is not accepted: {value}"
        )));
    }
    Ok(rub)
}

fn payment_status(value: &str) -> Result<ProviderPaymentStatus, ProviderError> {
    match value.trim() {
        "pending" | "waiting_for_capture" => Ok(ProviderPaymentStatus::Pending),
        "succeeded" => Ok(ProviderPaymentStatus::Succeeded),
        "canceled" => Ok(ProviderPaymentStatus::Cancelled),
        other => Err(ProviderError::BadRequest(format!(
            "unsupported YooKassa payment status: {other}"
        ))),
    }
}

impl YooKassaProvider {
    fn authenticated_client(&self) -> Result<reqwest::blocking::Client, ProviderError> {
        if self.shop_id.trim().is_empty() || self.secret_key.trim().is_empty() {
            return Err(ProviderError::Transport(
                "YooKassa credentials are not configured".into(),
            ));
        }
        crate::ensure_rustls_crypto_provider();
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| ProviderError::Transport(error.to_string()))
    }

    /// Authenticate an official webhook by resolving its payment through the
    /// YooKassa API. The callback body alone is never trusted to mark an order paid.
    pub fn verify_callback(&self, raw_body: &[u8]) -> Result<ProviderEvent, ProviderError> {
        let candidate = self.parse_callback(raw_body)?;
        let payment_id = candidate
            .provider_payment_id
            .as_deref()
            .ok_or_else(|| ProviderError::BadRequest("payment id is missing".into()))?;
        if payment_id.is_empty()
            || payment_id.len() > 128
            || !payment_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            return Err(ProviderError::BadRequest(
                "invalid YooKassa payment id".into(),
            ));
        }
        let endpoint = format!(
            "{}/v3/payments/{}",
            self.api_base_url.trim_end_matches('/'),
            payment_id
        );
        let response = self
            .authenticated_client()?
            .get(endpoint)
            .basic_auth(self.shop_id.trim(), Some(self.secret_key.trim()))
            .send()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(ProviderError::Transport(format!(
                "YooKassa verification returned HTTP {status}: {}",
                body.chars().take(512).collect::<String>()
            )));
        }
        let verified: YooKassaPayment = serde_json::from_slice(&bytes).map_err(|error| {
            ProviderError::BadRequest(format!("bad YooKassa verification response: {error}"))
        })?;
        if verified.id != payment_id {
            return Err(ProviderError::BadRequest(
                "YooKassa verification returned another payment".into(),
            ));
        }
        if verified.amount.currency != "RUB" {
            return Err(ProviderError::BadRequest(format!(
                "unsupported currency: {}",
                verified.amount.currency
            )));
        }
        let order_id_raw = verified
            .metadata
            .order_id
            .as_deref()
            .ok_or_else(|| ProviderError::BadRequest("metadata.order_id is missing".into()))?;
        let order_id = Uuid::parse_str(order_id_raw).map_err(|_| {
            ProviderError::BadRequest(format!("metadata.order_id is not a UUID: {order_id_raw}"))
        })?;
        let amount_rub = parse_rub_amount(&verified.amount.value)?;
        if order_id != candidate.order_id || amount_rub != candidate.amount_rub {
            return Err(ProviderError::BadRequest(
                "webhook data does not match the authenticated YooKassa payment".into(),
            ));
        }
        let status = payment_status(&verified.status)?;
        Ok(ProviderEvent {
            provider: ProviderKind::YooKassa,
            provider_event_id: format!("verified:{}:{}", verified.id, verified.status),
            provider_payment_id: Some(verified.id),
            order_id,
            status,
            amount_rub,
        })
    }
}

impl PaymentProvider for YooKassaProvider {
    fn create_payment(
        &self,
        request: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, ProviderError> {
        let return_url = request.return_url.clone().unwrap_or_else(|| {
            format!(
                "{}/payment/return/{}",
                self.public_base_url.trim_end_matches('/'),
                request.order_id
            )
        });
        let body = CreateYooKassaPayment {
            amount: CreateYooKassaAmount {
                value: format!("{}.00", request.amount_rub),
                currency: "RUB",
            },
            capture: true,
            confirmation: CreateYooKassaConfirmation {
                kind: "redirect",
                return_url,
            },
            description: request.description.trim(),
            metadata: CreateYooKassaMetadata {
                order_id: request.order_id.to_string(),
            },
        };
        let endpoint = format!("{}/v3/payments", self.api_base_url.trim_end_matches('/'));
        let response = self
            .authenticated_client()?
            .post(endpoint)
            .basic_auth(self.shop_id.trim(), Some(self.secret_key.trim()))
            .header("Idempotence-Key", request.order_id.to_string())
            .json(&body)
            .send()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(ProviderError::Transport(format!(
                "YooKassa returned HTTP {status}: {}",
                body.chars().take(512).collect::<String>()
            )));
        }
        let created: CreatedYooKassaPayment = serde_json::from_slice(&bytes).map_err(|error| {
            ProviderError::BadRequest(format!("bad YooKassa response: {error}"))
        })?;
        let confirmation_url = created
            .confirmation
            .and_then(|confirmation| confirmation.confirmation_url)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ProviderError::BadRequest("YooKassa response has no confirmation_url".into())
            })?;
        Ok(CreatePaymentResponse {
            provider: ProviderKind::YooKassa,
            provider_payment_id: created.id,
            confirmation_url,
            qr_url: None,
        })
    }

    fn parse_callback(&self, raw_body: &[u8]) -> Result<ProviderEvent, ProviderError> {
        let notification: YooKassaNotification =
            serde_json::from_slice(raw_body).map_err(|err| {
                ProviderError::BadRequest(format!("bad YooKassa notification: {err}"))
            })?;
        if notification.kind != "notification" {
            return Err(ProviderError::BadRequest(format!(
                "unexpected notification type: {}",
                notification.kind
            )));
        }
        let status = match notification.event.as_str() {
            "payment.succeeded" => ProviderPaymentStatus::Succeeded,
            "payment.canceled" => ProviderPaymentStatus::Cancelled,
            "payment.waiting_for_capture" => ProviderPaymentStatus::Pending,
            other => {
                return Err(ProviderError::BadRequest(format!(
                    "unsupported YooKassa event: {other}"
                )))
            }
        };
        if notification.object.amount.currency != "RUB" {
            return Err(ProviderError::BadRequest(format!(
                "unsupported currency: {}",
                notification.object.amount.currency
            )));
        }
        let order_id_raw = notification
            .object
            .metadata
            .order_id
            .as_deref()
            .ok_or_else(|| ProviderError::BadRequest("metadata.order_id is missing".into()))?;
        let order_id = Uuid::parse_str(order_id_raw).map_err(|_| {
            ProviderError::BadRequest(format!("metadata.order_id is not a UUID: {order_id_raw}"))
        })?;
        let amount_rub = parse_rub_amount(&notification.object.amount.value)?;
        Ok(ProviderEvent {
            provider: ProviderKind::YooKassa,
            provider_event_id: format!("{}:{}", notification.event, notification.object.id),
            provider_payment_id: Some(notification.object.id),
            order_id,
            status,
            amount_rub,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> YooKassaProvider {
        YooKassaProvider {
            public_base_url: "https://pay.example".into(),
            api_base_url: "https://api.yookassa.ru".into(),
            shop_id: "shop".into(),
            secret_key: "secret".into(),
        }
    }

    #[test]
    fn parses_succeeded_notification() {
        let body = br#"{
            "type": "notification",
            "event": "payment.succeeded",
            "object": {
                "id": "2d6ff9d7-000f-5000-8000-1b68e7b15f3f",
                "status": "succeeded",
                "amount": { "value": "1490.00", "currency": "RUB" },
                "metadata": { "order_id": "11111111-2222-3333-4444-555555555555" }
            }
        }"#;
        let event = provider().parse_callback(body).expect("parse");
        assert_eq!(event.status, ProviderPaymentStatus::Succeeded);
        assert_eq!(event.amount_rub, 1490);
        assert_eq!(
            event.order_id.to_string(),
            "11111111-2222-3333-4444-555555555555"
        );
        assert_eq!(
            event.provider_payment_id.as_deref(),
            Some("2d6ff9d7-000f-5000-8000-1b68e7b15f3f")
        );
    }

    #[test]
    fn rejects_missing_order_id_foreign_currency_and_kopecks() {
        let no_order = br#"{"type":"notification","event":"payment.succeeded","object":{"id":"x","status":"succeeded","amount":{"value":"10.00","currency":"RUB"},"metadata":{}}}"#;
        assert!(provider().parse_callback(no_order).is_err());
        let usd = br#"{"type":"notification","event":"payment.succeeded","object":{"id":"x","status":"succeeded","amount":{"value":"10.00","currency":"USD"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}}"#;
        assert!(provider().parse_callback(usd).is_err());
        let kop = br#"{"type":"notification","event":"payment.succeeded","object":{"id":"x","status":"succeeded","amount":{"value":"10.50","currency":"RUB"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}}"#;
        assert!(provider().parse_callback(kop).is_err());
    }

    #[test]
    fn cancellation_maps_to_cancelled() {
        let body = br#"{"type":"notification","event":"payment.canceled","object":{"id":"y","status":"canceled","amount":{"value":"100","currency":"RUB"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}}"#;
        let event = provider().parse_callback(body).expect("parse");
        assert_eq!(event.status, ProviderPaymentStatus::Cancelled);
        assert_eq!(event.amount_rub, 100);
    }

    #[test]
    fn create_payment_performs_authenticated_http_request_and_parses_redirect() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("address");
        let order_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").expect("test UUID");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let Some(headers_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..headers_end + 4]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if request.len() >= headers_end + 4 + content_length {
                    break;
                }
            }
            let request_text = String::from_utf8(request).expect("UTF-8 HTTP request");
            assert!(request_text.starts_with("POST /v3/payments HTTP/1.1\r\n"));
            let expected_auth = format!("Authorization: Basic {}", STANDARD.encode("shop:secret"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains(&expected_auth.to_ascii_lowercase()));
            assert!(
                request_text.contains("Idempotence-Key: 11111111-2222-3333-4444-555555555555")
                    || request_text
                        .contains("idempotence-key: 11111111-2222-3333-4444-555555555555")
            );
            assert!(request_text.contains("\"value\":\"1490.00\""));
            assert!(request_text.contains("\"order_id\":\"11111111-2222-3333-4444-555555555555\""));

            let body = r#"{"id":"payment-123","confirmation":{"confirmation_url":"https://pay.example/confirm/payment-123"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });

        let provider = YooKassaProvider {
            public_base_url: "https://product.example".into(),
            api_base_url: format!("http://{address}"),
            shop_id: "shop".into(),
            secret_key: "secret".into(),
        };
        let created = provider
            .create_payment(CreatePaymentRequest {
                order_id,
                amount_rub: 1490,
                description: "Universal document plan".into(),
                return_url: None,
            })
            .expect("payment must be created through the mock HTTP API");
        assert_eq!(created.provider, ProviderKind::YooKassa);
        assert_eq!(created.provider_payment_id, "payment-123");
        assert_eq!(
            created.confirmation_url,
            "https://pay.example/confirm/payment-123"
        );
        server.join().expect("mock server thread");
    }

    #[test]
    fn callback_is_accepted_only_after_authenticated_payment_lookup() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let address = listener.local_addr().expect("address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request_text = String::from_utf8(request).expect("UTF-8 request");
            assert!(request_text.starts_with("GET /v3/payments/payment-verified HTTP/1.1\r\n"));
            let expected_auth = format!("Authorization: Basic {}", STANDARD.encode("shop:secret"));
            assert!(request_text
                .to_ascii_lowercase()
                .contains(&expected_auth.to_ascii_lowercase()));
            let body = r#"{"id":"payment-verified","status":"succeeded","amount":{"value":"1490.00","currency":"RUB"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write response");
        });
        let provider = YooKassaProvider {
            public_base_url: "https://product.example".into(),
            api_base_url: format!("http://{address}"),
            shop_id: "shop".into(),
            secret_key: "secret".into(),
        };
        let body = br#"{"type":"notification","event":"payment.succeeded","object":{"id":"payment-verified","status":"succeeded","amount":{"value":"1490.00","currency":"RUB"},"metadata":{"order_id":"11111111-2222-3333-4444-555555555555"}}}"#;
        let event = provider
            .verify_callback(body)
            .expect("authenticated lookup must verify callback");
        assert_eq!(event.status, ProviderPaymentStatus::Succeeded);
        assert_eq!(event.amount_rub, 1490);
        assert_eq!(event.provider_event_id, "verified:payment-verified:succeeded");
        server.join().expect("mock server thread");
    }

    #[test]
    fn create_payment_fails_closed_without_credentials() {
        let mut provider = provider();
        provider.secret_key.clear();
        let err = provider
            .create_payment(CreatePaymentRequest {
                order_id: Uuid::nil(),
                amount_rub: 100,
                description: "t".into(),
                return_url: None,
            })
            .unwrap_err();
        assert!(matches!(err, ProviderError::Transport(_)));
    }
}
