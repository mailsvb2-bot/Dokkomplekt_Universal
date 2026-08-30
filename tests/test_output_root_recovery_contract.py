from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src" / "main.tsx"
TAURI_MAIN = ROOT / "src-tauri" / "src" / "main.rs"
OUTPUT_COMMANDS = ROOT / "src-tauri" / "src" / "subsystems" / "output_root_commands.rs"
OUTPUT_HOOK = ROOT / "src" / "hooks" / "useOutputDestination.ts"


def test_output_root_is_native_startup_invariant_without_blocking_first_react_paint() -> None:
    frontend = MAIN.read_text(encoding="utf-8")
    native = TAURI_MAIN.read_text(encoding="utf-8")
    commands = OUTPUT_COMMANDS.read_text(encoding="utf-8")
    hook = OUTPUT_HOOK.read_text(encoding="utf-8")

    assert "fn ensure_startup_output_root" in commands
    assert "ensure_output_root_path(&path)?" in commands
    assert "persist_output_preferences(app, &preferences)?" in commands
    ensure_index = native.index("ensure_startup_output_root(&handle)")
    window_index = native.index("tauri::WebviewWindowBuilder::from_config")
    assert ensure_index < window_index

    assert "await ensureDefaultOutputRoot" not in frontend
    assert "Native Rust startup owns durable output-root preparation" in frontend
    assert "ReactDOM.createRoot(root).render" in frontend

    assert "cachedRoot.trim() && cachedConfirmed" in hook
    assert "!stored.output_root.trim() || !stored.naming_confirmed" in hook
    assert "a previously confirmed user choice must win" in hook
