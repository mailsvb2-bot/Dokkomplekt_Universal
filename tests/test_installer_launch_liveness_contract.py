from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
LINUX_CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"


def test_windows_smoke_rejects_every_early_exit_including_zero() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "if ($process.HasExited)" in source
    assert "exited early during launch smoke" in source
    assert "$process.HasExited -and $process.ExitCode -ne 0" not in source
    assert "Stop-Process -Id $process.Id -Force" in source


def test_linux_smoke_requires_the_isolated_process_group_to_remain_alive() -> None:
    source = LINUX_CONTRACT.read_text(encoding="utf-8")

    assert 'if ! kill -0 -- "-$pid"' in source
    assert "exited early during launch smoke" in source
    assert 'if [ "$status" -ne 0 ]' not in source
    assert 'kill -TERM -- "-$pid"' in source
