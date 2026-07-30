from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_appimage_explicitly_bundles_dlopen_graphics_runtime() -> None:
    config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text("utf-8"))
    assert config["bundle"]["linux"]["appimage"]["files"] == {
        "/usr/lib/libGLESv2.so.2": "linux-runtime/libGLESv2.so.2",
        "/usr/lib/libEGL.so.1": "linux-runtime/libEGL.so.1",
        "/usr/lib/libGLdispatch.so.0": "linux-runtime/libGLdispatch.so.0",
    }


def test_build_script_stages_dlopen_only_graphics_libraries_fail_closed() -> None:
    text = (ROOT / "src-tauri/build.rs").read_text("utf-8")
    for name in ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0"):
        assert name in text
    assert 'Command::new("ldconfig")' in text
    assert "required AppImage graphics libraries are missing" in text
    assert 'starts_with(b"\\x7fELF")' in text


def test_installer_smoke_rejects_host_dependent_appimage() -> None:
    text = (ROOT / "tests/installer/linux_installer_contract.sh").read_text("utf-8")
    for name in ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0"):
        assert name in text
