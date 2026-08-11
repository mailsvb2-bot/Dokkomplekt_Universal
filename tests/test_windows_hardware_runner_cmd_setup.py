from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CMD = ROOT / "SETUP_HARDWARE_RUNNER.cmd"
SETUP = ROOT / "scripts" / "setup_windows_hardware_runner_from_cmd.ps1"
HOST_PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"


def read(path: Path) -> str:
    assert path.is_file(), f"missing hardware setup artifact: {path}"
    return path.read_text(encoding="utf-8")


def test_cmd_is_single_user_facing_entrypoint_and_targets_private_repo() -> None:
    cmd = read(CMD)
    assert "DokkomplektHardwareRunnerSetup" in cmd
    assert "setup_windows_hardware_runner_from_cmd.ps1" in cmd
    assert "register_windows_hardware_evidence_runner.ps1" in cmd
    assert "bootstrap_private_windows_runner.ps1" in cmd
    assert "raw.githubusercontent.com/mailsvb2-bot/Dokkomplekt_Universal/main/scripts" in cmd
    assert "dokkomplekt-hardware" in cmd
    assert "net session" in cmd
    assert "-Verb RunAs" in cmd


def test_setup_installs_missing_prerequisites_without_requiring_winget() -> None:
    text = read(SETUP)
    assert "git-for-windows/git" in text
    assert "PowerShell/PowerShell" in text
    assert "asset.digest" in text
    assert "Get-FileHash" in text
    assert "https://go.microsoft.com/fwlink/p/?LinkId=2124703" in text
    assert "https://aka.ms/vs/17/release/vs_BuildTools.exe" in text
    assert "Assert-MicrosoftSignedFile" in text
    assert "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" in text
    assert '"$rustupUrl.sha256"' in text
    assert "rustup-init SHA-256 mismatch" in text
    assert "1.97.1" in text
    assert "winget" not in text.lower()


def test_setup_auto_configures_real_printer_reboot_source_and_interactive_runner() -> None:
    text = read(SETUP)
    for required in (
        "Get-Printer",
        "Get-PrinterPort",
        "Get-CimInstance Win32_Printer",
        "Word.Application",
        "OpenFileDialog",
        "hardware-runner.json",
        "WINDOWS_REBOOT_E2E_RAW.json",
        "register_windows_hardware_evidence_runner.ps1",
        "Dokkomplekt_Hardware_Validation",
        "Get-ScheduledTask",
        "Runner.Listener",
    ):
        assert required in text
    assert "Microsoft Print to PDF" in text
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64" not in text
    assert "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64" not in text


def test_hardware_preflight_exports_local_non_secret_config_to_following_steps() -> None:
    text = read(HOST_PREFLIGHT)
    for required in (
        "dokkomplekt.hardware-runner-local-config.v1",
        "hardware-runner.json",
        "DOKKOMPLEKT_TEST_PRINTER",
        "DOKKOMPLEKT_TEST_DUPLEX",
        "DOKKOMPLEKT_TEST_TRAY",
        "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH",
        "DOKKOMPLEKT_REBOOT_SOURCE_DOCUMENT",
        "GITHUB_ENV",
        "local-hardware-config-loaded",
    ):
        assert required in text
    # Loading config before handoff verification must stay side-effect-free.
    assert "New-Object -ComObject Word.Application" not in text
    assert "Get-Printer -Name" not in text
    assert "wevtutil sl Microsoft-Windows-PrintService/Operational" not in text
