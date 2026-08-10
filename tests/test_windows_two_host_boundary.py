from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUNTIME_REGISTER = ROOT / "scripts" / "register_windows_runtime_runner.ps1"
RUNTIME_BOOTSTRAP = ROOT / "scripts" / "bootstrap_windows_runtime_runner.ps1"
RUNTIME_PREFLIGHT = ROOT / "scripts" / "verify_windows_runtime_signing_host.ps1"
RUNTIME_ACL = ROOT / "scripts" / "grant_windows_runtime_service_access.ps1"
HARDWARE_REGISTER = ROOT / "scripts" / "register_windows_hardware_evidence_runner.ps1"
HARDWARE_BOOTSTRAP = ROOT / "scripts" / "bootstrap_windows_hardware_evidence_runner.ps1"
HARDWARE_PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"
PRIVATE_WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
HANDOFF = ROOT / "scripts" / "windows_signed_handoff.py"


def read(path: Path) -> str:
    assert path.is_file(), f"missing two-host boundary file: {path}"
    return path.read_text(encoding="utf-8")


def test_two_host_bootstraps_have_opposite_execution_modes() -> None:
    runtime = read(RUNTIME_BOOTSTRAP)
    hardware = read(HARDWARE_BOOTSTRAP)
    assert "dokkomplekt-runtime" in runtime and "--runasservice" in runtime
    assert "NT AUTHORITY\\NETWORK SERVICE" in runtime
    assert "S-1-5-20" in runtime
    assert "dokkomplekt-hardware" in hardware
    assert "New-ScheduledTaskTrigger -AtLogOn" in hardware
    assert "Session 0/service execution is forbidden" in hardware
    assert "actions.runner.*" in hardware


def test_runtime_service_access_is_bounded_before_registration() -> None:
    register = read(RUNTIME_REGISTER)
    bootstrap = read(RUNTIME_BOOTSTRAP)
    acl = read(RUNTIME_ACL)
    preflight = read(RUNTIME_PREFLIGHT)
    assert "C:\\ProgramData\\DokkomplektRuntime" in register
    assert "grant_windows_runtime_service_access.ps1" in register
    assert register.index("grant_windows_runtime_service_access.ps1") < register.index(
        "Read-Host 'GitHub self-hosted runner registration token' -AsSecureString"
    )
    assert "RUNTIME_SERVICE_ACL.json" in bootstrap
    assert "Run register_windows_runtime_runner.ps1 instead of bypassing" in bootstrap
    assert "bounded runtime root" in bootstrap.lower()
    assert "Assert-UnderRoot" in acl
    assert "escapes the bounded runtime root" in acl
    assert "icacls.exe" in acl
    assert "(OI)(CI)(RX)" in acl
    assert "S-1-5-20" in acl
    assert "SecurityIdentifier" in acl
    assert "bounded-runtime-service-acl" in preflight
    assert "runtime-service-identity" in preflight
    assert "identity.User.Value" in preflight


def test_runtime_bootstrap_owns_manifest_and_hardware_bootstrap_forbids_it() -> None:
    runtime = read(RUNTIME_BOOTSTRAP)
    hardware = read(HARDWARE_BOOTSTRAP)
    assert "SidecarManifestPath" in runtime
    assert "supply_chain_locked" in runtime
    assert "Offline runtime-lock approval signature is missing" in runtime
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" in hardware
    assert "Runtime/signing environment must not be exposed on hardware host" in hardware
    assert "SidecarManifestPath" not in hardware


def test_registration_tokens_are_prompted_as_secure_strings() -> None:
    for path in (RUNTIME_REGISTER, HARDWARE_REGISTER):
        text = read(path)
        assert "Read-Host 'GitHub self-hosted runner registration token' -AsSecureString" in text
        assert "[Net.NetworkCredential]::new('', $secureToken)" in text
        assert "$plainToken = $null" in text
        assert "$secureToken.Dispose()" in text


def test_runtime_and_hardware_preflights_prove_distinct_trust_domains() -> None:
    runtime = read(RUNTIME_PREFLIGHT)
    hardware = read(HARDWARE_PREFLIGHT)
    assert "actions-runner-service-mode" in runtime
    assert "runtime-service-identity" in runtime
    assert "runner-owned-approved-runtime-manifest" in runtime
    assert "bounded-runtime-service-acl" in runtime
    assert "hardware-only-environment-not-exposed" in runtime
    assert "machine_fingerprint_sha256" in runtime
    assert "interactive-user-session" in hardware
    assert "actions-runner-not-service" in hardware
    assert "microsoft-word-com" in hardware
    assert "dedicated-real-printer" in hardware
    assert "printservice-operational-log" in hardware
    assert "visual-studio-vctools" in hardware
    assert "runtime-manifest-not-exposed" in hardware
    assert "signing-secrets-not-exposed" in hardware
    assert "machine_fingerprint_sha256" in hardware


def test_private_workflow_splits_secrets_and_enforces_physical_separation() -> None:
    text = read(PRIVATE_WORKFLOW)
    runtime_start = text.index("  signed-runtime-build:")
    hardware_start = text.index("  hardware-evidence:")
    runtime = text[runtime_start:hardware_start]
    hardware = text[hardware_start:]
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-runtime]" in runtime
    assert "environment: windows-production-signing" in runtime
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-hardware]" in hardware
    assert "environment: windows-hardware-validation" in hardware
    assert "needs: signed-runtime-build" in hardware
    for secret in (
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert secret in runtime
        assert secret not in hardware
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" in runtime
    assert "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH" not in hardware
    assert "verify_windows_runtime_signing_host.ps1" in runtime
    assert "verify_windows_hardware_evidence_host.ps1" in hardware
    assert "--producer-host-id" in runtime
    assert "--consumer-host-id" in hardware
    assert "SIGNED_HANDOFF_VERIFICATION.json" in hardware


def test_signed_handoff_schema_binds_producer_and_rejects_same_host() -> None:
    text = read(HANDOFF)
    assert "dokkomplekt.windows-signed-handoff.v2" in text
    assert "producer_host_id" in text
    assert "consumer_host_id" in text
    assert "runtime/signing and hardware evidence must run on distinct Windows hosts" in text
    assert "hosts_distinct" in text
