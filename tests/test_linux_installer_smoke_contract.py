from __future__ import annotations

import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tests" / "installer" / "linux_installer_contract.sh"


def test_appimage_validation_is_pipefail_safe(tmp_path: Path) -> None:
    bundle_dir = tmp_path / "bundle" / "appimage"
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
    result = subprocess.run(
        ["bash", str(CONTRACT), str(tmp_path / "bundle")],
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
