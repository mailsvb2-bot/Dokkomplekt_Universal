from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGISTER = ROOT / "scripts" / "register_windows_hardware_runner.ps1"
BOOTSTRAP = ROOT / "scripts" / "bootstrap_windows_hardware_runner.ps1"
PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_runner.ps1"
CLEANUP = ROOT / "scripts" / "cleanup_windows_reboot_preparation.ps1"
DISPATCHER = ROOT / "scripts" / "dispatch_private_hardware_validation.py"
PUBLIC_WORKFLOW = ROOT / ".github" / "workflows" / "windows-hardware-e2e.yml"
PRIVATE_WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
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


def test_host_preflight_checks_real_hardware_dependencies() -> None:
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


def test_private_workflow_owns_self_hosted_and_two_phase_reboot() -> None:
    text = read(PRIVATE_WORKFLOW)
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-hardware-e2e]" in text
    assert "environment: windows-production-signing" in text
    assert "run-name:" in text and "inputs.request_id" in text
    assert "source_repository:" in text
    assert "release_sha:" in text
    assert "reboot_phase:" in text
    assert "request_id:" in text
    assert "verify_windows_hardware_runner.ps1" in text
    assert "DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT" in text
    assert "DOKKOMPLEKT_PREPARE_REBOOT_E2E" in text
    assert "DOKKOMPLEKT_REBOOT_PREP_ROOT" in text
    assert "cleanup_windows_reboot_preparation.ps1" in text
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


def test_hardware_runner_runbook_describes_private_security_boundary() -> None:
    text = read(DOC)
    assert "windows-production-signing" in text
    assert "windows-hardware-dispatch" in text
    assert "private" in text.lower()
    assert "must **not** be registered" in text
    assert "DOKKOMPLEKT_HARDWARE_VALIDATION_REPOSITORY" in text
    assert "DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN" in text
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" in text
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64" in text
    assert "register_windows_hardware_runner.ps1" in text
    assert "prepare" in text
    assert "production-hardware" in text
