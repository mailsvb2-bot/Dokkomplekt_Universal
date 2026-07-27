from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"


def test_appimage_validation_is_pipefail_safe_with_relative_bundle_path(
    tmp_path: Path,
) -> None:
    bundle_root = tmp_path / "bundle"
    bundle_dir = bundle_root / "appimage"
    bundle_dir.mkdir(parents=True)
    appimage = bundle_dir / "Dokkomplekt Universal_test_amd64.AppImage"
    appimage.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = "--appimage-extract" ]
mkdir -p squashfs-root/usr/bin
printf '#!/usr/bin/env bash\\n' > squashfs-root/AppRun
chmod +x squashfs-root/AppRun
for index in $(seq 1 5000); do
  path="squashfs-root/usr/bin/helper-$index"
  : > "$path"
  chmod +x "$path"
done
""",
        encoding="utf-8",
    )
    appimage.chmod(0o755)

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
    appimage.write_text(
        """#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = "--appimage-extract" ]
mkdir -p squashfs-root
printf '#!/usr/bin/env bash\\n' > squashfs-root/AppRun
chmod +x squashfs-root/AppRun
""",
        encoding="utf-8",
    )
    appimage.chmod(0o755)

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
