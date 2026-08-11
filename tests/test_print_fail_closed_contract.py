from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_watcher_auto_print_fails_closed_on_control_and_preferences_errors() -> None:
    watcher = (ROOT / "src-tauri/src/subsystems/watcher_commands.rs").read_text("utf-8")
    assert "watcher_control_unavailable" in watcher
    assert "automatic_print_blocked_control_error" in watcher
    assert "let effective_auto_print = if control_error.is_some()" in watcher
    assert "load_print_preferences(&app).unwrap_or_default()" not in watcher
    assert "print_preferences_unavailable" in watcher
    assert "automatic_print_blocked_preferences_error" in watcher
    prefs = watcher.index("let print_preferences = match load_print_preferences(&app)")
    print_call = watcher.index("let print_result = print_resolved_jobs(&jobs, &print_preferences)")
    blocked = watcher.index("return;", prefs, print_call)
    assert prefs < blocked < print_call


def test_printer_discovery_failure_is_not_an_empty_inventory() -> None:
    rust = (ROOT / "src-tauri/src/subsystems/desktop_io.rs").read_text("utf-8")
    types = (ROOT / "src/lib/types.ts").read_text("utf-8")
    validation = (ROOT / "src/lib/runtimeValidation.ts").read_text("utf-8")
    ui = (ROOT / "src/components/AutomationControlCenter.tsx").read_text("utf-8")
    assert "discovery_error: Option<String>" in rust
    assert "fn build_printer_inventory" in rust
    assert "discover_printers().unwrap_or_default()" not in rust
    assert "discovery_error?: string | null" in types
    assert "root.discovery_error" in validation
    assert "Не удалось получить список принтеров" in ui
