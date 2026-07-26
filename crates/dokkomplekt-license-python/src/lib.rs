#![allow(clippy::useless_conversion)]

use dokkomplekt_license_core::{
    evaluate_access, verify_license_signature, AccessRequest, LicenseDocument, MachineFingerprint,
    PublicKeyBytes, UsageLedger,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use time::OffsetDateTime;

#[pyfunction]
fn native_core_version() -> String {
    "0.1.0".to_string()
}

#[pyfunction]
fn license_plan(license_json: &str) -> PyResult<String> {
    let document: LicenseDocument =
        serde_json::from_str(license_json).map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(format!("{:?}", document.license.payload.plan))
}

#[pyfunction]
fn proof_ok(license_json: &str, verifier_b64: &str) -> PyResult<bool> {
    let document: LicenseDocument =
        serde_json::from_str(license_json).map_err(|err| PyValueError::new_err(err.to_string()))?;
    let verifier = PublicKeyBytes::from_base64(verifier_b64)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    verify_license_signature(
        &document.license.payload,
        &document.license.signature,
        &verifier,
    )
    .map_err(|err| PyValueError::new_err(err.to_string()))?;
    Ok(true)
}

#[pyfunction]
fn access_decision(
    license_json: &str,
    machine_hash: &str,
    month_key: &str,
    requested_documents: u32,
) -> PyResult<String> {
    let document: LicenseDocument =
        serde_json::from_str(license_json).map_err(|err| PyValueError::new_err(err.to_string()))?;
    let request = AccessRequest {
        now_utc: OffsetDateTime::now_utc(),
        month_key: month_key.to_string(),
        machine: MachineFingerprint(machine_hash.to_string()),
        requested_documents,
        template_count: None,
        profile_count: None,
    };
    let decision = evaluate_access(&document.license.payload, &UsageLedger::default(), &request)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    serde_json::to_string(&decision).map_err(|err| PyValueError::new_err(err.to_string()))
}

#[pymodule]
fn dokkomplekt_license_native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(native_core_version, module)?)?;
    module.add_function(wrap_pyfunction!(license_plan, module)?)?;
    module.add_function(wrap_pyfunction!(proof_ok, module)?)?;
    module.add_function(wrap_pyfunction!(access_decision, module)?)?;
    Ok(())
}
