#!/usr/bin/env python3
"""Build and independently verify a deterministic clean source archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE_MANIFEST = "SOURCE_MANIFEST_SHA256.txt"
EXCLUDED_DIRS = {
    ".git",
    ".cargo-gate",
    ".release-gate",
    "release-runtime",
    "node_modules",
    "dist",
    "target",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "playwright-report",
    "test-results",
    # CI/runtime evidence is not authored source. It is hashed separately.
    "verification",
    "build-evidence",
}
EXCLUDED_SUFFIXES = {".pyc", ".pyo"}
ZIP_TIMESTAMP = (2026, 7, 26, 0, 0, 0)
EXCLUDED_PREFIXES = {("src-tauri", "resources", "tools")}
ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES = {
    ("src-tauri", "resources", "tools", "windows-x86_64", "sidecar-status.json"),
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_excluded(path: Path) -> bool:
    relative = path.relative_to(ROOT)
    if any(part in EXCLUDED_DIRS for part in relative.parts):
        return True
    if any(relative.parts[:len(prefix)] == prefix for prefix in EXCLUDED_PREFIXES):
        if relative.parts not in ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES:
            return True
    if path.suffix.lower() in EXCLUDED_SUFFIXES:
        return True
    return relative.as_posix() == SOURCE_MANIFEST


def source_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if is_excluded(path):
            continue
        if path.is_symlink():
            raise RuntimeError(f"Symlink is forbidden in source archive: {path.relative_to(ROOT)}")
        if path.is_file():
            files.append(path)
    return sorted(files, key=lambda item: item.relative_to(ROOT).as_posix())


def source_manifest_payload(files: list[Path] | None = None) -> bytes:
    selected = source_files() if files is None else files
    lines = [
        f"{sha256_file(path)}  {path.relative_to(ROOT).as_posix()}"
        for path in selected
    ]
    return ("\n".join(lines) + "\n").encode("utf-8")


def write_source_manifest(files: list[Path]) -> bytes:
    payload = source_manifest_payload(files)
    (ROOT / SOURCE_MANIFEST).write_bytes(payload)
    return payload


def zip_info(archive_path: str, source_path: Path | None = None) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(archive_path, ZIP_TIMESTAMP)
    info.create_system = 3
    mode = 0o644
    if source_path is not None and os.access(source_path, os.X_OK):
        mode = 0o755
    info.external_attr = (stat.S_IFREG | mode) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def validate_member_name(name: str, top_level: str) -> None:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != top_level:
        raise RuntimeError(f"Unsafe ZIP member: {name}")


def build_archive(output: Path, top_level: str) -> tuple[int, bytes]:
    files = source_files()
    manifest_payload = write_source_manifest(files)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files:
            relative = path.relative_to(ROOT).as_posix()
            archive.writestr(zip_info(f"{top_level}/{relative}", path), path.read_bytes())
        archive.writestr(zip_info(f"{top_level}/{SOURCE_MANIFEST}"), manifest_payload)
    return len(files) + 1, manifest_payload


def verify_archive(output: Path, top_level: str, expected_manifest: bytes) -> None:
    with zipfile.ZipFile(output, "r") as archive:
        bad = archive.testzip()
        if bad is not None:
            raise RuntimeError(f"ZIP CRC failed: {bad}")
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise RuntimeError("ZIP contains duplicate member names")
        for name in names:
            validate_member_name(name, top_level)
            member_parts = PurePosixPath(name).parts[1:]
            if any(part in EXCLUDED_DIRS for part in member_parts):
                raise RuntimeError(f"Excluded directory leaked into ZIP: {name}")
            if any(member_parts[:len(prefix)] == prefix for prefix in EXCLUDED_PREFIXES):
                if tuple(member_parts) not in ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES:
                    raise RuntimeError(f"Generated sidecar staging leaked into source ZIP: {name}")
        manifest_name = f"{top_level}/{SOURCE_MANIFEST}"
        archived_manifest = archive.read(manifest_name)
        if archived_manifest != expected_manifest:
            raise RuntimeError("Archived source manifest differs from generated manifest")
        for line in archived_manifest.decode("utf-8").splitlines():
            expected_hash, relative = line.split("  ", 1)
            member = f"{top_level}/{relative}"
            actual_hash = sha256_bytes(archive.read(member))
            if actual_hash != expected_hash:
                raise RuntimeError(f"SHA-256 mismatch in ZIP: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--top-level", required=True)
    parser.add_argument("--manifest-json", type=Path, required=True)
    args = parser.parse_args()

    version = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
    count, manifest_payload = build_archive(args.output.resolve(), args.top_level)
    verify_archive(args.output.resolve(), args.top_level, manifest_payload)
    archive_hash = sha256_file(args.output.resolve())
    sha_path = args.output.with_suffix(args.output.suffix + ".sha256").resolve()
    sha_path.write_text(f"{archive_hash}  {args.output.name}\n", encoding="utf-8")
    metadata = {
        "schema": "dokkomplekt.source-archive.v1",
        "version": version,
        "archive": args.output.name,
        "archive_sha256": archive_hash,
        "archive_size_bytes": args.output.stat().st_size,
        "top_level_directory": args.top_level,
        "source_file_count_including_manifest": count,
        "source_manifest": SOURCE_MANIFEST,
        "source_manifest_sha256": sha256_bytes(manifest_payload),
        "verification": {
            "zip_crc": "passed",
            "safe_member_paths": "passed",
            "duplicate_members": "passed",
            "source_sha256_entries": "passed",
        },
        "excluded_directories": sorted(EXCLUDED_DIRS),
        "excluded_prefixes": ["/".join(prefix) for prefix in sorted(EXCLUDED_PREFIXES)],
        "allowed_files_under_excluded_prefixes": [
            "/".join(parts) for parts in sorted(ALLOWED_FILES_UNDER_EXCLUDED_PREFIXES)
        ],
    }
    args.manifest_json.resolve().parent.mkdir(parents=True, exist_ok=True)
    args.manifest_json.resolve().write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(metadata, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
