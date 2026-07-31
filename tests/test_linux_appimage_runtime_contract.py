from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
STAGER = ROOT / "scripts" / "stage_linux_appimage_runtime.mjs"
FINAL_MANIFEST_WRITER = ROOT / "scripts" / "write_linux_appimage_runtime_manifest.py"
REQUIRED = ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0")


def _write_elf(path: Path, machine: int) -> None:
    data = bytearray(64)
    data[:4] = b"\x7fELF"
    data[4] = 2  # ELFCLASS64
    data[5] = 1  # little-endian
    data[18:20] = machine.to_bytes(2, "little")
    path.write_bytes(data)


def _write_fake_ldconfig(path: Path, libraries: dict[str, Path]) -> None:
    lines = [f"{name} (libc6,x86-64) => {library}" for name, library in libraries.items()]
    path.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "[ \"${1:-}\" = \"-p\" ]\n"
        "cat <<'EOF'\n"
        + "\n".join(lines)
        + "\nEOF\n",
        encoding="utf-8",
    )
    path.chmod(0o755)


def _run_stager(tmp_path: Path, libraries: dict[str, Path]) -> subprocess.CompletedProcess[str]:
    node = shutil.which("node")
    if node is None:
        pytest.skip("node is required for the AppImage packaging contract")
    fake_ldconfig = tmp_path / "ldconfig"
    _write_fake_ldconfig(fake_ldconfig, libraries)
    destination = tmp_path / "staged"
    env = os.environ.copy()
    env.update(
        {
            "DOKKOMPLEKT_LDCONFIG": str(fake_ldconfig),
            "DOKKOMPLEKT_APPIMAGE_RUNTIME_DIR": str(destination),
            "TAURI_ENV_ARCH": "x86_64",
        }
    )
    return subprocess.run(
        [node, str(STAGER)],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
        timeout=20,
    )


def test_appimage_packaging_hook_is_scoped_to_bundle_phase() -> None:
    config = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text("utf-8"))
    assert config["build"]["beforeBundleCommand"] == (
        "node scripts/stage_linux_appimage_runtime.mjs"
    )
    assert (ROOT / "src-tauri/build.rs").read_text("utf-8") == (
        "fn main() {\n    tauri_build::build()\n}\n"
    )
    assert config["bundle"]["linux"]["appimage"]["files"] == {
        "/usr/lib/libGLESv2.so.2": "target/appimage-runtime/libGLESv2.so.2",
        "/usr/lib/libEGL.so.1": "target/appimage-runtime/libEGL.so.1",
        "/usr/lib/libGLdispatch.so.0": "target/appimage-runtime/libGLdispatch.so.0",
        "/usr/share/dokkomplekt/appimage-runtime.json": (
            "target/appimage-runtime/manifest.json"
        ),
    }
    workflow = (ROOT / ".github/workflows/quality-gate.yml").read_text("utf-8")
    assert "write_linux_appimage_runtime_manifest.py --bundle-dir" in workflow
    assert FINAL_MANIFEST_WRITER.is_file()


def test_stager_copies_matching_elfs_and_writes_source_provenance_manifest(
    tmp_path: Path,
) -> None:
    library_dir = tmp_path / "libraries"
    library_dir.mkdir()
    libraries = {}
    for name in REQUIRED:
        path = library_dir / name
        _write_elf(path, machine=62)
        libraries[name] = path

    result = _run_stager(tmp_path, libraries)
    assert result.returncode == 0, result.stdout

    destination = tmp_path / "staged"
    manifest = json.loads((destination / "manifest.json").read_text("utf-8"))
    assert manifest["schema"] == 2
    assert manifest["phase"] == "pre-linuxdeploy"
    assert manifest["targetArch"] == "x86_64"
    records = {record["name"]: record for record in manifest["libraries"]}
    assert set(records) == set(REQUIRED)
    for name in REQUIRED:
        data = (destination / name).read_bytes()
        assert data[:4] == b"\x7fELF"
        assert records[name] == {
            "name": name,
            "elfMachine": 62,
            "sourceSize": len(data),
            "sourceSha256": hashlib.sha256(data).hexdigest(),
        }


def test_stager_fails_closed_for_missing_or_wrong_architecture_library(
    tmp_path: Path,
) -> None:
    library_dir = tmp_path / "libraries"
    library_dir.mkdir()
    libraries = {}
    for name in REQUIRED:
        path = library_dir / name
        _write_elf(path, machine=183 if name == "libEGL.so.1" else 62)
        libraries[name] = path

    result = _run_stager(tmp_path, libraries)
    assert result.returncode != 0
    assert "libEGL.so.1" in result.stdout
    assert "missing or has the wrong architecture" in result.stdout


def test_installer_smoke_verifies_final_hashes_elf_architecture_and_gui_liveness() -> None:
    text = (ROOT / "tests/installer/linux_installer_contract.sh").read_text("utf-8")
    for required in (
        "appimage-runtime.json",
        ".AppImage.runtime-manifest.json",
        "pre-linuxdeploy",
        "post-linuxdeploy",
        "embeddedSourceManifestSha256",
        "elfMachine",
        "sha256",
        "APPIMAGE_EXTRACT_AND_RUN=1",
        "Dokkomplekt Universal",
        "rendered GUI smoke",
        "did not render a non-blank",
    ):
        assert required in text
