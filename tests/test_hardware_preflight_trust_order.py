from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HOST_PREFLIGHT = ROOT / "scripts" / "verify_windows_hardware_evidence_host.ps1"
HARDWARE_E2E = ROOT / "tests" / "windows" / "windows_hardware_e2e.ps1"


def test_early_hardware_host_preflight_does_not_touch_word_or_printer():
    preflight = HOST_PREFLIGHT.read_text(encoding="utf-8")

    forbidden_before_handoff_verification = (
        "New-Object -ComObject Word.Application",
        "Get-Printer -Name",
        "Get-PrinterPort -Name",
        "wevtutil sl Microsoft-Windows-PrintService/Operational",
    )
    for command in forbidden_before_handoff_verification:
        assert command not in preflight

    assert "hardware-probes-deferred-until-signed-handoff" in preflight
    assert "hardware_probes_deferred_until_signed_handoff = $true" in preflight


def test_real_word_printer_and_printservice_probes_remain_in_hardware_e2e():
    hardware = HARDWARE_E2E.read_text(encoding="utf-8")

    assert "Get-Printer -Name $env:DOKKOMPLEKT_TEST_PRINTER" in hardware
    assert "wevtutil sl Microsoft-Windows-PrintService/Operational /e:true" in hardware
    assert "cargo test -p dokkomplekt-tauri windows_word_print_hardware_e2e" in hardware
