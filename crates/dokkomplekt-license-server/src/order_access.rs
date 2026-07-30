use axum::http::{header::AUTHORIZATION, HeaderMap};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const TOKEN_BYTES: usize = 32;
const TOKEN_HASH_DOMAIN: &[u8] = b"dokkomplekt-order-access-v1\0";
const MAX_BEARER_BYTES: usize = 256;

pub fn generate_order_access_token() -> (String, String) {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut bytes = [0_u8; TOKEN_BYTES];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_order_access_token(&token);
    (token, hash)
}

pub fn hash_order_access_token(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(TOKEN_HASH_DOMAIN);
    digest.update(token.as_bytes());
    hex::encode(digest.finalize())
}

pub fn authorize_order(headers: &HeaderMap, expected_hash: Option<&str>) -> bool {
    let Some(expected_hash) = expected_hash
        .map(str::trim)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    else {
        return false;
    };
    let Some(token) = bearer_token(headers) else {
        return false;
    };
    constant_time_eq(
        hash_order_access_token(token).as_bytes(),
        expected_hash.to_ascii_lowercase().as_bytes(),
    )
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    if value.len() > MAX_BEARER_BYTES {
        return None;
    }
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty() && !token.chars().any(char::is_whitespace)).then_some(token)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn generated_token_authorizes_and_wrong_token_does_not() {
        let (token, hash) = generate_order_access_token();
        assert_eq!(token.len(), 43);
        assert_eq!(hash.len(), 64);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        assert!(authorize_order(&headers, Some(&hash)));

        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer wrong"));
        assert!(!authorize_order(&headers, Some(&hash)));
    }

    #[test]
    fn missing_or_legacy_hash_fails_closed() {
        let headers = HeaderMap::new();
        assert!(!authorize_order(&headers, None));
        assert!(!authorize_order(&headers, Some("")));
        assert!(!authorize_order(&headers, Some("not-a-sha256")));
    }
}
