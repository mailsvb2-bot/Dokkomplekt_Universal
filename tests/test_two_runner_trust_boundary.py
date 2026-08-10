from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
DOC = ROOT / "docs" / "TWO_RUNNER_TRUST_BOUNDARY.md"


def read(path: Path) -> str:
    assert path.is_file(), f"missing trust-boundary file: {path}"
    return path.read_text(encoding="utf-8")


def _job_block(text: str, start: str, end: str | None = None) -> str:
    start_index = text.index(start)
    if end is None:
        return text[start_index:]
    end_index = text.index(end, start_index)
    return text[start_index:end_index]


def test_private_workflow_separates_runtime_and_hardware_trust_domains() -> None:
    text = read(WORKFLOW)
    runtime = _job_block(text, "  signed-runtime-build:", "  hardware-evidence:")
    hardware = _job_block(text, "  hardware-evidence:")

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
    assert "windows_signed_handoff.py build" in runtime
    assert "windows_signed_handoff.py verify" in hardware
    assert "verify_windows_hardware_evidence_host.ps1" in hardware


def test_runtime_runner_builds_only_verified_offline_application_parity_installer() -> None:
    text = read(WORKFLOW)
    runtime = _job_block(text, "  signed-runtime-build:", "  hardware-evidence:")

    parity = "verify_windows_runtime_app_parity.py --target windows-x86_64"
    bundle = "npx tauri bundle --bundles nsis --config src-tauri/tauri.offline.conf.json"
    installer_contract = (
        "windows_installer_contract.ps1 -TauriConfig src-tauri/tauri.offline.conf.json "
        "-ExpectedWebViewMode offlineInstaller"
    )
    handoff = "windows_signed_handoff.py build"

    for marker in (parity, bundle, installer_contract, handoff):
        assert marker in runtime
    assert "npx tauri bundle --bundles nsis\n" not in runtime
    assert runtime.index(parity) < runtime.index(bundle) < runtime.index(installer_contract)
    assert runtime.index(installer_contract) < runtime.index(handoff)


def test_hardware_verifies_handoff_before_word_printer_reboot_execution() -> None:
    text = read(WORKFLOW)
    hardware = _job_block(text, "  hardware-evidence:")
    verify_index = hardware.index("windows_signed_handoff.py verify")
    authenticode_index = hardware.index("Get-AuthenticodeSignature")
    e2e_index = hardware.index("tests/windows/windows_hardware_e2e.ps1")
    assert verify_index < authenticode_index < e2e_index


def test_documented_boundary_matches_workflow_invariant() -> None:
    text = read(DOC)
    for marker in (
        "dokkomplekt-runtime",
        "windows-production-signing",
        "dokkomplekt-hardware",
        "windows-hardware-validation",
        "SIGNED_HANDOFF.json",
        "no production signing/private-key secrets",
    ):
        assert marker in text
