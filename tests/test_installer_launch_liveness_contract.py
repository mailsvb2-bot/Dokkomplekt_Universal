from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WINDOWS_CONTRACT = ROOT / "tests" / "installer" / "windows_installer_contract.ps1"
LINUX_CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"
LINUX_CONTRACT_CORE = (
    ROOT / "tests" / "installer" / "linux_installer_contract_core.sh"
)
TAURI_CONFIG = ROOT / "src-tauri" / "tauri.conf.json"
TAURI_PROCESS_BLUEPRINTS = (
    ROOT / "src-tauri" / "src" / "subsystems" / "process_blueprints.rs"
)
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
    assert "Dokkomplekt native frontend IPC ready" in source
    assert "did not prove native frontend IPC readiness" in source
    assert 'kill -TERM "$pid"' in source
    assert 'kill -TERM -- "-$pgid"' in source
    assert 'DOKKOMPLEKT_LINUX_SMOKE_WATCHDOG_SECONDS' in source
    assert 'exec timeout --signal=TERM --kill-after=10s "${watchdog_seconds}s"' in source


def test_linux_x11_probe_inherits_the_private_xvfb_authority_cookie() -> None:
    source = _linux_contract_source()

    assert 'xauthority_file="$smoke_home/xauthority-path"' in source
    assert r'''printf '%s\n' "${XAUTHORITY:-}" >"$xauthority_file"''' in source
    assert (
        '"$wrapper" "$display_file" "$xauthority_file" "$mode" "$executable"'
        in source
    )
    assert 'if [ -s "$display_file" ] && [ -s "$xauthority_file" ]; then' in source
    assert 'xauthority="$(cat "$xauthority_file")"' in source
    assert 'if [ ! -r "$xauthority" ]; then' in source
    assert 'XAUTHORITY="$xauthority" timeout --signal=KILL' in source
    assert '2>"$probe_error_file"' in source
    probe_call = source.index('python scripts/verify_rendered_x11_window.py')
    assert source.rfind('XAUTHORITY="$xauthority"', 0, probe_call) != -1


def test_linux_smoke_forces_webkit_onto_x11_software_rendering_under_xvfb() -> None:
    entrypoint = LINUX_CONTRACT.read_text(encoding="utf-8")

    assert "Xvfb runner has no usable DRI3/GBM device" in entrypoint
    assert "export GDK_BACKEND=x11" in entrypoint
    assert "export LIBGL_ALWAYS_SOFTWARE=1" in entrypoint
    assert "export WEBKIT_DISABLE_DMABUF_RENDERER=1" in entrypoint
    assert "export WEBKIT_DISABLE_COMPOSITING_MODE=1" in entrypoint
    assert 'exec bash "$script_dir/linux_installer_contract_core.sh" "$@"' in entrypoint


def test_linux_ready_title_requires_successful_frontend_tauri_ipc() -> None:
    config = TAURI_CONFIG.read_text(encoding="utf-8")
    native = TAURI_PROCESS_BLUEPRINTS.read_text(encoding="utf-8")
    frontend = FRONTEND_ENTRY.read_text(encoding="utf-8")
    capability = TAURI_CAPABILITY.read_text(encoding="utf-8")

    assert '"title": "Dokkomplekt Universal"' in config
    assert 'fn get_process_blueprints(' in native
    assert 'app: tauri::AppHandle' in native
    assert 'app.get_webview_window("main")' in native
    assert 'window.set_title("Dokkomplekt Universal")' in native
    assert 'Dokkomplekt native frontend IPC ready' in native
    assert "grep -Fq 'Dokkomplekt native frontend IPC ready'" in _linux_contract_source()
    assert 'Dokkomplekt native frontend IPC signal failed:' in native
    assert "import { invoke } from '@tauri-apps/api/core';" in frontend
    assert "invoke('get_process_blueprints')" in frontend
    assert 'Failed to confirm rendered frontend IPC' in frontend
    assert '"title": "Доккомплект — запуск"' not in config
    assert "const READY_WINDOW_TITLE = 'Dokkomplekt Universal';" in frontend
    assert "const RENDER_PROBE_INTERVAL_MS = 50;" in frontend
    assert "const REQUIRED_STABLE_READY_CHECKS = 2;" in frontend
    assert "stableReadyChecks = hasRenderedContent ? stableReadyChecks + 1 : 0" in frontend
    assert "stableReadyChecks >= REQUIRED_STABLE_READY_CHECKS" in frontend
    assert "window.setTimeout(probe, RENDER_PROBE_INTERVAL_MS)" in frontend
    assert "window.setTimeout(probe, 0)" in frontend
    assert "window.requestAnimationFrame(" not in frontend
    assert "root.childElementCount > 0" in frontend
    assert "root.textContent?.trim().length" in frontend
    assert "root.getBoundingClientRect()" not in frontend
    assert "window.innerWidth" not in frontend
    assert "window.innerHeight" not in frontend
    assert "__TAURI_INTERNALS__" not in frontend
    assert "document.title = READY_WINDOW_TITLE" in frontend
    assert "getCurrentWindow()" in frontend
    assert ".setTitle(READY_WINDOW_TITLE)" in frontend
    assert "Failed to signal rendered native window" in frontend
    assert "Failed to access rendered native window" in frontend
    assert '"core:window:allow-set-title"' in capability
