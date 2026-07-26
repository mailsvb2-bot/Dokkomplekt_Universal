//! Signed, domain-scoped confidence calibration for the desktop print gate.
//!
//! A calibration package is produced from the opt-in, privacy-preserving corpus
//! after comparing system proposals with the specialist's final accepted case.
//! The desktop trusts only the build-time Ed25519 anchor and re-verifies the
//! encrypted stored package every time it is used. Missing, stale-looking or
//! malformed evidence never disables generation; it only disables AutoPrint.

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use dokkomplekt_core::{CalibratedThresholds, DomainKind, SemanticCase};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const PACKAGE_SCHEMA: &str = "dokkomplekt.calibrated-thresholds.v1";
const PACKAGE_SIGNATURE_ALG: &str = "ed25519";
const PACKAGE_INDEX_STATE_KEY: &str = "calibrated_threshold_index_v1";
const PACKAGE_STATE_PREFIX: &str = "calibrated_threshold_package_v1:";
const MAX_PACKAGE_BYTES: usize = 1024 * 1024;
const MAX_FUTURE_CLOCK_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationTrainingStats {
    pub entry_count: usize,
    pub high_risk_observations: usize,
    pub auto_bucket_observations: usize,
    pub auto_bucket_errors: usize,
    pub auto_bucket_error_rate: f64,
    pub review_bucket_observations: usize,
    pub review_bucket_errors: usize,
    pub review_bucket_error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationHoldoutStats {
    pub entry_count: usize,
    pub high_risk_observations: usize,
    pub auto_bucket_observations: usize,
    pub auto_bucket_errors: usize,
    pub auto_bucket_error_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationPolicy {
    pub holdout_percent: u8,
    pub min_auto_samples: usize,
    pub min_review_samples: usize,
    pub min_holdout_auto_samples: usize,
    pub source_of_truth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationPayload {
    pub schema: String,
    pub domain: String,
    pub generated_at: String,
    pub corpus_sha256: String,
    pub auto_min_confidence: f32,
    pub review_min_confidence: f32,
    pub max_auto_error_rate: f32,
    pub training: CalibrationTrainingStats,
    pub holdout: CalibrationHoldoutStats,
    pub policy: CalibrationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedCalibrationPackage {
    pub payload: CalibrationPayload,
    pub signature_alg: String,
    pub signature_b64: String,
    #[serde(default)]
    pub public_key_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCalibrationPackage {
    package: SignedCalibrationPackage,
    imported_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalibrationIndex {
    domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CalibratedThresholdStatus {
    pub installed: bool,
    pub domain: String,
    pub generated_at: String,
    pub imported_at: String,
    pub corpus_sha256: String,
    pub auto_min_confidence: f32,
    pub review_min_confidence: f32,
    pub max_auto_error_rate: f32,
    pub training_observations: usize,
    pub holdout_observations: usize,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ImportCalibratedThresholdsRequest {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub bytes_base64: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ThresholdSelection {
    pub thresholds: CalibratedThresholds,
    pub warning: Option<String>,
}

pub(crate) fn import_package(
    app: &tauri::AppHandle,
    request: ImportCalibratedThresholdsRequest,
) -> Result<CalibratedThresholdStatus, String> {
    let bytes = request_bytes(&request)?;
    let package = verify_package_bytes(&bytes)?;
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| error.to_string())?;
    let stored = StoredCalibrationPackage {
        package: package.clone(),
        imported_at: now,
    };
    let repo = crate::repository_for(&crate::default_state_db_path(app)?)?;
    let domain = package.payload.domain.clone();
    repo.save_state_value(&package_state_key(&domain), &stored)
        .map_err(|error| error.to_string())?;
    let mut index = repo
        .load_state_value::<CalibrationIndex>(PACKAGE_INDEX_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    if !index.domains.iter().any(|item| item == &domain) {
        index.domains.push(domain.clone());
        index.domains.sort();
        index.domains.dedup();
        repo.save_state_value(PACKAGE_INDEX_STATE_KEY, &index)
            .map_err(|error| error.to_string())?;
    }
    crate::append_audit_event(
        app,
        "calibrated_thresholds_imported",
        "",
        &serde_json::json!({
            "domain": domain,
            "corpus_sha256": package.payload.corpus_sha256,
            "generated_at": package.payload.generated_at,
            "training_observations": package.payload.training.auto_bucket_observations,
            "holdout_observations": package.payload.holdout.auto_bucket_observations,
            "source_file": request.file_name,
        }),
    )?;
    status_from_stored(&stored)
}

pub(crate) fn list_statuses(
    app: &tauri::AppHandle,
) -> Result<Vec<CalibratedThresholdStatus>, String> {
    let repo = crate::repository_for(&crate::default_state_db_path(app)?)?;
    let index = repo
        .load_state_value::<CalibrationIndex>(PACKAGE_INDEX_STATE_KEY)
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let mut statuses = Vec::new();
    for domain in index.domains {
        let stored = repo
            .load_state_value::<StoredCalibrationPackage>(&package_state_key(&domain))
            .map_err(|error| error.to_string())?;
        let Some(stored) = stored else {
            continue;
        };
        // Re-verify on every read. Encrypted storage protects confidentiality and
        // integrity, while this check protects the release trust boundary.
        verify_package(&stored.package)?;
        statuses.push(status_from_stored(&stored)?);
    }
    statuses.sort_by(|left, right| left.domain.cmp(&right.domain));
    Ok(statuses)
}

pub(crate) fn thresholds_for_case(
    app: &tauri::AppHandle,
    case: &SemanticCase,
) -> ThresholdSelection {
    let domain = match single_case_domain(case) {
        Ok(domain) => domain,
        Err(warning) => {
            return ThresholdSelection {
                thresholds: CalibratedThresholds::default(),
                warning: Some(warning),
            }
        }
    };
    let loaded = (|| -> Result<Option<StoredCalibrationPackage>, String> {
        let repo = crate::repository_for(&crate::default_state_db_path(app)?)?;
        repo.load_state_value::<StoredCalibrationPackage>(&package_state_key(&domain))
            .map_err(|error| error.to_string())
    })();
    match loaded {
        Ok(Some(stored)) => match verify_package(&stored.package) {
            Ok(package) => ThresholdSelection {
                thresholds: thresholds_from_payload(&package.payload),
                warning: None,
            },
            Err(error) => ThresholdSelection {
                thresholds: CalibratedThresholds::default(),
                warning: Some(format!(
                    "Подписанная калибровка домена «{domain}» повреждена или недоверена: {error}. Автопечать запрещена."
                )),
            },
        },
        Ok(None) => ThresholdSelection {
            thresholds: CalibratedThresholds::default(),
            warning: Some(format!(
                "Для домена «{domain}» не установлена подписанная калибровка корпуса; автопечать запрещена."
            )),
        },
        Err(error) => ThresholdSelection {
            thresholds: CalibratedThresholds::default(),
            warning: Some(format!(
                "Не удалось прочитать локальную калибровку домена «{domain}»: {error}. Автопечать запрещена."
            )),
        },
    }
}

fn request_bytes(request: &ImportCalibratedThresholdsRequest) -> Result<Vec<u8>, String> {
    let has_path = request
        .path
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_bytes = request
        .bytes_base64
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if has_path == has_bytes {
        return Err(
            "Передайте ровно один источник пакета калибровки: path либо bytes_base64.".into(),
        );
    }
    let bytes = if let Some(path) = request
        .path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        read_limited(Path::new(path.trim()))?
    } else {
        BASE64_STANDARD
            .decode(request.bytes_base64.as_deref().unwrap_or_default().trim())
            .map_err(|_| "Пакет калибровки не является корректным base64.".to_string())?
    };
    if bytes.is_empty() || bytes.len() > MAX_PACKAGE_BYTES {
        return Err(format!(
            "Пакет калибровки должен занимать от 1 байта до {MAX_PACKAGE_BYTES} байт."
        ));
    }
    Ok(bytes)
}

fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Не удалось прочитать пакет калибровки: {error}"))?;
    if !metadata.is_file() || metadata.len() as usize > MAX_PACKAGE_BYTES {
        return Err("Пакет калибровки отсутствует, не является файлом или слишком велик.".into());
    }
    fs::read(path).map_err(|error| format!("Не удалось прочитать пакет калибровки: {error}"))
}

fn verify_package_bytes(bytes: &[u8]) -> Result<SignedCalibrationPackage, String> {
    if bytes.is_empty() || bytes.len() > MAX_PACKAGE_BYTES {
        return Err("Некорректный размер пакета калибровки.".into());
    }
    let package: SignedCalibrationPackage = serde_json::from_slice(bytes)
        .map_err(|error| format!("Некорректный JSON пакета калибровки: {error}"))?;
    verify_package(&package)
}

fn verify_package(package: &SignedCalibrationPackage) -> Result<SignedCalibrationPackage, String> {
    verify_package_with_key(package, crate::TRUSTED_THRESHOLD_PUBKEY_B64)
}

fn verify_package_with_key(
    package: &SignedCalibrationPackage,
    trusted_key_b64: &str,
) -> Result<SignedCalibrationPackage, String> {
    validate_payload(&package.payload)?;
    if !package
        .signature_alg
        .eq_ignore_ascii_case(PACKAGE_SIGNATURE_ALG)
    {
        return Err("Пакет калибровки должен быть подписан Ed25519.".into());
    }
    let key_bytes = BASE64_STANDARD
        .decode(trusted_key_b64.trim())
        .map_err(|_| "Некорректный встроенный public key калибровки.".to_string())?;
    let key_array: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Public key калибровки должен содержать 32 байта.".to_string())?;
    let key = VerifyingKey::from_bytes(&key_array)
        .map_err(|_| "Некорректный public key калибровки.".to_string())?;
    if let Some(advertised) = package.public_key_b64.as_deref() {
        let advertised_bytes = BASE64_STANDARD
            .decode(advertised.trim())
            .map_err(|_| "public_key_b64 пакета калибровки некорректен.".to_string())?;
        if advertised_bytes != key_bytes {
            return Err(
                "Public key внутри пакета не совпадает со встроенным trust anchor приложения."
                    .into(),
            );
        }
    }
    let signature_bytes = BASE64_STANDARD
        .decode(package.signature_b64.trim())
        .map_err(|_| "Некорректная base64-подпись калибровки.".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "Некорректная длина подписи калибровки.".to_string())?;
    let payload_value =
        serde_json::to_value(&package.payload).map_err(|error| error.to_string())?;
    let canonical = crate::canonical_json_bytes(&payload_value)?;
    key.verify(&canonical, &signature)
        .map_err(|_| "Подпись калибровки не прошла проверку.".to_string())?;
    Ok(package.clone())
}

fn validate_payload(payload: &CalibrationPayload) -> Result<(), String> {
    if payload.schema != PACKAGE_SCHEMA {
        return Err("Неподдерживаемая схема пакета калибровки.".into());
    }
    let normalized_domain = normalize_domain_slug(&payload.domain)?;
    if normalized_domain != payload.domain {
        return Err("Домен пакета должен быть записан в каноническом нижнем регистре.".into());
    }
    if !is_sha256(&payload.corpus_sha256) {
        return Err("corpus_sha256 должен быть 64-символьным SHA-256.".into());
    }
    let generated = OffsetDateTime::parse(&payload.generated_at, &Rfc3339)
        .map_err(|_| "generated_at пакета должен быть RFC3339 timestamp.".to_string())?;
    let future_limit = OffsetDateTime::now_utc()
        .checked_add(time::Duration::seconds(MAX_FUTURE_CLOCK_SKEW_SECONDS))
        .unwrap_or_else(OffsetDateTime::now_utc);
    if generated > future_limit {
        return Err("generated_at пакета находится недопустимо далеко в будущем.".into());
    }
    let thresholds = thresholds_from_payload(payload);
    thresholds.validate()?;
    if payload.policy.source_of_truth != "specialist_final_accepted" {
        return Err("Ground truth калибровки должен быть specialist_final_accepted.".into());
    }
    if !(1..=50).contains(&payload.policy.holdout_percent)
        || payload.policy.min_auto_samples == 0
        || payload.policy.min_review_samples == 0
        || payload.policy.min_holdout_auto_samples == 0
    {
        return Err("Некорректная политика размера обучающей/held-out выборки.".into());
    }
    validate_stats(
        payload.training.auto_bucket_observations,
        payload.training.auto_bucket_errors,
        payload.training.auto_bucket_error_rate,
        "training auto bucket",
    )?;
    validate_stats(
        payload.training.review_bucket_observations,
        payload.training.review_bucket_errors,
        payload.training.review_bucket_error_rate,
        "training review bucket",
    )?;
    validate_stats(
        payload.holdout.auto_bucket_observations,
        payload.holdout.auto_bucket_errors,
        payload.holdout.auto_bucket_error_rate,
        "held-out auto bucket",
    )?;
    if payload.training.auto_bucket_observations < payload.policy.min_auto_samples
        || payload.training.review_bucket_observations < payload.policy.min_review_samples
        || payload.holdout.auto_bucket_observations < payload.policy.min_holdout_auto_samples
    {
        return Err("Пакет не содержит заявленного минимального числа наблюдений.".into());
    }
    let ceiling = f64::from(payload.max_auto_error_rate) + 1e-9;
    if payload.training.auto_bucket_error_rate > ceiling
        || payload.holdout.auto_bucket_error_rate > ceiling
    {
        return Err("Ошибка auto-bucket превышает подписанный допустимый предел.".into());
    }
    if payload.training.high_risk_observations < payload.training.auto_bucket_observations
        || payload.holdout.high_risk_observations < payload.holdout.auto_bucket_observations
        || payload.training.entry_count == 0
        || payload.holdout.entry_count == 0
    {
        return Err("Статистика корпуса внутренне противоречива.".into());
    }
    Ok(())
}

fn validate_stats(
    observations: usize,
    errors: usize,
    rate: f64,
    label: &str,
) -> Result<(), String> {
    if observations == 0
        || errors > observations
        || !rate.is_finite()
        || !(0.0..=1.0).contains(&rate)
    {
        return Err(format!("Некорректная статистика {label}."));
    }
    let expected = errors as f64 / observations as f64;
    if (expected - rate).abs() > 1e-9 {
        return Err(format!("Доля ошибок {label} не совпадает со счётчиками."));
    }
    Ok(())
}

fn thresholds_from_payload(payload: &CalibrationPayload) -> CalibratedThresholds {
    CalibratedThresholds {
        auto_min_confidence: payload.auto_min_confidence,
        review_min_confidence: payload.review_min_confidence,
        max_auto_error_rate: payload.max_auto_error_rate,
        calibration_evidence_sha256: Some(payload.corpus_sha256.clone()),
    }
}

fn status_from_stored(
    stored: &StoredCalibrationPackage,
) -> Result<CalibratedThresholdStatus, String> {
    let package = verify_package(&stored.package)?;
    Ok(CalibratedThresholdStatus {
        installed: true,
        domain: package.payload.domain,
        generated_at: package.payload.generated_at,
        imported_at: stored.imported_at.clone(),
        corpus_sha256: package.payload.corpus_sha256,
        auto_min_confidence: package.payload.auto_min_confidence,
        review_min_confidence: package.payload.review_min_confidence,
        max_auto_error_rate: package.payload.max_auto_error_rate,
        training_observations: package.payload.training.auto_bucket_observations,
        holdout_observations: package.payload.holdout.auto_bucket_observations,
        message: "Порог проверен Ed25519, связан с corpus SHA-256 и held-out метриками.".into(),
    })
}

fn single_case_domain(case: &SemanticCase) -> Result<String, String> {
    let mut domains = case
        .active_domains
        .iter()
        .filter(|domain| !matches!(domain, DomainKind::Generic))
        .map(domain_slug)
        .collect::<BTreeSet<_>>();
    if domains.is_empty() {
        domains.insert("generic".into());
    }
    if domains.len() != 1 {
        return Err(format!(
            "В деле одновременно активны несколько доменов ({}); один порог нельзя безопасно применять ко всему комплекту. Автопечать запрещена.",
            domains.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(domains
        .into_iter()
        .next()
        .unwrap_or_else(|| "generic".into()))
}

fn domain_slug(domain: &DomainKind) -> String {
    match domain {
        DomainKind::Generic => "generic".into(),
        DomainKind::Medical => "medical".into(),
        DomainKind::Legal => "legal".into(),
        DomainKind::Hr => "hr".into(),
        DomainKind::Education => "education".into(),
        DomainKind::Accounting => "accounting".into(),
        DomainKind::Custom(value) => format!("custom:{}", normalize_custom_slug(value)),
    }
}

fn normalize_domain_slug(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "generic" | "medical" | "legal" | "hr" | "education" | "accounting"
    ) {
        return Ok(normalized);
    }
    if let Some(custom) = normalized.strip_prefix("custom:") {
        let custom = normalize_custom_slug(custom);
        if !custom.is_empty() {
            return Ok(format!("custom:{custom}"));
        }
    }
    Err("Пакет калибровки содержит неизвестный или небезопасный домен.".into())
}

fn normalize_custom_slug(raw: &str) -> String {
    raw.chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if matches!(character, '-' | '_' | '.') {
                Some(character)
            } else {
                None
            }
        })
        .take(48)
        .collect()
}

fn package_state_key(domain: &str) -> String {
    let digest = hex::encode(Sha256::digest(domain.as_bytes()));
    format!("{PACKAGE_STATE_PREFIX}{digest}")
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};

    fn payload() -> CalibrationPayload {
        CalibrationPayload {
            schema: PACKAGE_SCHEMA.into(),
            domain: "hr".into(),
            generated_at: "2026-07-21T12:00:00Z".into(),
            corpus_sha256: "a".repeat(64),
            auto_min_confidence: 0.995,
            review_min_confidence: 0.90,
            max_auto_error_rate: 0.005,
            training: CalibrationTrainingStats {
                entry_count: 90,
                high_risk_observations: 100,
                auto_bucket_observations: 80,
                auto_bucket_errors: 0,
                auto_bucket_error_rate: 0.0,
                review_bucket_observations: 100,
                review_bucket_errors: 2,
                review_bucket_error_rate: 0.02,
            },
            holdout: CalibrationHoldoutStats {
                entry_count: 10,
                high_risk_observations: 10,
                auto_bucket_observations: 10,
                auto_bucket_errors: 0,
                auto_bucket_error_rate: 0.0,
            },
            policy: CalibrationPolicy {
                holdout_percent: 10,
                min_auto_samples: 50,
                min_review_samples: 20,
                min_holdout_auto_samples: 10,
                source_of_truth: "specialist_final_accepted".into(),
            },
        }
    }

    fn signed_package(payload: CalibrationPayload, key: &SigningKey) -> SignedCalibrationPackage {
        let value = serde_json::to_value(&payload).expect("payload");
        let canonical = crate::canonical_json_bytes(&value).expect("canonical");
        SignedCalibrationPackage {
            payload,
            signature_alg: PACKAGE_SIGNATURE_ALG.into(),
            signature_b64: BASE64_STANDARD.encode(key.sign(&canonical).to_bytes()),
            public_key_b64: Some(BASE64_STANDARD.encode(key.verifying_key().to_bytes())),
        }
    }

    #[test]
    fn valid_signed_held_out_calibration_is_accepted() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let package = signed_package(payload(), &key);
        let verified = verify_package_with_key(
            &package,
            &BASE64_STANDARD.encode(key.verifying_key().to_bytes()),
        )
        .expect("verified");
        assert_eq!(verified.payload.domain, "hr");
    }

    #[test]
    fn payload_tampering_is_rejected() {
        let key = SigningKey::from_bytes(&[8_u8; 32]);
        let mut package = signed_package(payload(), &key);
        package.payload.auto_min_confidence = 0.80;
        assert!(verify_package_with_key(
            &package,
            &BASE64_STANDARD.encode(key.verifying_key().to_bytes()),
        )
        .is_err());
    }

    #[test]
    fn insufficient_held_out_evidence_is_rejected_before_signature_use() {
        let key = SigningKey::from_bytes(&[9_u8; 32]);
        let mut weak = payload();
        weak.holdout.auto_bucket_observations = 1;
        weak.holdout.high_risk_observations = 1;
        let package = signed_package(weak, &key);
        assert!(verify_package_with_key(
            &package,
            &BASE64_STANDARD.encode(key.verifying_key().to_bytes()),
        )
        .is_err());
    }

    #[test]
    fn multi_domain_case_never_receives_one_domains_threshold() {
        let case = SemanticCase {
            active_domains: vec![DomainKind::Hr, DomainKind::Accounting],
            ..SemanticCase::default()
        };
        assert!(single_case_domain(&case).is_err());
    }
}
