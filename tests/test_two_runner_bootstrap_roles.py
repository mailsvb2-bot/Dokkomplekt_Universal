from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHARED = ROOT / "scripts" / "bootstrap_private_windows_runner.ps1"
RUNTIME = ROOT / "scripts" / "register_windows_runtime_runner.ps1"
HARDWARE = ROOT / "scripts" / "register_windows_hardware_evidence_runner.ps1"
RUNTIME_ACL = ROOT / "scripts" / "grant_windows_runtime_service_access.ps1"


def read(path: Path) -> str:
    assert path.is_file(), f"missing bootstrap file: {path}"
    return path.read_text(encoding="utf-8")


def test_shared_bootstrap_pins_private_repo_and_distinct_roles() -> None:
    text = read(SHARED)
    assert "[ValidateSet('runtime', 'hardware')]" in text
    assert "https://github.com/mailsvb2-bot/Dokkomplekt_Hardware_Validation" in text
    assert "https://github.com/mailsvb2-bot/Dokkomplekt_Universal" in text
    assert "Production runners may register only to" in text
    assert "dokkomplekt-runtime" in text
    assert "dokkomplekt-hardware" in text
    assert "C:\\actions-runner-runtime" in text
    assert "C:\\actions-runner-hardware" in text
    assert "New-ScheduledTaskTrigger -AtLogOn" in text
    assert "--runasservice" in text
    assert "asset.digest" in text
    assert "Get-FileHash" in text
    assert "Runner package SHA-256 mismatch" in text


def test_runtime_role_requires_locked_approved_manifest_and_bounded_acl() -> None:
    text = read(SHARED)
    runtime = text[text.index("function Assert-RuntimeHost"):text.index("function Assert-HardwareHost")]
    assert "SidecarManifestPath is required for Role=runtime" in runtime
    assert "supply_chain_locked" in runtime
    assert "item.FullName + '.sig'" in runtime
    assert "C:\\ProgramData\\DokkomplektRuntime" in text
    assert "RUNTIME_SERVICE_ACL.json" in text
    assert "dokkomplekt.runtime-service-acl.v2" in runtime
    assert "S-1-5-20" in text
    assert "Visual Studio C++ Build Tools" in runtime
    assert "Ensure-OpenSslFromGit" in runtime
    assert "Word.Application" not in runtime
    assert "Get-Printer" not in runtime


def test_runtime_runs_as_service_while_hardware_remains_interactive() -> None:
    text = read(SHARED)
    configure = text[text.index("Push-Location $RunnerRoot"):]
    assert "if ($Role -eq 'runtime')" in configure
    assert "--runasservice" in configure
    assert "Register-InteractiveTask -UserName $interactive.user" in configure
    assert "windows-service-network-service" in configure
    assert "interactive-at-logon" in configure
    assert "Role -eq 'hardware'" in text
    assert "Word/printer validation must remain interactive" in text


def test_runtime_acl_helper_is_fail_closed_and_fixed_root() -> None:
    acl = read(RUNTIME_ACL)
    assert "$ExpectedRuntimeRoot = 'C:\\ProgramData\\DokkomplektRuntime'" in acl
    assert "Production RuntimeRoot is fixed to" in acl
    assert "Assert-UnderRoot" in acl
    assert "escapes the bounded runtime root" in acl
    assert "S-1-5-20" in acl
    assert "icacls.exe" in acl
    assert "(OI)(CI)(RX)" in acl
    assert "SecurityIdentifier" in acl
    assert "dokkomplekt.runtime-service-acl.v2" in acl


def test_hardware_role_requires_word_printer_and_rejects_runtime_or_signing_material() -> None:
    text = read(SHARED)
    hardware = text[text.index("function Assert-HardwareHost"):text.index("function Get-RunnerAsset")]
    assert "Role=hardware must not accept a runtime manifest" in hardware
    assert "Get-Printer" in hardware
    assert "Word.Application" in hardware
    assert "EdgeWebView" in hardware
    for forbidden in (
        "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64",
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64",
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
        "DOKKOMPLEKT_GATE_PRIVATE_KEY_B64",
    ):
        assert forbidden in hardware


def test_secure_entrypoints_do_not_cross_role_parameters() -> None:
    runtime = read(RUNTIME)
    hardware = read(HARDWARE)
    for text in (runtime, hardware):
        assert "Read-Host 'GitHub self-hosted runner registration token' -AsSecureString" in text
        assert "[Net.NetworkCredential]::new('', $secureToken)" in text
        assert "$plainToken = $null" in text
        assert "$secureToken.Dispose()" in text
    assert "-Role runtime" in runtime
    assert "SidecarManifestPath" in runtime
    assert "RuntimeRoot" in runtime
    assert "grant_windows_runtime_service_access.ps1" in runtime
    assert runtime.index("grant_windows_runtime_service_access.ps1") < runtime.index(
        "Read-Host 'GitHub self-hosted runner registration token' -AsSecureString"
    )
    assert "PrinterName" not in runtime
    assert "-Role hardware" in hardware
    assert "PrinterName" in hardware
    assert "SidecarManifestPath" not in hardware
