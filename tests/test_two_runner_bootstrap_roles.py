from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHARED = ROOT / "scripts" / "bootstrap_private_windows_runner.ps1"
RUNTIME = ROOT / "scripts" / "register_windows_runtime_runner.ps1"
HARDWARE = ROOT / "scripts" / "register_windows_hardware_evidence_runner.ps1"


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
    assert "asset.digest" in text
    assert "Get-FileHash" in text
    assert "Runner package SHA-256 mismatch" in text


def test_runtime_role_requires_locked_approved_manifest_but_not_word_or_printer() -> None:
    text = read(SHARED)
    runtime = text[text.index("function Assert-RuntimeHost"):text.index("function Assert-HardwareHost")]
    assert "SidecarManifestPath is required for Role=runtime" in runtime
    assert "supply_chain_locked" in runtime
    assert "item.FullName + '.sig'" in runtime
    assert "Visual Studio C++ Build Tools" in runtime
    assert "Ensure-OpenSslFromGit" in runtime
    assert "Word.Application" not in runtime
    assert "Get-Printer" not in runtime


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
    assert "PrinterName" not in runtime
    assert "-Role hardware" in hardware
    assert "PrinterName" in hardware
    assert "SidecarManifestPath" not in hardware
