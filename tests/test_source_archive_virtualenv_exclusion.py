from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "build_source_archive.py"
SPEC = importlib.util.spec_from_file_location("build_source_archive_virtualenv_test", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_standard_in_tree_virtualenv_names_are_excluded_before_symlink_validation(tmp_path: Path) -> None:
    original_root = MODULE.ROOT
    MODULE.ROOT = tmp_path
    try:
        target = tmp_path / "outside-lib"
        target.mkdir()
        for name in (".venv", "venv", "env"):
            environment = tmp_path / name
            environment.mkdir()
            (environment / "lib64").symlink_to(target, target_is_directory=True)
        assert MODULE.source_files() == []
    finally:
        MODULE.ROOT = original_root
