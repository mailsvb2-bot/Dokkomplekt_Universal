#!/usr/bin/env python3
"""Fail-closed verification for a complete offline Dokkomplekt runtime.

The verifier reads the status emitted by ``prepare_sidecars.py``, recomputes
SHA-256 for every staged file and checks that the platform has the minimum
components needed for Word/PDF/scan intake and deterministic PDF printing.
Optional ``--require-semantic-model`` additionally requires a locally bundled
llama.cpp server and a GGUF model. No network access is used.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    from scripts._release_policy import validate_relative_runtime_path
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_relative(value: str) -> Path:
    return Path(validate_relative_runtime_path(value, "sidecar path"))


def load_status(target: str) -> tuple[Path, dict[str, Any]]:
    target_dir = (TOOLS_ROOT / target).resolve()
    try:
        target_dir.relative_to(TOOLS_ROOT.resolve())
    except ValueError as exc:
        raise ValueError("target escapes resources/tools") from exc
    status_path = target_dir / "sidecar-status.json"
    if not status_path.is_file():
        raise FileNotFoundError(
            f"missing {status_path}; run scripts/prepare_sidecars.py first"
        )
    status = json.loads(status_path.read_text("utf-8"))
    if status.get("schema") != 1 or status.get("target") != target:
        raise ValueError("sidecar-status.json has an incompatible schema or target")
    if status.get("network_used") is not False:
        raise ValueError("offline runtime status must explicitly state network_used=false")
    files = status.get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("sidecar-status.json does not contain staged files")
    return target_dir, status


def verify_supply_chain(target_dir: Path, status: dict[str, Any]) -> None:
    if status.get("supply_chain_locked") is not True:
        raise ValueError("offline runtime is not supply-chain locked")
    for index, raw in enumerate(status.get("files", [])):
        for key in ("version", "source_url", "license", "license_path", "license_sha256"):
            value = str(raw.get(key, "")).strip()
            if not value:
                raise ValueError(f"files[{index}] is missing provenance field {key}")
        license_relative = safe_relative(str(raw["license_path"]))
        license_path = target_dir / license_relative
        if not license_path.is_file():
            raise ValueError(f"files[{index}] staged license notice is missing")
        if sha256_file(license_path) != str(raw["license_sha256"]).lower():
            raise ValueError(f"files[{index}] license notice SHA-256 mismatch")


def verify_entries(target_dir: Path, status: dict[str, Any]) -> dict[str, list[Path]]:
    tools: dict[str, list[Path]] = {}
    seen_paths: set[Path] = set()
    for index, raw in enumerate(status["files"]):
        if not isinstance(raw, dict):
            raise ValueError(f"files[{index}] is not an object")
        tool = str(raw.get("tool", "")).strip().lower()
        if not tool:
            raise ValueError(f"files[{index}].tool is empty")
        relative = safe_relative(str(raw.get("path", "")))
        if relative in seen_paths:
            raise ValueError(f"duplicate staged path: {relative.as_posix()}")
        seen_paths.add(relative)
        expected = str(raw.get("sha256", "")).strip().lower()
        if not SHA256_RE.fullmatch(expected):
            raise ValueError(f"files[{index}].sha256 is invalid")
        path = target_dir / relative
        if not path.is_file():
            raise FileNotFoundError(f"staged sidecar is missing: {path}")
        actual = sha256_file(path)
        if actual != expected:
            raise ValueError(
                f"SHA-256 mismatch for {relative.as_posix()}: expected {expected}, got {actual}"
            )
        size = raw.get("size_bytes")
        if not isinstance(size, int) or size != path.stat().st_size:
            raise ValueError(f"size mismatch for {relative.as_posix()}")
        tools.setdefault(tool, []).append(relative)
    return tools


def names(paths: list[Path]) -> set[str]:
    return {path.name.lower() for path in paths}


def require_file(tool_files: dict[str, list[Path]], tool: str, *candidates: str) -> None:
    actual = names(tool_files.get(tool, []))
    expected = {value.lower() for value in candidates}
    if not actual.intersection(expected):
        raise ValueError(
            f"offline runtime is missing {tool}: expected one of {sorted(expected)}"
        )


def require_suffix(tool_files: dict[str, list[Path]], tool: str, suffix: str) -> None:
    suffix_lower = suffix.lower().replace("\\", "/")
    if not any(path.as_posix().lower().endswith(suffix_lower) for path in tool_files.get(tool, [])):
        raise ValueError(f"offline runtime is missing {tool}/{suffix}")


def verify_distribution_review(
    target_dir: Path, status: dict[str, Any], tool_files: dict[str, list[Path]]
) -> None:
    raw = status.get("distribution_review")
    if not isinstance(raw, dict) or raw.get("complete_portable_tree") is not True:
        raise ValueError(
            "production runtime requires a reviewed complete portable-tree inventory"
        )
    for key in ("reviewer", "reviewed_at", "scope", "inventory_path", "inventory_sha256"):
        if not str(raw.get(key, "")).strip():
            raise ValueError(f"distribution_review is missing {key}")
    inventory_path = target_dir / safe_relative(str(raw["inventory_path"]))
    if not inventory_path.is_file():
        raise FileNotFoundError("staged distribution inventory is missing")
    expected_sha = str(raw["inventory_sha256"]).strip().lower()
    if not SHA256_RE.fullmatch(expected_sha) or sha256_file(inventory_path) != expected_sha:
        raise ValueError("staged distribution inventory SHA-256 mismatch")
    inventory = json.loads(inventory_path.read_text("utf-8"))
    if inventory.get("schema") != 1 or not isinstance(inventory.get("tools"), dict):
        raise ValueError("distribution inventory has an incompatible schema")
    actual = {tool: {path.as_posix() for path in paths} for tool, paths in tool_files.items()}
    declared: dict[str, set[str]] = {}
    for tool, raw_paths in inventory["tools"].items():
        if not isinstance(raw_paths, list) or not raw_paths:
            raise ValueError(f"distribution inventory for {tool} must be non-empty")
        paths = {safe_relative(str(value)).as_posix() for value in raw_paths}
        if len(paths) != len(raw_paths):
            raise ValueError(f"distribution inventory for {tool} contains duplicates")
        declared[str(tool)] = paths
    if declared != actual:
        raise ValueError("staged runtime does not exactly match the reviewed distribution inventory")

    libreoffice = actual.get("libreoffice", set())
    lower_lo = {path.lower() for path in libreoffice}
    if not any(path.endswith("/soffice.exe") or path == "soffice.exe" for path in lower_lo):
        raise ValueError("LibreOffice inventory lacks soffice.exe")
    if not any(path.endswith("/soffice.bin") or path == "soffice.bin" for path in lower_lo):
        raise ValueError("LibreOffice inventory lacks soffice.bin; the portable tree is incomplete")
    if not any(path.endswith("/fundamental.ini") or path == "fundamental.ini" for path in lower_lo):
        raise ValueError("LibreOffice inventory lacks fundamental.ini; the portable tree is incomplete")

    seven_zip = actual.get("7zip", set())
    lower_7z = {path.lower() for path in seven_zip}
    uses_split_7z = any(path.endswith("/7z.exe") or path == "7z.exe" for path in lower_7z)
    if uses_split_7z and not any(path.endswith("/7z.dll") or path == "7z.dll" for path in lower_7z):
        raise ValueError("7z.exe requires the matching 7z.dll in the reviewed portable tree")


def verify_production_plausibility(target_dir: Path, tool_files: dict[str, list[Path]]) -> None:
    """Reject placeholder/test payloads before a production installer is built."""
    for tool, paths in tool_files.items():
        for relative in paths:
            path = target_dir / relative
            lower = relative.name.lower()
            if lower.endswith(".exe"):
                with path.open("rb") as stream:
                    if stream.read(2) != b"MZ":
                        raise ValueError(f"production Windows executable is not a PE file: {relative}")
                if path.stat().st_size < 32 * 1024:
                    raise ValueError(f"production executable is implausibly small: {relative}")
            if lower.endswith(".traineddata") and path.stat().st_size < 512 * 1024:
                raise ValueError(f"Tesseract language data is implausibly small: {relative}")
            if tool == "semantic_model" and lower.endswith(".gguf"):
                if path.stat().st_size < 64 * 1024 * 1024:
                    raise ValueError(f"production GGUF model is implausibly small: {relative}")


def verify_required_runtime(
    tool_files: dict[str, list[Path]],
    require_model: bool,
    require_msgconvert: bool = False,
) -> None:
    require_file(tool_files, "tesseract", "tesseract.exe", "tesseract")
    require_suffix(tool_files, "tesseract", "tessdata/rus.traineddata")
    require_suffix(tool_files, "tesseract", "tessdata/eng.traineddata")
    require_file(tool_files, "poppler", "pdftotext.exe", "pdftotext")
    require_file(tool_files, "poppler", "pdftoppm.exe", "pdftoppm")
    require_file(tool_files, "libreoffice", "soffice.exe", "soffice")
    require_file(tool_files, "sumatrapdf", "sumatrapdf.exe", "sumatrapdf")
    require_file(tool_files, "7zip", "7z.exe", "7zz.exe", "7z", "7zz")
    if require_msgconvert:
        # The Windows desktop resolver currently executes msgconvert.exe directly.
        # A msgconvert.pl + Perl tree may be probeable in isolation, but it is not
        # production-ready until the application has an equally locked launch path.
        require_file(tool_files, "msgconvert", "msgconvert.exe")

    if require_model:
        require_file(
            tool_files,
            "llama_cpp",
            "llama-server.exe",
            "llama-server",
            "server.exe",
            "server",
        )
        models = tool_files.get("semantic_model", [])
        if not any(path.suffix.lower() == ".gguf" for path in models):
            raise ValueError("offline semantic runtime requires a verified .gguf model")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    parser.add_argument("--require-semantic-model", action="store_true")
    parser.add_argument("--require-supply-chain", action="store_true")
    parser.add_argument("--production", action="store_true", help="reject test/placeholder payloads")
    parser.add_argument(
        "--require-distribution-review",
        action="store_true",
        help="require an exact reviewed inventory of the complete portable runtime tree",
    )
    args = parser.parse_args()

    target_dir, status = load_status(args.target)
    tools = verify_entries(target_dir, status)
    if args.require_supply_chain:
        verify_supply_chain(target_dir, status)
    verify_required_runtime(tools, args.require_semantic_model, args.production)
    if args.require_distribution_review or args.production:
        verify_distribution_review(target_dir, status, tools)
    if args.production:
        verify_production_plausibility(target_dir, tools)
    model_note = " + llama.cpp/GGUF" if args.require_semantic_model else ""
    supply_note = " + provenance/licenses" if args.require_supply_chain else ""
    review_note = " + reviewed-portable-tree" if (args.require_distribution_review or args.production) else ""
    production_note = " + production-plausibility" if args.production else ""
    print(
        f"OFFLINE RUNTIME READY: target={args.target}; "
        f"files={sum(len(items) for items in tools.values())}{model_note}{supply_note}{review_note}{production_note}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
