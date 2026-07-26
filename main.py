#!/usr/bin/env python3
"""Click launcher for the Dokkomplekt Universal source tree.

The production application is a signed Tauri executable.  In a source checkout this
wrapper delegates to the Windows batch launcher, which either starts an existing
binary or runs the pinned development toolchain with useful logging.
"""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def main() -> int:
    os.chdir(ROOT)
    if os.name == "nt":
        comspec = os.environ.get("COMSPEC", "cmd.exe")
        launcher = ROOT / "main.bat"
        return subprocess.call([comspec, "/d", "/s", "/c", f'"{launcher}"'], cwd=ROOT)
    print("Dokkomplekt Universal source launcher")
    print("On Linux/macOS use ./main.sh after installing the pinned Node.js and Rust toolchains.")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
