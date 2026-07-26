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
