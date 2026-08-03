from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_legal_identifiers_and_email_fail_closed() -> None:
    validators = text("crates/dokkomplekt-core/src/validators.rs")
    assert "fn normalize_digit_text" in validators
    assert "value.matches('@').count() != 1" in validators
    assert "digit_identifiers_reject_hidden_letters_and_symbols" in validators


def test_license_server_defaults_to_strict_and_validates_public_origin() -> None:
    config = text("crates/dokkomplekt-license-server/src/config.rs")
    assert "!(development_mode && explicit_insecure_opt_in)" in config
    assert "validate_public_base_url" in config
    assert 'payment_provider != "yookassa"' in config
    assert "unsupported payment provider" in config


def test_failed_payment_creation_is_recoverable_and_stubs_fail_closed() -> None:
    orders = text("crates/dokkomplekt-license-server/src/http/orders.rs")
    sbp = text("crates/dokkomplekt-license-server/src/provider_sbp.rs")
    assert "/api/orders/:order_id/payment" in orders
    assert '"retry_required"' in orders
    assert "authorize_order" in orders
    assert "ProviderError::Unsupported" in sbp


def test_payment_webhook_duplicate_is_atomic_and_order_bound() -> None:
    postgres = text("crates/dokkomplekt-license-server/src/storage/postgres.rs")
    memory = text("crates/dokkomplekt-license-server/src/memory_store.rs")
    assert "ON CONFLICT (provider, provider_event_id) DO NOTHING" in postgres
    assert "provider_event_order_mismatch" in postgres
    assert "provider_event_order_mismatch" in memory


def test_signing_workflow_is_approval_and_commit_pinned() -> None:
    workflow = text(".github/workflows/windows-hardware-e2e.yml")
    assert "environment: windows-production-signing" in workflow
    assert "release_sha:" in workflow
    assert "persist-credentials: false" in workflow
    assert "DOKKOMPLEKT_SIGNING_SCRIPT_SHA256" in workflow
    assert "git merge-base --is-ancestor" in workflow


def test_production_csp_excludes_dev_server_and_dev_overlay_is_explicit() -> None:
    import json

    production = json.loads(text("src-tauri/tauri.conf.json"))["app"]["security"]["csp"]
    development = text("src-tauri/tauri.dev.conf.json")
    package = text("package.json")
    assert "ws://127.0.0.1:1420" not in production
    assert "http://127.0.0.1:1420" not in production
    assert "ws://127.0.0.1:1420" in development
    assert "tauri.dev.conf.json" in package


def test_stale_versioned_ci_success_snapshot_is_removed() -> None:
    assert not (ROOT / "docs/provenance/GITHUB_ACTIONS_EVIDENCE_18_4_3.json").exists()
    policy = text("docs/provenance/PROVENANCE_POLICY.md")
    assert "Versioned `GITHUB_ACTIONS_EVIDENCE_*.json` snapshots are forbidden" in policy
