from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_kit_learning_requires_evidence_before_auto_apply():
    text = (ROOT / "crates/dokkomplekt-core/src/kit_learning.rs").read_text(encoding="utf-8")
    assert "min_observations: 8" in text
    assert "min_consecutive_clean: 8" in text
    assert "min_accuracy: 0.98" in text
    assert "rule.promoted" in text
    assert "correction_resets_clean_streak_and_blocks_auto_apply" in text


def test_kit_learning_is_exported_from_core():
    text = (ROOT / "crates/dokkomplekt-core/src/lib.rs").read_text(encoding="utf-8")
    assert "pub mod kit_learning;" in text
    assert "pub use kit_learning::*;" in text


def test_ground_truth_metrics_and_calibration_are_present():
    measure = (ROOT / "scripts/measure_domain.py").read_text(encoding="utf-8")
    calibrate = (ROOT / "scripts/calibrate_thresholds.py").read_text(encoding="utf-8")
    assert '"field_accuracy"' in measure
    assert '"kit_completeness"' in measure
    assert '"auto_bucket_error_rate"' in measure
    assert '"source_of_truth": "specialist_final_accepted"' in calibrate
    assert "held-out auto-bucket error rate" in calibrate


def test_kit_learning_is_wired_into_live_zero_touch_runtime():
    runtime = (ROOT / "src-tauri/src/subsystems/automation_runtime.rs").read_text(encoding="utf-8")
    intake = (ROOT / "src-tauri/src/subsystems/source_intake_commands.rs").read_text(encoding="utf-8")
    main = (ROOT / "src-tauri/src/main.rs").read_text(encoding="utf-8")
    corpus = (ROOT / "crates/dokkomplekt-core/src/corpus_recorder.rs").read_text(encoding="utf-8")
    assert "resolve_document_bundle_for_case(" in runtime
    assert "decision_for_key(&corpus_entries" in intake
    assert "decide_document_bundle(" in intake
    assert "learned.as_ref()" in intake
    assert "if bundle_decision.review_required" in runtime
    assert "let selected_document_ids = bundle_decision" in runtime
    assert "get_learned_kit_decision" in main
    assert "pub cluster_id: Option<String>" in corpus

def test_desktop_workspace_excludes_server_and_no_local_security_forks_remain():
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    audit = (ROOT / ".cargo/audit.toml").read_text(encoding="utf-8")
    assert 'exclude = [' in cargo
    assert '"crates/dokkomplekt-license-server"' in cargo
    assert '[patch.crates-io]' not in cargo
    assert 'vendor/time' not in cargo
    assert 'RUSTSEC-2026-0009' not in audit
    assert 'rust-version = "1.97.1"' in cargo
