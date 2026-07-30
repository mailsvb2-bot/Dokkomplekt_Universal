from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def test_release_identity_is_consistent() -> None:
    version = text("VERSION").strip()
    assert version == "18.4.3"
    assert json.loads(text("package.json"))["version"] == version
    assert json.loads(text("src-tauri/tauri.conf.json"))["version"] == version
    assert f'version = "{version}"' in text("src-tauri/Cargo.toml")
    assert f'version = "{version}"' in text("crates/dokkomplekt-license-server/Cargo.toml")
    assert f'version = "{version}"' in text("crates/dokkomplekt-license-python/pyproject.toml")


def test_click_launcher_has_real_execution_path_and_diagnostics() -> None:
    launcher = text("main.bat")
    assert ":refresh_path" in launcher
    assert ":check_tools" in launcher
    assert "npm run tauri:dev" in launcher
    assert "target\\release\\dokkomplekt-tauri.exe" in launcher
    assert "launcher_logs\\last_launch.log" in launcher


def test_active_python_gates_run_the_complete_pytest_contour() -> None:
    for relative in (
        ".github/workflows/quality-gate.yml",
        ".github/workflows/build-installers.yml",
        "CHECK_PROJECT.bat",
        "BUILD_WINDOWS_INSTALLER.bat",
    ):
        content = text(relative)
        assert "run_python_contracts_sharded.py" in content
        assert "unittest discover" not in content
    assert "pytest==" in text("requirements-dev.txt")


def test_current_rust_toolchain_is_pinned_everywhere_active() -> None:
    assert 'channel = "1.97.1"' in text("rust-toolchain.toml")
    assert 'rust-version = "1.97.1"' in text("Cargo.toml")
    assert 'REQUIRED_RUST="1.97.1"' in text("main.sh")


def test_commercial_rust_crates_are_no_longer_ignored_by_release_gate() -> None:
    checker = text("scripts/check_commercial_rust_crates.py")
    assert "dokkomplekt-license-server" in checker
    assert "dokkomplekt-license-python" in checker
    assert '"cargo", "clippy"' in checker
    assert '"cargo", "test"' in checker
    assert '"cargo", "audit"' in checker
    assert "check_commercial_rust_crates.py" in text("scripts/prepackage_rust_gate.sh")
    assert "COMMERCIAL_CRATES_EVIDENCE.json" in text("scripts/assert_release_ready.py")


def test_diaries_require_and_render_both_real_signature_lines() -> None:
    required = text("crates/dokkomplekt-core/src/required_blocks.rs")
    renderer = text("crates/dokkomplekt-core/src/document_generation.rs")
    assert "treating_physician_signature" in required
    assert "department_head_signature" in required
    assert "contains_signature_line" in required
    assert "Лечащий врач __________________" in renderer
    assert "Заведующий отделением __________" in renderer


def test_generated_document_identity_is_not_silently_inherited() -> None:
    popup = text("crates/dokkomplekt-core/src/popup_profiles.rs")
    assert "should_ask_fresh_each_run" in popup
    assert '"document.number" | "document.date" => true' in popup
    assert "PromptAskMode::Always" in popup


def test_trivial_repeated_digit_inn_is_rejected() -> None:
    validators = text("crates/dokkomplekt-core/src/validators.rs")
    assert "numbers.iter().all(|value| value == first)" in validators
    assert 'validate_field_value("org.inn", "0000000000")' in validators
    assert 'validate_field_value("org.inn", "111111111111")' in validators


def test_accounting_popup_ids_and_input_kinds_are_canonicalized_before_merge() -> None:
    aliases = text("crates/dokkomplekt-core/src/field_aliases.rs")
    profiles = text("crates/dokkomplekt-core/src/popup_profiles.rs")
    domain_profiles = text("crates/dokkomplekt-core/src/domain_profiles.rs")

    assert '"accounting.client" => "counterparty.name".into()' in aliases
    assert '"accounting.currency" => "amount.currency".into()' in aliases
    assert '.map(|field| canonical_storage_field_id(field))' in profiles
    assert 'id.contains("count")' not in profiles
    assert 'let leaf = id.rsplit' in profiles
    assert 'add("counterparty.name", true)' in profiles
    assert 'add("amount.currency", false)' in profiles
    assert '"accounting.client",\n                    "accounting.amount_total"' not in domain_profiles
