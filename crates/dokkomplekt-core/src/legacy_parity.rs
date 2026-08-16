use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyMigrationUnit {
    pub legacy_file: String,
    pub target_layer: String,
    pub status: String,
    pub notes: String,
}

pub const LEGACY_MIGRATION_INVENTORY_JSON: &str =
    include_str!("../../../docs/LEGACY_MIGRATION_INVENTORY.json");

/// High-level parity matrix. The canonical file-level evidence is the checked-in
/// JSON inventory above and is validated in CI without network access.
pub fn required_parity_groups() -> Vec<LegacyMigrationUnit> {
    vec![
        unit(
            "app.py / window.py / *_mixin.py",
            "src + src-tauri",
            "superseded",
            "Tkinter shell replaced by TypeScript/Tauri; business rules live in Rust",
        ),
        unit(
            "universal_template_engine.py / window_document_mapper.py",
            "dokkomplekt-core::template_intelligence + button_registry",
            "migrated-core",
            "Template title/role/placeholder analysis and button creation",
        ),
        unit(
            "actions_required_fields_popup.py / dialog_*",
            "dokkomplekt-core::popup_engine + workflow_engine",
            "migrated-core",
            "Merged popup plan, validation and shared fields",
        ),
        unit(
            "diary_*",
            "dokkomplekt-core::diary_engine + professional_records + dokkomplekt-docx",
            "migrated-domain-profile",
            "Medical diary adapter retains D0+1, discharge stop, specialist texts and signatures",
        ),
        unit(
            "desktop_intake_agent.py",
            "dokkomplekt-core::intake_agent + src-tauri platform shell",
            "migrated-core",
            "Dedup/single-instance rules moved to Rust contracts",
        ),
        unit(
            "universal_scanner.py",
            "dokkomplekt-core::scanner_engine",
            "migrated-core",
            "Manual field mapping has explicit priority below user popup",
        ),
        unit(
            "icd10_f*.py",
            "Medical profile reference data seam",
            "migrated-domain-profile",
            "Dictionary is profile data, not universal UI logic",
        ),
    ]
}

pub fn validate_legacy_migration_inventory() -> Result<(), String> {
    let root: serde_json::Value =
        serde_json::from_str(LEGACY_MIGRATION_INVENTORY_JSON).map_err(|error| error.to_string())?;
    if root.get("schema").and_then(|value| value.as_str())
        != Some("dokkomplekt.legacy-migration-inventory.v1")
    {
        return Err("legacy migration inventory schema is missing or unsupported".into());
    }
    let allowed = root
        .get("allowed_statuses")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "allowed_statuses missing".to_string())?
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let expected = [
        "migrated-core",
        "migrated-domain-profile",
        "superseded",
        "not-needed",
        "missing",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if allowed != expected {
        return Err("legacy migration inventory status vocabulary changed".into());
    }
    let donors = root
        .get("donors")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "donors missing".to_string())?;
    for donor in ["Dokkomplekt", "diary-filler"] {
        let entry = donors
            .get(donor)
            .and_then(|value| value.as_object())
            .ok_or_else(|| format!("donor {donor} missing"))?;
        let commit = entry
            .get("source_commit")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if commit.len() != 40 || !commit.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!("donor {donor} has no pinned source commit"));
        }
    }

    let entries = root
        .get("entries")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "entries missing".to_string())?;
    let mut unique = BTreeSet::new();
    let mut indexed = BTreeSet::new();
    let mut missing = Vec::new();
    for entry in entries {
        let donor = entry
            .get("donor")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let path = entry
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let status = entry
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let target = entry
            .get("target")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if !donors.contains_key(donor)
            || path.is_empty()
            || target.is_empty()
            || !allowed.contains(status)
        {
            return Err(format!(
                "invalid legacy inventory entry: {donor}:{path}:{status}"
            ));
        }
        let key = format!("{donor}:{path}");
        if !unique.insert(key.clone()) {
            return Err(format!("duplicate legacy inventory entry: {key}"));
        }
        indexed.insert(key);
        if status == "missing" {
            missing.push(format!("{donor}:{path}"));
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "legacy migration still has missing behavior: {}",
            missing.join(", ")
        ));
    }

    // These are the donor seams most likely to disappear silently during a UI
    // rewrite. CI requires them explicitly instead of trusting broad group labels.
    for required in [
        "Dokkomplekt:actions_creation_foldering.py",
        "Dokkomplekt:actions_required_fields_popup.py",
        "Dokkomplekt:desktop_intake_agent.py",
        "Dokkomplekt:dialog_document_details.py",
        "diary-filler:diary_template_selection.py",
        "diary-filler:diary_text_selection.py",
        "diary-filler:dnd_mixin.py",
        "diary-filler:layout_sources.py",
    ] {
        if !indexed.contains(required) {
            return Err(format!(
                "required donor seam missing from inventory: {required}"
            ));
        }
    }
    Ok(())
}

fn unit(legacy_file: &str, target_layer: &str, status: &str, notes: &str) -> LegacyMigrationUnit {
    LegacyMigrationUnit {
        legacy_file: legacy_file.into(),
        target_layer: target_layer.into(),
        status: status.into(),
        notes: notes.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_file_level_inventory_is_complete_and_uses_only_final_statuses() {
        validate_legacy_migration_inventory()
            .expect("legacy migration inventory must stay complete");
    }
}
