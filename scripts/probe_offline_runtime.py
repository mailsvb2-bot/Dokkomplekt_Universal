#!/usr/bin/env python3
"""Execute staged Windows sidecars before an installer may be released.

Hash checks prove identity, not launchability. This probe catches incomplete
portable distributions (missing DLLs/data files, wrong architecture, broken
executables) by starting every production entry point with a bounded, read-only
version/help command. It never sends documents to a network service.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
TIMEOUT_SECONDS = 20


def safe_relative(raw: str) -> Path:
    path = Path(raw.replace("\\", "/"))
    if path.is_absolute() or not path.parts or ".." in path.parts:
        raise ValueError(f"unsafe staged path: {raw!r}")
    return path


def load_status(target: str) -> tuple[Path, dict]:
    target_dir = (TOOLS_ROOT / target).resolve()
    target_dir.relative_to(TOOLS_ROOT.resolve())
    status_path = target_dir / "sidecar-status.json"
    if not status_path.is_file():
        raise FileNotFoundError(f"missing verified runtime status: {status_path}")
    status = json.loads(status_path.read_text("utf-8"))
    if status.get("target") != target or not isinstance(status.get("files"), list):
        raise ValueError("sidecar status does not match requested target")
    return target_dir, status


def tool_path(target_dir: Path, status: dict, tool: str, names: Iterable[str]) -> Path:
    expected = {name.lower() for name in names}
    for item in status["files"]:
        relative = safe_relative(str(item.get("path", "")))
        if str(item.get("tool", "")).lower() == tool and relative.name.lower() in expected:
            path = target_dir / relative
            if path.is_file():
                return path
    raise FileNotFoundError(f"runtime entry point is missing for {tool}: {sorted(expected)}")


def run_probe(title: str, command: list[str], accepted_codes: set[int] | None = None) -> None:
    completed = subprocess.run(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=TIMEOUT_SECONDS,
        check=False,
        env={**os.environ, "DOKKOMPLEKT_RUNTIME_PROBE": "1"},
    )
    accepted = accepted_codes or {0}
    if completed.returncode not in accepted:
        tail = completed.stdout[-1200:].strip()
        raise RuntimeError(
            f"{title} failed to start (exit {completed.returncode}). "
            f"The portable runtime may miss DLLs or use the wrong architecture. Output: {tail}"
        )
    print(f"RUNTIME PROBE OK: {title}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    args = parser.parse_args()
    if os.name != "nt":
        raise RuntimeError("executable runtime probing must run on the target Windows machine")

    target_dir, status = load_status(args.target)
    probes = [
        ("Tesseract", tool_path(target_dir, status, "tesseract", ["tesseract.exe"]), ["--version"], {0}),
        ("Poppler pdftotext", tool_path(target_dir, status, "poppler", ["pdftotext.exe"]), ["-v"], {0, 1, 99}),
        ("Poppler pdftoppm", tool_path(target_dir, status, "poppler", ["pdftoppm.exe"]), ["-v"], {0, 1, 99}),
        ("LibreOffice", tool_path(target_dir, status, "libreoffice", ["soffice.exe"]), ["--headless", "--version"], {0}),
        ("SumatraPDF", tool_path(target_dir, status, "sumatrapdf", ["sumatrapdf.exe"]), ["-help"], {0}),
        ("7-Zip", tool_path(target_dir, status, "7zip", ["7z.exe", "7zz.exe"]), ["i"], {0}),
        ("llama.cpp", tool_path(target_dir, status, "llama_cpp", ["llama-server.exe", "server.exe"]), ["--version"], {0}),
    ]
    for title, executable, arguments, accepted in probes:
        run_probe(title, [str(executable), *arguments], accepted)
    print(f"OFFLINE RUNTIME EXECUTION PROBE PASSED: target={args.target}; probes={len(probes)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
