from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src" / "main.tsx"
BOOTSTRAP = ROOT / "src" / "lib" / "outputRootBootstrap.ts"


def test_failed_output_root_bootstrap_is_visible_and_forces_recovery_setup() -> None:
    main = MAIN.read_text(encoding="utf-8")
    bootstrap = BOOTSTRAP.read_text(encoding="utf-8")

    assert "getOutputRootBootstrapError" in main
    assert "OUTPUT_NAMING_CONFIRMED_KEY" in main
    assert "localStorage.removeItem(OUTPUT_NAMING_CONFIRMED_KEY)" in main
    assert "Папка готовых документов не подготовлена" in main
    assert "lastBootstrapError" in bootstrap
    assert "Не удалось подготовить папку готовых документов" in bootstrap
