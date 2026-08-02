#!/usr/bin/env python3
"""Record exact post-linuxdeploy AppImage runtime bytes.

The libraries staged before Tauri bundling are intentionally patched by
linuxdeploy/patchelf. Their pre-bundle hashes therefore describe provenance,
not the final bytes. This script extracts the completed AppImage and writes a
sidecar manifest for the exact packaged artifact and graphics runtime.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import Any

REQUIRED_LIBRARIES = (
    "libGLESv2.so.2",
    "libEGL.so.1",
    "libGLdispatch.so.0",
)
ARCHITECTURES = {
    "x86_64": ("x86_64", 62),
    "amd64": ("x86_64", 62),
    "aarch64": ("aarch64", 183),
    "arm64": ("aarch64", 183),
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_elf_machine(path: Path) -> int:
    with path.open("rb") as handle:
        header = handle.read(20)
    if len(header) < 20 or header[:4] != b"\x7fELF":
        raise ValueError(f"{path} is not an ELF binary")
    if header[4] != 2:
        raise ValueError(f"{path} is not a 64-bit ELF binary")
    byteorder = "little" if header[5] == 1 else "big" if header[5] == 2 else None
    if byteorder is None:
        raise ValueError(f"{path} has an unsupported ELF byte order")
    return int.from_bytes(header[18:20], byteorder)


def target_architecture() -> tuple[str, int]:
    machine = platform.machine().lower()
    try:
        return ARCHITECTURES[machine]
    except KeyError as error:
        raise RuntimeError(f"unsupported Linux AppImage architecture: {machine}") from error


def load_source_manifest(path: Path, arch_name: str) -> tuple[dict[str, Any], bytes]:
    raw = path.read_bytes()
    manifest = json.loads(raw.decode("utf-8"))
    if manifest.get("schema") != 2 or manifest.get("phase") != "pre-linuxdeploy":
        raise RuntimeError("unsupported embedded AppImage runtime source manifest")
    if manifest.get("targetArch") != arch_name:
        raise RuntimeError(
            "embedded AppImage runtime source manifest architecture mismatch: "
            f"{manifest.get('targetArch')} != {arch_name}"
        )
    records = manifest.get("libraries")
    if not isinstance(records, list):
        raise RuntimeError("embedded AppImage runtime source manifest has no libraries")
    return manifest, raw


def find_appimages(bundle_dir: Path) -> list[Path]:
    return sorted(path for path in bundle_dir.rglob("*.AppImage") if path.is_file())


def write_manifest(appimage: Path) -> Path:
    arch_name, expected_machine = target_architecture()
    appimage.chmod(appimage.stat().st_mode | stat.S_IXUSR)

    with tempfile.TemporaryDirectory(prefix="dokkomplekt-appimage-manifest-") as temporary:
        temporary_path = Path(temporary)
        environment = os.environ.copy()
        environment["APPIMAGE_EXTRACT_AND_RUN"] = "1"
        subprocess.run(
            [str(appimage.resolve()), "--appimage-extract"],
            cwd=temporary_path,
            env=environment,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        root = temporary_path / "squashfs-root"
        source_manifest_path = root / "usr/share/dokkomplekt/appimage-runtime.json"
        if not source_manifest_path.is_file():
            raise RuntimeError("completed AppImage has no embedded runtime source manifest")
        source_manifest, source_manifest_raw = load_source_manifest(
            source_manifest_path, arch_name
        )
        source_records = {
            entry.get("name"): entry
            for entry in source_manifest["libraries"]
            if isinstance(entry, dict)
        }

        libraries: list[dict[str, Any]] = []
        for name in REQUIRED_LIBRARIES:
            path = root / "usr/lib" / name
            if not path.is_file() or path.stat().st_size == 0:
                raise RuntimeError(f"completed AppImage is missing runtime library: {name}")
            machine = read_elf_machine(path)
            if machine != expected_machine:
                raise RuntimeError(
                    f"completed AppImage runtime has wrong architecture: {name} ({machine})"
                )
            source = source_records.get(name)
            if not isinstance(source, dict):
                raise RuntimeError(f"source manifest does not describe runtime library: {name}")
            if source.get("elfMachine") != machine:
                raise RuntimeError(f"source manifest architecture mismatch: {name}")
            source_size = source.get("sourceSize")
            source_sha256 = source.get("sourceSha256")
            if not isinstance(source_size, int) or source_size <= 0:
                raise RuntimeError(f"source manifest has invalid size: {name}")
            if not isinstance(source_sha256, str) or len(source_sha256) != 64:
                raise RuntimeError(f"source manifest has invalid SHA-256: {name}")
            libraries.append(
                {
                    "name": name,
                    "elfMachine": machine,
                    "size": path.stat().st_size,
                    "sha256": sha256_file(path),
                    "sourceSize": source_size,
                    "sourceSha256": source_sha256,
                }
            )

    output = Path(f"{appimage}.runtime-manifest.json")
    payload = {
        "schema": 1,
        "phase": "post-linuxdeploy",
        "generatedBy": "scripts/write_linux_appimage_runtime_manifest.py",
        "targetArch": arch_name,
        "appImage": {
            "name": appimage.name,
            "size": appimage.stat().st_size,
            "sha256": sha256_file(appimage),
        },
        "embeddedSourceManifestSha256": hashlib.sha256(source_manifest_raw).hexdigest(),
        "libraries": libraries,
    }
    output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"Recorded final AppImage runtime integrity: {output}")
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--bundle-dir",
        type=Path,
        default=Path("target/release/bundle"),
        help="Tauri bundle directory containing one or more AppImages",
    )
    arguments = parser.parse_args()
    bundle_dir = arguments.bundle_dir.resolve()
    if not bundle_dir.is_dir():
        raise SystemExit(f"bundle directory not found: {bundle_dir}")
    appimages = find_appimages(bundle_dir)
    if not appimages:
        raise SystemExit(f"no AppImage found below: {bundle_dir}")
    for appimage in appimages:
        write_manifest(appimage)


if __name__ == "__main__":
    main()
