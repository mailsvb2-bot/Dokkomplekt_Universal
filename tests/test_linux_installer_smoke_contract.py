from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"
REQUIRED = ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0")


def _fake_appimage_script(*, helper_count: int = 0) -> str:
    return f'''#!/usr/bin/env bash
set -euo pipefail
[ "${{1:-}}" = "--appimage-extract" ]
mkdir -p squashfs-root/usr/bin squashfs-root/usr/lib squashfs-root/usr/share/dokkomplekt
python - <<'PY_EMBEDDED'
import hashlib
import json
import platform
from pathlib import Path

arch_name, machine = {{
    "x86_64": ("x86_64", 62),
    "amd64": ("x86_64", 62),
    "aarch64": ("aarch64", 183),
    "arm64": ("aarch64", 183),
}}[platform.machine().lower()]
root = Path("squashfs-root")
records = []
for name in ("libGLESv2.so.2", "libEGL.so.1", "libGLdispatch.so.0"):
    data = bytearray(64)
    data[:4] = b"\\x7fELF"
    data[4] = 2
    data[5] = 1
    data[18:20] = machine.to_bytes(2, "little")
    path = root / "usr/lib" / name
    path.write_bytes(data)
    path.chmod(0o755)
    records.append({{
        "name": name,
        "elfMachine": machine,
        "sourceSize": len(data),
        "sourceSha256": hashlib.sha256(data).hexdigest(),
    }})
(root / "usr/share/dokkomplekt/appimage-runtime.json").write_text(
    json.dumps({{
        "schema": 2,
        "phase": "pre-linuxdeploy",
        "targetArch": arch_name,
        "libraries": records,
    }}),
    encoding="utf-8",
)
for index in range({helper_count}):
    path = root / "usr/bin" / f"helper-{{index}}"
    path.write_bytes(b"#!/bin/sh\\n")
    path.chmod(0o755)
PY_EMBEDDED
printf '#!/usr/bin/env bash\\n' > squashfs-root/AppRun
chmod +x squashfs-root/AppRun
'''


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _write_runtime_sidecar(appimage: Path, tmp_path: Path) -> None:
    extract_dir = tmp_path / f"sidecar-extract-{appimage.stem}"
    extract_dir.mkdir()
    subprocess.run(
        [str(appimage.resolve()), "--appimage-extract"],
        cwd=extract_dir,
        check=True,
        stdout=subprocess.DEVNULL,
    )
    root = extract_dir / "squashfs-root"
    source_path = root / "usr/share/dokkomplekt/appimage-runtime.json"
    source_raw = source_path.read_bytes()
    source = json.loads(source_raw.decode("utf-8"))
    source_records = {record["name"]: record for record in source["libraries"]}
    arch_name, machine = {
        "x86_64": ("x86_64", 62),
        "amd64": ("x86_64", 62),
        "aarch64": ("aarch64", 183),
        "arm64": ("aarch64", 183),
    }[platform.machine().lower()]
    records = []
    for name in REQUIRED:
        path = root / "usr/lib" / name
        staged = source_records[name]
        records.append(
            {
                "name": name,
                "elfMachine": machine,
                "size": path.stat().st_size,
                "sha256": _sha256_file(path),
                "sourceSize": staged["sourceSize"],
                "sourceSha256": staged["sourceSha256"],
            }
        )
    sidecar = Path(f"{appimage}.runtime-manifest.json")
    sidecar.write_text(
        json.dumps(
            {
                "schema": 1,
                "phase": "post-linuxdeploy",
                "targetArch": arch_name,
                "appImage": {
                    "name": appimage.name,
                    "size": appimage.stat().st_size,
                    "sha256": _sha256_file(appimage),
                },
                "embeddedSourceManifestSha256": hashlib.sha256(source_raw).hexdigest(),
                "libraries": records,
            }
        ),
        encoding="utf-8",
    )
    shutil.rmtree(extract_dir)


def test_appimage_validation_is_pipefail_safe_with_relative_bundle_path(
    tmp_path: Path,
) -> None:
    bundle_root = tmp_path / "bundle"
    bundle_dir = bundle_root / "appimage"
    bundle_dir.mkdir(parents=True)
    appimage = bundle_dir / "Dokkomplekt Universal_test_amd64.AppImage"
    appimage.write_text(_fake_appimage_script(helper_count=1024), encoding="utf-8")
    appimage.chmod(0o755)
    _write_runtime_sidecar(appimage, tmp_path)

    env = os.environ.copy()
    env.update(
        {
            "DOKKOMPLEKT_REQUIRED_LINUX_BUNDLES": "appimage",
            "DOKKOMPLEKT_SKIP_LINUX_INSTALL_SMOKE": "1",
        }
    )
    relative_bundle_root = os.path.relpath(bundle_root, ROOT)
    result = subprocess.run(
        ["bash", str(CONTRACT), relative_bundle_root],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=30,
        check=False,
    )

    assert result.returncode == 0, result.stdout
    assert "Linux bundle validation OK" in result.stdout
    assert str(bundle_root.resolve()) in result.stdout


def test_cleanup_retries_a_transient_remove_failure(tmp_path: Path) -> None:
    bundle_root = tmp_path / "bundle"
    bundle_dir = bundle_root / "appimage"
    bundle_dir.mkdir(parents=True)
    appimage = bundle_dir / "Dokkomplekt Universal_cleanup_test_amd64.AppImage"
    appimage.write_text(_fake_appimage_script(), encoding="utf-8")
    appimage.chmod(0o755)
    _write_runtime_sidecar(appimage, tmp_path)

    real_rm = shutil.which("rm")
    assert real_rm is not None
    fake_bin = tmp_path / "fake-bin"
    fake_bin.mkdir()
    first_failure_marker = tmp_path / "rm-failed-once"
    fake_rm = fake_bin / "rm"
    fake_rm.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
state="${DOKKOMPLEKT_TEST_RM_STATE:?}"
if [ ! -e "$state" ]; then
  : > "$state"
  exit 1
fi
exec "${DOKKOMPLEKT_REAL_RM:?}" "$@"
""",
        encoding="utf-8",
    )
    fake_rm.chmod(0o755)

    env = os.environ.copy()
    env.update(
        {
            "PATH": f"{fake_bin}{os.pathsep}{env['PATH']}",
            "DOKKOMPLEKT_REQUIRED_LINUX_BUNDLES": "appimage",
            "DOKKOMPLEKT_SKIP_LINUX_INSTALL_SMOKE": "1",
            "DOKKOMPLEKT_TEST_RM_STATE": str(first_failure_marker),
            "DOKKOMPLEKT_REAL_RM": real_rm,
        }
    )
    result = subprocess.run(
        ["bash", str(CONTRACT), str(bundle_root)],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=30,
        check=False,
    )

    assert first_failure_marker.exists(), "the test did not inject a cleanup failure"
    assert result.returncode == 0, result.stdout
    assert "Linux bundle validation OK" in result.stdout
