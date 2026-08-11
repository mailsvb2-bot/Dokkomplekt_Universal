from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGISTER = ROOT / "scripts" / "register_windows_hardware_runner.ps1"
BOOTSTRAP = ROOT / "scripts" / "bootstrap_windows_hardware_runner.ps1"
PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_runner.ps1"
EVIDENCE_PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"
CLEANUP = ROOT / "scripts" / "cleanup_windows_reboot_preparation.ps1"
DISPATCHER = ROOT / "scripts" / "dispatch_private_hardware_validation.py"
PUBLIC_WORKFLOW = ROOT / ".github" / "workflows" / "windows-hardware-e2e.yml"
PRIVATE_WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
APPROVAL = ROOT / "scripts" / "windows_runtime_bundle_approval.py"
STAGER = ROOT / "scripts" / "stage_signed_runtime_bundle.py"
HANDOFF = ROOT / "scripts" / "windows_signed_handoff.py"
DOC = ROOT / "docs" / "WINDOWS_HARDWARE_RUNNER.md"


def read(path: Path) -> str:
    assert path.is_file(), f"missing required hardware-runner file: {path}"
    return path.read_text(encoding="utf-8")


def test_registration_entrypoint_prompts_securely_and_rejects_public_repo() -> None:
    text = read(REGISTER)
    assert "Read-Host 'GitHub self-hosted runner registration token' -AsSecureString" in text
    assert "[Net.NetworkCredential]::new('', $secureToken)" in text
    assert "bootstrap_windows_hardware_runner.ps1" in text
    assert "Refusing to register a persistent hardware runner in the public Dokkomplekt_Universal repository" in text
    assert "[Parameter(Mandatory = $true)] [string] $RepositoryUrl" in text
    assert "$plainToken = $null" in text
    assert "$secureToken.Dispose()" in text


def test_bootstrap_forbids_service_mode_and_uses_interactive_task() -> None:
    text = read(BOOTSTRAP)
    assert "actions.runner.*" in text
    assert "Session 0/service execution is forbidden" in text
    assert "New-ScheduledTaskTrigger -AtLogOn" in text
    assert "New-ScheduledTaskPrincipal" in text
    assert "-LogonType Interactive" in text
    assert "Runner.Listener" in text
    assert "dokkomplekt-hardware-e2e" in text


def test_bootstrap_pins_downloaded_runner_by_release_asset_digest() -> None:
    text = read(BOOTSTRAP)
    assert "https://api.github.com/repos/actions/runner/releases/latest" in text
    assert "asset.digest" in text
    assert "Get-FileHash" in text
    assert "GitHub runner package SHA-256 mismatch" in text


def test_host_preflight_checks_legacy_full_hardware_dependencies() -> None:
    text = read(PREFLIGHT)
    for required in (
        "interactive-user-session",
        "actions-runner-not-service",
        "microsoft-word-com",
        "dedicated-real-printer",
        "printservice-operational-log",
        "visual-studio-vctools",
        "webview2-runtime",
        "runner-owned-sidecar-manifest",
        "openssl",
    ):
        assert required in text


