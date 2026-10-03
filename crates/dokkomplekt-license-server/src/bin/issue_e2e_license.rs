use anyhow::{Context, Result};
use dokkomplekt_license_core::PlanId;
use std::path::PathBuf;
use uuid::Uuid;

#[path = "../issuer.rs"]
mod issuer;

use issuer::{issue_license, IssueLicenseInput};

fn required_arg(name: &str) -> Result<String> {
    let prefix = format!("--{name}=");
    std::env::args()
        .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_string))
        .with_context(|| format!("missing {prefix}<value>"))
}

fn main() -> Result<()> {
    let private_key_file = PathBuf::from(required_arg("private-key-file")?);
    let output = PathBuf::from(required_arg("output")?);
    let issuer_key_b64 =
        std::fs::read_to_string(&private_key_file).context("read ephemeral FPR-18 private key")?;
    let document = issue_license(
        IssueLicenseInput {
            order_id: Uuid::new_v4(),
            plan: PlanId::DoctorPro,
            owner_name: Some("FPR-18 Installed Proof".to_string()),
            organization_name: None,
            allowed_machines: vec![],
            valid_days: 30,
            product_id: "dokkomplekt_universal".to_string(),
            owner_unlimited: false,
        },
        "fpr18-e2e-issuer",
        issuer_key_b64.trim(),
    )
    .context("issue canonical FPR-18 license")?;
    let bytes = serde_json::to_vec_pretty(&document).context("serialize FPR-18 license")?;
    std::fs::write(&output, bytes).context("write FPR-18 license fixture")?;
    Ok(())
}
