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


def test_linux_smoke_requires_a_rendered_named_window_not_only_a_live_process() -> None:
    source = LINUX_CONTRACT.read_text(encoding="utf-8")

    assert 'if ! kill -0 -- "-$pid"' in source
    assert "exited before rendering its window" in source
    assert "verify_rendered_x11_window.py" in source
    assert '--title "Dokkomplekt Universal"' in source
    assert "--min-width 800" in source
    assert "--min-height 500" in source
    assert "--min-colors 64" in source
    assert "did not render a non-blank Dokkomplekt Universal window" in source
    assert 'kill -TERM -- "-$pid"' in source
