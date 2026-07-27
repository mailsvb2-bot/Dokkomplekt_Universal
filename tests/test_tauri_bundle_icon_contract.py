from __future__ import annotations

import json
import struct
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TAURI_ROOT = ROOT / "src-tauri"
CONFIG_PATH = TAURI_ROOT / "tauri.conf.json"


def _png_dimensions(path: Path) -> tuple[int, int]:
    header = path.read_bytes()[:24]
    assert header[:8] == b"\x89PNG\r\n\x1a\n", f"not a PNG file: {path}"
    return struct.unpack(">II", header[16:24])


def test_tauri_bundle_declares_existing_cross_platform_icons() -> None:
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    icon_paths = config.get("bundle", {}).get("icon")

    assert isinstance(icon_paths, list) and icon_paths, (
        "Tauri bundle.icon must explicitly declare application icons; "
        "otherwise AppImage packaging cannot select a square icon"
    )
    assert len(icon_paths) == len(set(icon_paths)), (
        "Tauri bundle.icon must not contain duplicate paths because target-specific "
        "icon selection must remain deterministic"
    )

    resolved = [TAURI_ROOT / relative_path for relative_path in icon_paths]
    missing = [path.relative_to(ROOT).as_posix() for path in resolved if not path.is_file()]
    assert not missing, f"declared Tauri icons are missing: {missing}"

    square_pngs: list[tuple[Path, int]] = []
    for path in resolved:
        if path.suffix.lower() != ".png":
            continue
        width, height = _png_dimensions(path)
        if width == height:
            square_pngs.append((path, width))

    assert square_pngs, "AppImage packaging requires at least one square PNG icon"
    assert any(size >= 128 for _, size in square_pngs), (
        "Tauri bundles require a square PNG icon of at least 128x128 for production packaging"
    )
    assert any(path.suffix.lower() == ".ico" for path in resolved), (
        "Windows NSIS packaging must declare an ICO application icon"
    )