def test_new_hardware_evidence_preflight_is_side_effect_free_before_handoff_verification() -> None:
    text = read(EVIDENCE_PREFLIGHT)
    for required in (
        "interactive-user-session",
        "actions-runner-not-service",
        "dedicated-printer-name-configured",
        "hardware-probes-deferred-until-signed-handoff",
        "visual-studio-vctools",
        "webview2-runtime",
        "runtime-manifest-not-exposed",
        "signing-secrets-not-exposed",
        "runtime_manifest_env_exposed",
        "signing_secret_env_exposed",
        "hardware_probes_deferred_until_signed_handoff",
    ):
        assert required in text
    assert "[Parameter(Mandatory = $true)] [string] $SidecarManifestPath" not in text
    assert "runner-owned-sidecar-manifest" not in text
    for forbidden_hardware_probe in (
        "New-Object -ComObject Word.Application",
        "Get-Printer -Name",
        "Get-PrinterPort -Name",
        "wevtutil sl Microsoft-Windows-PrintService/Operational",
    ):
        assert forbidden_hardware_probe not in text
    for forbidden_secret in (
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert forbidden_secret in text


def test_public_workflow_never_targets_self_hosted_runner() -> None:
    text = read(PUBLIC_WORKFLOW)
    assert "runs-on: ubuntu-latest" in text
    assert "runs-on: [self-hosted" not in text
    assert "windows-hardware-dispatch" in text
    assert "DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY" in text
    assert "DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN" in text
    assert "dispatch_private_hardware_validation.py" in text


def test_dispatcher_requires_a_separate_private_target_and_correlates_runs() -> None:
    text = read(DISPATCHER)
    assert 'target.get("private") is not True' in text
    assert "hardware validation target must be a separate private repository" in text
    assert "request_id = str(uuid.uuid4())" in text
    assert "request_id not in display_title" in text
    assert '"source_repository": args.source_repository' in text
    assert '"release_sha": args.release_sha' in text
    assert '"reboot_phase": args.reboot_phase' in text


def test_private_workflow_requires_only_one_physical_windows_runner() -> None:
    text = read(PRIVATE_WORKFLOW)
    assert "signed-runtime-build:" in text
    assert "hardware-evidence:" in text
    assert "runs-on: windows-latest" in text
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-hardware]" in text
    assert "dokkomplekt-runtime]" not in text
    assert text.count("runs-on: [self-hosted") == 1
    assert "environment: windows-production-signing" in text
    assert "environment: windows-hardware-validation" in text
    assert "needs: signed-runtime-build" in text
    assert "Dokkomplekt-Windows-Signed-Handoff-" in text
    assert "actions/upload-artifact@" in text
    assert "actions/download-artifact@" in text

    runtime_start = text.index("  signed-runtime-build:")
    hardware_start = text.index("  hardware-evidence:")
    runtime = text[runtime_start:hardware_start]
    hardware = text[hardware_start:]
    for secret_name in (
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert secret_name in runtime
        assert secret_name not in hardware
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" not in text
    assert "verify_windows_hosted_signing_runner.py" in runtime
    assert "verify_windows_hardware_evidence_host.ps1" in hardware
    assert "verify_windows_hardware_runner.ps1" not in hardware


def test_hosted_workflow_requires_preapproved_signed_runtime_before_staging() -> None:
    workflow = read(PRIVATE_WORKFLOW)
    approval = read(APPROVAL)
    stager = read(STAGER)
    runtime = workflow[workflow.index("  signed-runtime-build:"):workflow.index("  hardware-evidence:")]
    for required in (
        "DOKKOMPLEKT_RUNTIME_BUNDLE_URL",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_PAYLOAD_URL",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_SIGNATURE_URL",
        "DOKKOMPLEKT_RUNTIME_BUNDLE_APPROVAL_SIGNATURE_URL",
        "DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PUBKEY_PEM_B64",
        "fetch_hosted_runtime_bundle.py",
        "stage_signed_runtime_bundle.py",
        "HOSTED_RUNTIME_STAGE.json",
    ):
        assert required in runtime
    assert runtime.index("fetch_hosted_runtime_bundle.py") < runtime.index("stage_signed_runtime_bundle.py")
    assert runtime.index("stage_signed_runtime_bundle.py") < runtime.index("assert_offline_runtime_ready.py")
    assert "Ed25519PrivateKey" in approval
    assert "Ed25519PublicKey" in approval
    assert "private_key_present_in_ci" in approval
    assert "DOKKOMPLEKT_RUNTIME_LOCK_APPROVAL_PRIVATE" not in workflow
    assert "runtime release signature verification failed" in stager
    assert "offline_approval_verified" in stager
    assert "runtime bundle file set mismatch" in stager


def test_signed_handoff_is_verified_before_hardware_execution() -> None:
    workflow = read(PRIVATE_WORKFLOW)
    handoff = read(HANDOFF)
    hardware = workflow[workflow.index("  hardware-evidence:"):]
    assert "windows_signed_handoff.py build" in workflow
    assert "windows_signed_handoff.py verify" in hardware
    assert "SIGNED_HANDOFF_VERIFICATION.json" in hardware
    assert hardware.index("windows_signed_handoff.py verify") < hardware.index("windows_hardware_e2e.ps1")
    assert "Get-AuthenticodeSignature" in hardware
    assert "verify_offline_runtime_bundle.py" in hardware
    assert "Ed25519PrivateKey" in handoff
    assert "Ed25519PublicKey" in handoff
    assert "handoff file set mismatch" in handoff
    assert "handoff sha256 mismatch" in handoff


def test_private_workflow_owns_two_phase_reboot_only_on_hardware_job() -> None:
    text = read(PRIVATE_WORKFLOW)
    hardware = text[text.index("  hardware-evidence:"):]
    assert "run-name:" in text and "inputs.request_id" in text
    assert "DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT" in hardware
    assert "DOKKOMPLEKT_PREPARE_REBOOT_E2E" in hardware
    assert "DOKKOMPLEKT_REBOOT_PREP_ROOT" in hardware
    assert "cleanup_windows_reboot_preparation.ps1" in hardware
    assert "signed_handoff_manifest_sha256" in hardware
    assert "https://github.com/${{ inputs.source_repository }}.git" in text


def test_reboot_prepare_state_is_persistent_and_cleanup_is_bounded() -> None:
    workflow = read(PRIVATE_WORKFLOW)
    cleanup = read(CLEANUP)
    assert "ProgramData" in workflow
    assert "DokkomplektE2E" in workflow
    assert "RUNNER_TEMP" in workflow
    assert "TEMP" in workflow
    assert "Assert-UnderDokkomplektProgramData" in cleanup
    assert "--background-watch" in cleanup
    assert "Prepared NSIS uninstall" in cleanup


def test_hardware_runner_runbook_describes_single_physical_machine_boundary() -> None:
    text = read(DOC)
    assert "windows-production-signing" in text
    assert "windows-hardware-validation" in text
    assert "windows-hardware-dispatch" in text
    assert "private" in text.lower()
    assert "GitHub-hosted" in text
    assert "одна физическая Windows-машина" in text
    assert "DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY" in text
    assert "DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN" in text
    assert "DOKKOMPLEKT_RUNTIME_BUNDLE_URL" in text
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64" in text
    assert "dokkomplekt-hardware" in text
    assert "SIGNED_HANDOFF.json" in text
    assert "prepare" in text
    assert "production-hardware" in text
