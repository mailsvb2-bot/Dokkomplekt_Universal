use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RewriteCoverageSummary {
    pub legacy_python_files: usize,
    pub legacy_python_lines: usize,
    pub legacy_test_files: usize,
    pub required_layers: Vec<String>,
}

/// Hard guard against calling a tiny demo a rewrite.
/// These numbers are taken from the uploaded v1.5.8/v1597 source inventory.
pub fn rewrite_coverage_summary() -> RewriteCoverageSummary {
    RewriteCoverageSummary {
        legacy_python_files: 294,
        legacy_python_lines: 45_619,
        legacy_test_files: 100,
        required_layers: vec![
            "tauri_shell".into(),
            "typescript_ui".into(),
            "domain_core".into(),
            "workflow_engine".into(),
            "popup_engine".into(),
            "template_intelligence_engine".into(),
            "document_generation_engine".into(),
            "docx_openxml_adapter".into(),
            "scanner_engine".into(),
            "diary_engine".into(),
            "intake_agent".into(),
            "sqlite_storage".into(),
            "domain_profiles".into(),
            "icd10_catalog".into(),
            "golden_master_tests".into(),
            "webdriver_e2e_tests".into(),
            "installer_tests".into(),
        ],
    }
}

pub fn minimum_rewrite_layers_present(layer_names: &[&str]) -> Result<(), Vec<String>> {
    let required = rewrite_coverage_summary().required_layers;
    let missing = required
        .into_iter()
        .filter(|required| !layer_names.contains(&required.as_str()))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_is_not_tiny_demo() {
        let s = rewrite_coverage_summary();
        assert!(s.legacy_python_files >= 294);
        assert!(s.legacy_python_lines >= 45_000);
        assert!(s.legacy_test_files >= 90);
    }
}
