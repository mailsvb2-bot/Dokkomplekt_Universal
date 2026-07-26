#!/usr/bin/env python3
"""Prove that starter-pack generation is deterministic and all published copies match."""
from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WATCHED = [
    ROOT / "content-packs",
    ROOT / "public" / "starter-packs",
    ROOT / "src" / "data" / "starterPacks.ts",
]


def digest_tree() -> dict[str, str]:
    result: dict[str, str] = {}
    for item in WATCHED:
        paths = [item] if item.is_file() else sorted(path for path in item.rglob("*") if path.is_file())
        for path in paths:
            relative = path.relative_to(ROOT).as_posix()
            result[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def main() -> int:
    before = digest_tree()
    subprocess.run([sys.executable, str(ROOT / "scripts" / "generate_starter_content_packs.py")], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    after = digest_tree()
    if before != after:
        changed = sorted(set(before) | set(after))
        changed = [path for path in changed if before.get(path) != after.get(path)]
        raise SystemExit("STARTER PACK REPRODUCIBILITY FAILED:\n" + "\n".join(changed))
    templates = [path for path in after if path.startswith("content-packs/") and path.endswith((".docx", ".docm"))]
    print(f"STARTER PACK REPRODUCIBILITY PASSED: templates={len(templates)}; tracked_files={len(after)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
