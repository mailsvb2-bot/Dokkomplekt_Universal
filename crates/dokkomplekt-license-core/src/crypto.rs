use crate::canonical::canonical_json;
use crate::core_error::{CoreError, CoreResult};
use crate::models::{LicenseDocument, LicensePayload};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyBytes(pub [u8; 32]);

impl PublicKeyBytes {
    pub fn from_base64(input: &str) -> CoreResult<Self> {
        let decoded = STANDARD
            .decode(input)
            .map_err(|_| CoreError::BadPublicKey)?;
        let bytes: [u8; 32] = decoded.try_into().map_err(|_| CoreError::BadPublicKey)?;
        Ok(Self(bytes))
    }
}

pub fn verify_license_product_id(
    document: &LicenseDocument,
    expected_product_id: &str,
    allow_legacy_missing: bool,
) -> CoreResult<()> {
    let expected = expected_product_id.trim();
    if expected.is_empty() {
        return Err(CoreError::ProductMismatch("empty_expected_product_id".to_string()));
    }
    match document.license.payload.metadata.get("product_id") {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(CoreError::ProductMismatch(actual.clone())),
        None if allow_legacy_missing => Ok(()),
        None => Err(CoreError::ProductMismatch("missing_product_id".to_string())),
    }
}

pub fn verify_license_signature(
    payload: &LicensePayload,
    signature_b64: &str,
    public_key: &PublicKeyBytes,
) -> CoreResult<()> {
    if signature_b64.trim().is_empty() {
        return Err(CoreError::MissingProof);
    }
    let message = canonical_json(payload)?;
    let signature_bytes = STANDARD
        .decode(signature_b64)
        .map_err(|_| CoreError::BadProof)?;
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| CoreError::BadProof)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key.0).map_err(|_| CoreError::BadPublicKey)?;
    // Strict verification rejects non-canonical signatures and small-order public keys.
    verifying_key
        .verify_strict(&message, &signature)
        .map_err(|_| CoreError::BadProof)
}

/// Verify a full license document: the signature must be valid **and** the license
/// must be inside its validity window at the given instant. A validly-signed but
/// expired (or not-yet-valid) license is rejected.
pub fn verify_license_document_at(
    document: &LicenseDocument,
    public_key: &PublicKeyBytes,
    now: OffsetDateTime,
) -> CoreResult<()> {
    let payload = &document.license.payload;
    verify_license_signature(payload, &document.license.signature, public_key)?;
    if now < payload.valid_from {
        return Err(CoreError::NotYetValid);
    }
    if now > payload.valid_until {
        return Err(CoreError::Expired);
    }
    Ok(())
}

/// Convenience wrapper using the current system time.
pub fn verify_license_document_now(
    document: &LicenseDocument,
    public_key: &PublicKeyBytes,
) -> CoreResult<()> {
    verify_license_document_at(document, public_key, OffsetDateTime::now_utc())
}


#[cfg(test)]
mod product_scope_tests {
    use super::*;
    use crate::models::{Feature, PlanId, SignedLicense, WatermarkMode};
    use std::collections::BTreeMap;
    use time::OffsetDateTime;

    fn document(product_id: Option<&str>) -> LicenseDocument {
        let mut metadata = BTreeMap::new();
        if let Some(product_id) = product_id {
            metadata.insert("product_id".to_string(), product_id.to_string());
        }
        let now = OffsetDateTime::now_utc();
        LicenseDocument {
            schema: "dokkomplekt.license.v1".to_string(),
            license: SignedLicense {
                payload: LicensePayload {
                    license_id: "test".to_string(),
                    order_id: None,
                    plan: PlanId::DoctorStart,
                    owner_name: None,
                    organization_name: None,
                    seats: 1,
                    allowed_machines: vec![],
                    valid_from: now,
                    valid_until: now,
                    document_limit_month: 1,
                    template_limit: 1,
                    profile_limit: 1,
                    features: Vec::<Feature>::new(),
                    grace_days: 0,
                    watermark_mode: WatermarkMode::None,
                    issued_by: "test".to_string(),
                    issued_at: now,
                    metadata,
                },
                signature_alg: "ed25519".to_string(),
                signature: String::new(),
            },
        }
    }

    #[test]
    fn product_scope_rejects_cross_product_license() {
        let document = document(Some("diary_filler"));
        assert!(matches!(
            verify_license_product_id(&document, "dokkomplekt_universal", false),
            Err(CoreError::ProductMismatch(_))
        ));
    }

    #[test]
    fn legacy_missing_product_id_is_explicitly_opt_in_only() {
        let document = document(None);
        assert!(verify_license_product_id(&document, "dokkomplekt_universal", true).is_ok());
        assert!(matches!(
            verify_license_product_id(&document, "dokkomplekt_universal", false),
            Err(CoreError::ProductMismatch(_))
        ));
    }
}
