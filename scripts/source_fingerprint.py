from __future__ import annotations

import hashlib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INCLUDED_ROOTS = [
    ROOT / "crates",
    ROOT / "src",
    ROOT / "src-tauri",
    ROOT / "scripts",
    ROOT / "tests",
    ROOT / ".github" / "workflows",
    ROOT / "resources",
    ROOT / "sidecars",
    ROOT / "content-packs",
    ROOT / "components",
    ROOT / "vendor",
    ROOT / "public" / "starter-packs",
    ROOT / "schemas",
]
INCLUDED_FILES = [
    ROOT / "VERSION",
    ROOT / "Cargo.toml",
    ROOT / "Cargo.lock",
    ROOT / "package.json",
    ROOT / "package-lock.json",
    ROOT / "rust-toolchain.toml",
    ROOT / "requirements-dev.txt",
    ROOT / ".cargo" / "audit.toml",
    ROOT / "SECURITY_EXCEPTIONS.md",
    ROOT / "vite.config.ts",
    ROOT / "vitest.config.ts",
    ROOT / "playwright.config.ts",
    ROOT / "src-tauri" / "tauri.conf.json",
]
EXCLUDED_PARTS = {
    "node_modules",
    "target",
    "dist",
    ".cargo-gate",
    ".release-gate",
    "release-runtime",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
}
# Tauri regenerates this tree from its toolchain. It is build metadata, not authored source.
EXCLUDED_PREFIXES = {("src-tauri", "gen"), ("src-tauri", "resources", "tools")}


def is_excluded(path: Path) -> bool:
    relative_parts = path.relative_to(ROOT).parts
    if any(part in EXCLUDED_PARTS for part in relative_parts):
        return True
    return any(relative_parts[: len(prefix)] == prefix for prefix in EXCLUDED_PREFIXES)


def iter_files() -> list[Path]:
    files: set[Path] = {path for path in INCLUDED_FILES if path.is_file() and not is_excluded(path)}
    for base in INCLUDED_ROOTS:
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or is_excluded(path):
                continue
            files.add(path)
    return sorted(files, key=lambda path: path.relative_to(ROOT).as_posix())


def source_fingerprint() -> str:
    digest = hashlib.sha256()
    for path in iter_files():
        relative = path.relative_to(ROOT).as_posix().encode("utf-8")
        payload = path.read_bytes().replace(b"\r\n", b"\n")
        digest.update(len(relative).to_bytes(4, "big"))
        digest.update(relative)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest()


if __name__ == "__main__":
    if "--list" in sys.argv[1:]:
        for source_file in iter_files():
            print(source_file.relative_to(ROOT).as_posix())
    else:
        print(source_fingerprint())
