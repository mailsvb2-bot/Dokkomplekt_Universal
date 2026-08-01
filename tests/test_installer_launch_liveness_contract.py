from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
LINUX_CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"
LINUX_CONTRACT_CORE = (
    ROOT / "tests" / "installer" / "linux_installer_contract_core.sh"
)
TAURI_CONFIG = ROOT / "src-tauri" / "tauri.conf.json"
FRONTEND_ENTRY = ROOT / "src" / "main.tsx"
TAURI_CAPABILITY = ROOT / "src-tauri" / "capabilities" / "default.json"


def _linux_contract_source() -> str:
    return "\n".join(
        (
            LINUX_CONTRACT.read_text(encoding="utf-8"),
            LINUX_CONTRACT_CORE.read_text(encoding="utf-8"),
        )
    )


def test_windows_smoke_rejects_every_early_exit_including_zero() -> None:
    source = WINDOWS_CONTRACT.read_text(encoding="utf-8")

    assert "if ($process.HasExited)" in source
    assert "exited early during launch smoke" in source
    assert "$process.HasExited -and $process.ExitCode -ne 0" not in source
    assert "Stop-Process -Id $process.Id -Force" in source


def test_linux_smoke_requires_a_rendered_named_window_not_only_a_live_process() -> None:
    source = _linux_contract_source()

    assert 'if ! process_is_running "$pid"; then' in source
    assert 'if ! kill -0 -- "-$pid"' not in source
    assert "exited before rendering its window" in source
    assert "verify_rendered_x11_window.py" in source
    assert "DOKKOMPLEKT_X11_PROBE_TIMEOUT_SECONDS" in source
    assert 'timeout --signal=KILL "${probe_timeout_seconds}s"' in source
    assert 'for command in xvfb-run dbus-run-session setsid timeout python ps tr' in source
    assert '--title "Dokkomplekt Universal"' in source
    assert "--exact-title" not in source
    assert "--min-width 800" in source
    assert "--min-height 500" in source
    assert "--min-colors 64" in source
    assert "--min-colors 1" in source
    assert "--accept-title-handshake" not in source
    assert "did not emit a rendered Dokkomplekt Universal ready window" in source
    assert 'kill -TERM "$pid"' in source
    assert 'kill -TERM -- "-$pgid"' in source
    assert 'DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_SECONDS' in source
    assert 'exec timeout --signal=TERM --kill-after=10s "${watchdog_seconds}s"' in source


def test_linux_smoke_forces_webkit_onto_x11_software_rendering_under_xvfb() -> None:
    entrypoint = LINUX_CONTRACT.read_text(encoding="utf-8")

    assert "Xvfb runner has no usable DRI3/GBM device" in entrypoint
    assert "export GDK_BACKEND=x11" in entrypoint
    assert "export LIBGL_ALWAYS_SOFTWARE=1" in entrypoint
    assert "export WEBKIT_DISABLE_DMABUF_RENDERER=1" in entrypoint
    assert "export WEBKIT_DISABLE_COMPOSITING_MODE=1" in entrypoint
    assert 'exec bash "$script_dir/linux_installer_contract_core.sh" "$@"' in entrypoint


def test_linux_ready_title_is_emitted_only_after_visible_react_layout() -> None:
    config = TAURI_CONFIG.read_text(encoding="utf-8")
    frontend = FRONTEND_ENTRY.read_text(encoding="utf-8")
    capability = TAURI_CAPABILITY.read_text(encoding="utf-8")

    assert '"title": "Доккомплект — запуск"' in config
    assert '"title": "Dokkomplekt Universal — запуск"' not in config
    assert "const READY_WINDOW_TITLE = 'Dokkomplekt Universal';" in frontend
    assert "window.requestAnimationFrame(() => window.requestAnimationFrame(probe))" in frontend
    assert "root.childElementCount > 0" in frontend
    assert "root.textContent?.trim().length" in frontend
    assert "root.getBoundingClientRect()" in frontend
    assert "rectangle.width >= MIN_RENDER_WIDTH" in frontend
    assert "rectangle.height >= MIN_RENDER_HEIGHT" in frontend
    assert "getCurrentWindow()" in frontend
    assert ".setTitle(READY_WINDOW_TITLE)" in frontend
    assert "Failed to signal rendered native window" in frontend
    assert '"core:window:allow-set-title"' in capability
