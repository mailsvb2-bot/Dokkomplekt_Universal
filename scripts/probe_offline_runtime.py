#!/usr/bin/env python3
"""Execute staged Windows sidecars before an installer may be released.

``core`` probes only the document-processing executables embedded by the stock
installer. ``full`` additionally probes llama.cpp. The GGUF itself is data and
is verified by the offline-runtime integrity gate.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Iterable

try:
    from scripts._release_policy import validate_relative_runtime_path
    from scripts._runtime_profile import CORE_PROFILE, FULL_PROFILE, PROFILES, normalize_profile
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path
    from _runtime_profile import CORE_PROFILE, FULL_PROFILE, PROFILES, normalize_profile

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
TIMEOUT_SECONDS = 20


def safe_relative(value: str) -> Path:
    return Path(validate_relative_runtime_path(value, "staged runtime path"))


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


def runtime_probes(
    target_dir: Path, status: dict, profile: str = FULL_PROFILE
) -> list[tuple[str, list[str], set[int]]]:
    normalize_profile(profile, semantic_model_required=(profile == FULL_PROFILE))
    probes = [
        ("Tesseract", [str(tool_path(target_dir, status, "tesseract", ["tesseract.exe"])), "--version"], {0}),
        ("Poppler pdftotext", [str(tool_path(target_dir, status, "poppler", ["pdftotext.exe"])), "-v"], {0, 1, 99}),
        ("Poppler pdftoppm", [str(tool_path(target_dir, status, "poppler", ["pdftoppm.exe"])), "-v"], {0, 1, 99}),
        ("LibreOffice", [str(tool_path(target_dir, status, "libreoffice", ["soffice.exe"])), "--headless", "--version"], {0}),
        ("SumatraPDF", [str(tool_path(target_dir, status, "sumatrapdf", ["sumatrapdf.exe"])), "-help"], {0}),
        ("7-Zip", [str(tool_path(target_dir, status, "7zip", ["7z.exe", "7zz.exe"])), "i"], {0}),
    ]
    if profile == FULL_PROFILE:
        probes.append(("llama.cpp", [str(tool_path(target_dir, status, "llama_cpp", ["llama-server.exe", "server.exe"])), "--version"], {0}))
    return probes


def _execute_probe(title: str, command: list[str], accepted_codes: set[int]) -> None:
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
    if completed.returncode not in accepted_codes:
        tail = completed.stdout[-1200:].strip()
        raise RuntimeError(
            f"{title} failed to start (exit {completed.returncode}). "
            f"The portable runtime may miss DLLs or use the wrong architecture. Output: {tail}"
        )
    print(f"RUNTIME PROBE OK: {title}")


def run_probe(title: str, command: list[str], accepted_codes: set[int] | None = None) -> None:
    accepted = accepted_codes or {0}
    if title == "LibreOffice":
        # LibreOffice owns a per-user profile and can wait on a stale lock or a
        # first-start profile transition even when the staged binary itself is
        # healthy. Release probing must be deterministic and must not read or
        # mutate the runner/user's real LibreOffice profile. Keep the isolated
        # profile alive until the child exits; Path.as_uri() also handles spaces
        # and non-ASCII Windows profile roots correctly.
        with tempfile.TemporaryDirectory(prefix="dokkomplekt-lo-probe-") as profile_dir:
            isolated_command = [
                command[0],
                f"-env:UserInstallation={Path(profile_dir).resolve().as_uri()}",
                *command[1:],
            ]
            _execute_probe(title, isolated_command, accepted)
        return
    _execute_probe(title, command, accepted)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    parser.add_argument("--profile", choices=PROFILES)
    args = parser.parse_args()
    if os.name != "nt":
        raise RuntimeError("executable runtime probing must run on the target Windows machine")

    target_dir, status = load_status(args.target)
    if args.profile:
        profile = args.profile
    elif status.get("runtime_profile"):
        profile = normalize_profile(
            status.get("runtime_profile"),
            semantic_model_required=status.get("semantic_model_required"),
        )
    else:
        profile = FULL_PROFILE
    probes = runtime_probes(target_dir, status, profile)
    for title, command, accepted in probes:
        run_probe(title, command, accepted)
    print(f"OFFLINE RUNTIME EXECUTION PROBE PASSED: target={args.target}; profile={profile}; probes={len(probes)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
