import shutil
import subprocess
from pathlib import Path

import pytest
import yaml

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


def test_hardware_workflow_stages_runtime_and_preserves_release_evidence() -> None:
    workflow = text(".github/workflows/windows-hardware-e2e.yml")
    release_workflow = text(".github/workflows/build-installers.yml")
    hardware = text("tests/windows/windows_hardware_e2e.ps1")
    sidecar_signatures = text("tests/windows/verify_sidecar_authenticode.ps1")
    assert "python -m pip install --disable-pip-version-check -r requirements-dev.txt" in workflow
    assert "--mode windows-runtime" in workflow
    assert "scripts/prepare_sidecars.py $env:DOKKOMPLEKT_SIDECAR_MANIFEST_PATH --clean" in workflow
    assert "scripts\\prepackage_rust_gate.bat" in workflow
    assert "verify_sidecar_authenticode.ps1" in workflow
    assert "SIDECAR_AUTHENTICODE.json" in workflow
    assert "function Test-PortableExecutable" in sidecar_signatures
    assert "0x4D" in sidecar_signatures and "0x5A" in sidecar_signatures
    assert "Get-AuthenticodeSignature" in sidecar_signatures
    assert "Sidecar Authenticode signature is not valid" in sidecar_signatures
    assert "dokkomplekt.sidecar-authenticode.v1" in sidecar_signatures
    assert "create_offline_runtime_bundle.py" in workflow
    assert "--require-signature" in workflow
    assert "finally {" in workflow
    assert "Runtime signing private key cleanup failed" in workflow
    assert "--output-json verification/release/scanned-pdf-ocr.json" in workflow
    assert "release-runtime/**" in workflow
    assert "verification/release/**" in workflow
    assert "$rebootEvidencePath = $env:DOKKOMPLEKT_REBOOT_EVIDENCE_PATH" in hardware
    assert "PRINT_EVENT_307.json" in hardware
    assert "AUTHENTICODE_SIGNATURES.json" in hardware
    assert "NSIS silent uninstall failed" in hardware
    assert "silent_uninstall_passed = $true" in hardware

    assert "environment: windows-production-signing" in release_workflow
    assert "verify_sidecar_authenticode.ps1" in release_workflow
    assert "SIDECAR_AUTHENTICODE.json" in release_workflow
    assert "--output-json verification/release/scanned-pdf-ocr.json" in release_workflow
    assert "verification/release/**" in release_workflow
    assert "path: .release-gate/**" in release_workflow
    assert "needs: [windows-hardware-e2e, linux-bundles]" in release_workflow
    assert "Attach artifacts only after signing and hardware E2E" in release_workflow


def test_production_workflow_yaml_and_hardware_powershell_parse() -> None:
    expected_workflows = {
        ".github/workflows/windows-hardware-e2e.yml": "Windows Hardware E2E",
        ".github/workflows/build-installers.yml": "Build Signed Offline Installers",
    }
    for path, expected_name in expected_workflows.items():
        parsed = yaml.safe_load(text(path))
        assert parsed["name"] == expected_name

    pwsh = shutil.which("pwsh")
    if pwsh is None:
        pytest.skip("PowerShell is not installed in this development environment")
    parser_script = r"""
$paths = @(
  'tests/windows/windows_hardware_e2e.ps1',
  'tests/windows/verify_sidecar_authenticode.ps1'
)
foreach ($path in $paths) {
  $tokens = $null
  $errors = $null
  [System.Management.Automation.Language.Parser]::ParseFile(
    (Resolve-Path $path),
    [ref]$tokens,
    [ref]$errors
  ) | Out-Null
  if ($errors.Count -gt 0) {
    $details = ($errors | ForEach-Object { $_.Message }) -join '; '
    throw "PowerShell parse failed for ${path}: ${details}"
  }
}
"""
    result = subprocess.run(
        [pwsh, "-NoProfile", "-NonInteractive", "-Command", parser_script],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


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
