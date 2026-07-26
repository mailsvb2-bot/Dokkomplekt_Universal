use base64::{engine::general_purpose::STANDARD, Engine as _};
use dokkomplekt_license_core::canonical::canonical_json;
use dokkomplekt_license_core::models::WatermarkMode;
use dokkomplekt_license_core::{
    evaluate_access, verify_license_signature, AccessRequest, AccessStatus, Feature,
    LicensePayload, MachineFingerprint, PlanId, PublicKeyBytes, UsageLedger,
};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use time::macros::datetime;

fn doctor_pro_payload() -> LicensePayload {
    LicensePayload {
        license_id: "DKK-E2E-1".to_string(),
        order_id: Some("ORDER-1".to_string()),
        plan: PlanId::DoctorPro,
        owner_name: Some("Doctor".to_string()),
        organization_name: None,
        seats: 2,
        allowed_machines: vec!["machine-a".to_string()],
        valid_from: datetime!(2026-01-01 00:00:00 UTC),
        valid_until: datetime!(2027-01-01 00:00:00 UTC),
        document_limit_month: 3000,
        template_limit: 150,
        profile_limit: 3,
        features: vec![Feature::BatchGeneration, Feature::BatchPrint],
        grace_days: 7,
        watermark_mode: WatermarkMode::None,
        issued_by: "test-issuer".to_string(),
        issued_at: datetime!(2026-06-27 00:00:00 UTC),
        metadata: Default::default(),
    }
}

#[test]
fn signed_paid_license_allows_access_but_tampered_limit_fails_proof() {
    let signer = SigningKey::from_bytes(&[7u8; 32]);
    let verifier = VerifyingKey::from(&signer);
    let public_key = PublicKeyBytes(verifier.to_bytes());
    let payload = doctor_pro_payload();
    let message = canonical_json(&payload).expect("canonical payload");
    let signature = STANDARD.encode(signer.sign(&message).to_bytes());

    verify_license_signature(&payload, &signature, &public_key).expect("valid signature");

    let decision = evaluate_access(
        &payload,
        &UsageLedger::default(),
        &AccessRequest {
            now_utc: datetime!(2026-06-28 00:00:00 UTC),
            month_key: "2026-06".to_string(),
            machine: MachineFingerprint("machine-a".to_string()),
            requested_documents: 25,
            template_count: Some(42),
            profile_count: Some(1),
        },
    )
    .expect("valid policy decision");
    assert_eq!(decision.status, AccessStatus::Allowed);
    assert!(!decision.watermark);

    let mut tampered = payload.clone();
    tampered.document_limit_month = 999_999;
    assert!(verify_license_signature(&tampered, &signature, &public_key).is_err());
}
