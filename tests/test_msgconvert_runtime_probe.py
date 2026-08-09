from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "probe_offline_runtime.py"


def load_module():
    spec = importlib.util.spec_from_file_location("probe_offline_runtime", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def status_for(paths: list[str]) -> dict:
    return {
        "target": "windows-x86_64",
        "files": [{"tool": "msgconvert", "path": path} for path in paths],
    }


def create_files(root: Path, paths: list[str]) -> None:
    for relative in paths:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fixture")


def test_native_msgconvert_wrapper_is_preferred() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        paths = ["msgconvert/msgconvert.exe", "msgconvert/perl.exe", "msgconvert/msgconvert.pl"]
        create_files(root, paths)
        command = module.msgconvert_probe_command(root, status_for(paths))
        assert Path(command[0]).name.lower() == "msgconvert.exe"
        assert command[1:] == ["--help"]


def test_perl_msgconvert_requires_locked_local_perl_runtime() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        paths = ["msgconvert/msgconvert.pl"]
        create_files(root, paths)
        with pytest.raises(FileNotFoundError, match="perl"):
            module.msgconvert_probe_command(root, status_for(paths))


def test_perl_msgconvert_uses_perl_from_same_locked_component() -> None:
    module = load_module()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        paths = ["msgconvert/bin/perl.exe", "msgconvert/msgconvert.pl"]
        create_files(root, paths)
        command = module.msgconvert_probe_command(root, status_for(paths))
        assert Path(command[0]).name.lower() == "perl.exe"
        assert Path(command[1]).name.lower() == "msgconvert.pl"
        assert command[2:] == ["--help"]
