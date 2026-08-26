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
    assert 'matches!(payment_provider.as_str(), "yookassa" | "sbp")' in config
    assert "if strict_runtime && !uses_yookassa_api" in config
    assert "strict_runtime && uses_yookassa_api" in config
    assert "unsupported payment provider" in config


def test_failed_payment_creation_is_recoverable_and_unverified_providers_fail_closed() -> None:
    orders = text("crates/dokkomplekt-license-server/src/http/orders.rs")
    sbp = text("crates/dokkomplekt-license-server/src/provider_sbp.rs")
    assert "/api/orders/:order_id/payment" in orders
    assert '"retry_required"' in orders
    assert "authorize_order" in orders
    assert "YooKassaProvider" in sbp
    assert "create_sbp_payment" in sbp
    assert '"bank_invoice" => Err(ProviderCallError::Provider(' in orders
    assert "bank invoice provider is not implemented and cannot create payments" in orders


def test_payment_webhook_duplicate_is_atomic_and_order_bound() -> None:
    postgres = text("crates/dokkomplekt-license-server/src/storage/postgres.rs")
    memory = text("crates/dokkomplekt-license-server/src/memory_store.rs")
    assert "ON CONFLICT (provider, provider_event_id) DO NOTHING" in postgres
    assert "provider_event_order_mismatch" in postgres
    assert "provider_event_order_mismatch" in memory


def test_signing_workflow_is_private_approval_and_commit_pinned() -> None:
    public_bridge = text(".github/workflows/windows-hardware-e2e.yml")
    workflow = text("ops/private-hardware-validation/windows-hardware-e2e.yml")
    assert "runs-on: ubuntu-latest" in public_bridge
    assert "runs-on: [self-hosted" not in public_bridge
    assert "environment: windows-hardware-dispatch" in public_bridge
    assert "dispatch_private_hardware_validation.py" in public_bridge
    assert "persist-credentials: false" in public_bridge
    assert "environment: windows-production-signing" in workflow
    assert "environment: windows-hardware-validation" in workflow
    assert "release_sha:" in workflow
    assert "DOKKOMPLEKT_SIGNING_SCRIPT_SHA256" in workflow
    assert "git merge-base --is-ancestor" in workflow
    assert "unexpected source repository" in workflow
    assert "https://github.com/${{ inputs.source_repository }}.git" in workflow
    assert "runs-on: windows-latest" in workflow
    assert "self-hosted, Windows, X64, dokkomplekt-runtime" not in workflow
    assert workflow.count("runs-on: [self-hosted") == 1


def test_hardware_workflow_stages_runtime_and_preserves_release_evidence() -> None:
    workflow = text("ops/private-hardware-validation/windows-hardware-e2e.yml")
    handoff = text("scripts/windows_signed_handoff.py")
    release_workflow = text(".github/workflows/build-installers.yml")
    hardware = text("tests/windows/windows_hardware_e2e.ps1")
    sidecar_signatures = text("tests/windows/verify_sidecar_authenticode.ps1")
    evidence_index = text("scripts/write_windows_hardware_evidence_index.ps1")
    release_evidence = text("scripts/write_windows_release_evidence.ps1")

    assert "python -m pip install --disable-pip-version-check -r requirements-dev.txt" in workflow
    assert "verify_windows_hosted_signing_runner.py" in workflow
    assert "fetch_hosted_runtime_bundle.py" in workflow
    assert "stage_signed_runtime_bundle.py" in workflow
    assert "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL" in workflow
    assert "DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64" in workflow
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" not in workflow
    assert "--mode windows-runtime" not in workflow
    assert "scripts\\prepackage_rust_gate.bat" in workflow
    assert "verify_sidecar_authenticode.ps1" in workflow
    assert "SIDECAR_AUTHENTICODE.json" in workflow
    assert "function Test-PortableExecutable" in sidecar_signatures
    assert "0x4D" in sidecar_signatures and "0x5A" in sidecar_signatures
    assert "Get-AuthenticodeSignature" in sidecar_signatures
    assert "Sidecar Authenticode signature is not valid" in sidecar_signatures
    assert "dokkomplekt.sidecar-authenticode.v1" in sidecar_signatures
    # Runtime composition is reviewed and signed before hosted CI. CI must not
    # silently recreate a different release runtime from local files.
    assert "create_offline_runtime_bundle.py" not in workflow
    assert "HOSTED_SIGNING_PREFLIGHT.json" in workflow
    assert "HOSTED_RUNTIME_FETCH.json" in workflow
    assert "HOSTED_RUNTIME_STAGE.json" in workflow
    assert "finally {" in workflow
    assert "Remove-Item -LiteralPath $privateKey -Force" in workflow
    assert "if (Test-Path -LiteralPath $privateKey)" in workflow
    assert "TRANSFERRED_GATE_DIRS" in handoff
    assert "stage_repository_gate_evidence" in handoff
    assert "restore_verified_build_evidence" in handoff
    assert "--output-json verification/release/scanned-pdf-ocr.json" in workflow
    assert "source/signed-handoff/**" in workflow
    assert "source/verification/release/**" in workflow
    assert "$rebootEvidencePath = $env:DOKKOMPLEKT_REBOOT_EVIDENCE_PATH" in hardware
    assert "PRINT_EVENT_307.json" in hardware
    assert "AUTHENTICODE_SIGNATURES.json" in hardware
    assert "NSIS silent uninstall failed" in hardware
    assert "silent_uninstall_passed = $true" in hardware
    assert "write_windows_hardware_evidence_index.ps1" in workflow
    assert "WINDOWS_HARDWARE_EVIDENCE_INDEX.json" in evidence_index
    assert "offline-runtime-approval-signature" in evidence_index
    assert "protected_pinned_public_key" in release_evidence
    assert "--public-key $runtimePublicKey" not in release_evidence

    assert "environment: windows-production-signing" in release_workflow
    assert "runs-on: windows-latest" in release_workflow
    assert "self-hosted, Windows, X64, dokkomplekt-runtime" not in release_workflow
    assert "stage_signed_runtime_bundle.py" in release_workflow
    assert "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL" in release_workflow
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
        "ops/private-hardware-validation/windows-hardware-e2e.yml": "Dokkomplekt Private Windows Hardware E2E",
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
  'tests/windows/verify_sidecar_authenticode.ps1',
  'scripts/write_windows_hardware_evidence_index.ps1'
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


