use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyMigrationUnit {
    pub legacy_file: String,
    pub target_layer: String,
    pub status: String,
    pub notes: String,
}

/// High-level parity matrix. Full file-by-file inventory is generated in docs/LEGACY_MIGRATION_INVENTORY.json.
pub fn required_parity_groups() -> Vec<LegacyMigrationUnit> {
    vec![
        unit(
            "app.py / window.py / *_mixin.py",
            "src + src-tauri",
            "migrated-shell",
            "UI shell moved to TypeScript/Tauri commands; business rules removed from UI",
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
            "Merged popup plan, validation, shared fields",
        ),
        unit(
            "diary_*",
            "dokkomplekt-core::diary_engine + dokkomplekt-docx",
            "migrated-core",
            "Admission+1 schedule, discharge stop, signatures",
        ),
        unit(
            "desktop_intake_agent.py",
            "dokkomplekt-core::intake_agent + src-tauri platform shell",
            "migrated-core-contract",
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
            "dokkomplekt-core::medical_profile + SQLite import seam",
            "seeded-migration",
            "API ready; full dictionary must be imported as data, not hardcoded into UI",
        ),
    ]
}

fn unit(legacy_file: &str, target_layer: &str, status: &str, notes: &str) -> LegacyMigrationUnit {
    LegacyMigrationUnit {
        legacy_file: legacy_file.into(),
        target_layer: target_layer.into(),
        status: status.into(),
        notes: notes.into(),
    }
}
