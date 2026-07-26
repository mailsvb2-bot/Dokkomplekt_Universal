use dokkomplekt_core::{minimum_rewrite_layers_present, rewrite_coverage_summary};

#[test]
fn rewrite_declares_real_legacy_scope() {
    let summary = rewrite_coverage_summary();
    assert_eq!(summary.legacy_python_files, 294);
    assert_eq!(summary.legacy_test_files, 100);
    assert!(summary.legacy_python_lines > 45_000);
}

#[test]
fn all_required_new_layers_are_accounted_for() {
    let layers = [
        "tauri_shell",
        "typescript_ui",
        "domain_core",
        "workflow_engine",
        "popup_engine",
        "template_intelligence_engine",
        "document_generation_engine",
        "docx_openxml_adapter",
        "scanner_engine",
        "diary_engine",
        "intake_agent",
        "sqlite_storage",
        "domain_profiles",
        "icd10_catalog",
        "golden_master_tests",
        "webdriver_e2e_tests",
        "installer_tests",
    ];
    assert!(minimum_rewrite_layers_present(&layers).is_ok());
}