def test_final_windows_hardware_evidence_index_is_fail_closed() -> None:
    script = text("scripts/write_windows_hardware_evidence_index.ps1")
    identity = text("scripts/release_source_identity.py")
    workflow = text("ops/private-hardware-validation/windows-hardware-e2e.yml")
    handoff = text("scripts/windows_signed_handoff.py")
    assert "dokkomplekt.windows-hardware-evidence-index.v1" in script
    assert "release_source_identity.py" in script
    assert "$env:GITHUB_SHA" not in script
    assert 'CANONICAL_REPOSITORY = "mailsvb2-bot/Dokkomplekt_Universal"' in identity
    assert 'git_value(root, "rev-parse", "--verify", "HEAD")' in identity
    assert 'git_value(root, "remote", "get-url", "origin")' in identity
    assert "Signed build evidence is not bound to the checked-out source repository" in script
    assert "Signed build evidence is not bound to the checked-out release SHA" in script
    assert "Signed build evidence is not bound to the current source fingerprint" in script
    assert "Hardware E2E evidence is not bound to the current source fingerprint" in script
    assert "GUI evidence application" in script
    assert "Hardware Authenticode application" in script
    assert "Pinned runtime public key" in script
    assert "Offline runtime approval signature" in script
    assert "Rust gate attestation" in script
    assert "Rust gate signature" in script
    assert "Hardware E2E required flag is not true" in script
    assert "GUI evidence must contain two titled launches" in script
    assert "Hardware E2E installer SHA-256 does not match" in script
    assert "Expected exactly one offline runtime ZIP" in script
    for required in (
        "WINDOWS_SIGNED_BUILD_PASSED.json",
        "WINDOWS_HARDWARE_E2E_PASSED.json",
        "GUI_AND_CONSOLE_EVIDENCE.json",
        "PRINT_EVENT_307.json",
        "AUTHENTICODE_SIGNATURES.json",
        "WINDOWS_REBOOT_E2E_PASSED.json",
        "WATCHER_INSTALL.json",
        "WATCHER_UNINSTALL.json",
        "CARGO_GATE_ATTESTATION.json",
        "CARGO_GATE_ATTESTATION.sig",
        "HOSTED_SIGNING_PREFLIGHT.json",
        "HOSTED_RUNTIME_FETCH.json",
        "HOSTED_RUNTIME_STAGE.json",
        "hardware-preflight.json",
        "sidecar-status.json",
        "SIDECAR_AUTHENTICODE.json",
        "offline-runtime-probe.log",
        "scanned-pdf-ocr.json",
        "runtime-trusted-public.pem",
    ):
        assert required in script
    assert "windows-runtime-preflight.json" not in script
    assert "write_windows_hardware_evidence_index.ps1" in workflow
    assert "Copy-Item signed-handoff/SIGNED_HANDOFF.json verification/release/SIGNED_HANDOFF.json -Force" in workflow
    assert "Copy-Item signed-handoff/SIGNED_HANDOFF.json.sig verification/release/SIGNED_HANDOFF.json.sig -Force" in workflow
    assert "source/verification/release/**" in workflow
    assert "source/.release-gate/**" in workflow
    assert '".cargo-gate": f"{BUILD_EVIDENCE_DIR}/cargo-gate"' in handoff
    assert '".release-gate": f"{BUILD_EVIDENCE_DIR}/release-gate"' in handoff
    assert "restore_verified_build_evidence(root)" in handoff
    assert 'verification_destination = repository / "verification" / "release"' in handoff
