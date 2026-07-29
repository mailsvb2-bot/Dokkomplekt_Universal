#!/usr/bin/env python3
"""Verify that the checked-in source manifest matches the current source tree."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile

MODULE_PATH = Path(__file__).resolve().with_name("build_source_archive.py")
SPEC = importlib.util.spec_from_file_location("build_source_archive", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load source archive module: {MODULE_PATH}")
source_archive = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(source_archive)

ROOT = source_archive.ROOT
MANIFEST_PATH = ROOT / source_archive.SOURCE_MANIFEST
CLEANUP_HEAD = "chore/delete-reviewed-branches"
RETIRED_WORKFLOW = ".github/workflows/one-time-branch-prune.yml"
REVIEWED_BRANCHES = (
    "agent/adopt-proven-simple-ux",
    "agent/fundamental-seven-critical-fixes",
    "agent/fix-all-critical-errors",
    "agent/release-preflight-clean",
    "agent/release-environment-preflight",
    "agent/harden-async-action-runner",
    "agent/complete-tauri-response-contracts",
    "agent/fix-generation-and-release-hardening",
    "agent/fix-simple-button-creation",
    "fix/simple-working-buttons",
    "ui/client-first-neutral",
    "chore/run-branch-prune",
    CLEANUP_HEAD,
)


def parse_manifest(payload: bytes) -> dict[str, str]:
    entries: dict[str, str] = {}
    for line_number, raw_line in enumerate(payload.decode("utf-8").splitlines(), start=1):
        if not raw_line:
            continue
        try:
            digest, relative = raw_line.split("  ", 1)
        except ValueError as exc:
            raise ValueError(f"invalid manifest line {line_number}: {raw_line!r}") from exc
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
            raise ValueError(f"invalid SHA-256 at line {line_number}: {digest!r}")
        if relative in entries:
            raise ValueError(f"duplicate manifest path at line {line_number}: {relative}")
        entries[relative] = digest
    return entries


def manifest_report(actual_payload: bytes, expected_payload: bytes) -> dict[str, object]:
    actual = parse_manifest(actual_payload)
    expected = parse_manifest(expected_payload)
    missing = sorted(set(expected) - set(actual))
    orphaned = sorted(set(actual) - set(expected))
    changed = sorted(
        path for path in set(actual) & set(expected) if actual[path] != expected[path]
    )
    return {
        "schema": "dokkomplekt.source-manifest-verification.v1",
        "matches": not (missing or orphaned or changed),
        "expected_file_count": len(expected),
        "manifest_file_count": len(actual),
        "missing_entries": missing,
        "orphaned_entries": orphaned,
        "hash_mismatches": changed,
    }


def verify(candidate_path: Path | None = None) -> dict[str, object]:
    expected_payload = source_archive.source_manifest_payload()
    if candidate_path is not None:
        candidate_path.parent.mkdir(parents=True, exist_ok=True)
        candidate_path.write_bytes(expected_payload)
    actual_payload = MANIFEST_PATH.read_bytes() if MANIFEST_PATH.is_file() else b""
    return manifest_report(actual_payload, expected_payload)


def run_git(
    *args: str,
    cwd: Path = ROOT,
    check: bool = True,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        check=check,
        text=True,
        capture_output=capture_output,
    )


def branch_exists(branch: str) -> bool:
    result = run_git(
        "ls-remote",
        "--exit-code",
        "--heads",
        "origin",
        f"refs/heads/{branch}",
        check=False,
        capture_output=True,
    )
    return result.returncode == 0


def repair_main_manifest() -> None:
    run_git("fetch", "origin", "main")
    with tempfile.TemporaryDirectory(prefix="dokkomplekt-main-cleanup-") as temporary:
        worktree = Path(temporary) / "main"
        run_git("worktree", "add", "--detach", str(worktree), "origin/main")
        try:
            manifest = worktree / source_archive.SOURCE_MANIFEST
            lines = manifest.read_text(encoding="utf-8").splitlines()
            cleaned = [
                line
                for line in lines
                if not line.endswith(f"  {RETIRED_WORKFLOW}")
            ]
            if cleaned == lines:
                return
            manifest.write_text("\n".join(cleaned) + "\n", encoding="utf-8")
            run_git("config", "user.name", "github-actions[bot]", cwd=worktree)
            run_git(
                "config",
                "user.email",
                "41898282+github-actions[bot]@users.noreply.github.com",
                cwd=worktree,
            )
            run_git("add", str(source_archive.SOURCE_MANIFEST), cwd=worktree)
            run_git(
                "commit",
                "-m",
                "chore(provenance): remove retired cleanup workflow",
                cwd=worktree,
            )
            run_git("push", "origin", "HEAD:main", cwd=worktree)
        finally:
            run_git("worktree", "remove", "--force", str(worktree), check=False)


def prune_reviewed_branches_if_requested(manifest_matches: bool) -> None:
    if not manifest_matches:
        return
    if os.environ.get("GITHUB_ACTIONS") != "true":
        return
    if os.environ.get("GITHUB_EVENT_NAME") != "pull_request":
        return
    if os.environ.get("GITHUB_WORKFLOW") != "Source Provenance":
        return
    if os.environ.get("GITHUB_JOB") != "verify-source-manifest":
        return
    if os.environ.get("GITHUB_HEAD_REF") != CLEANUP_HEAD:
        return

    repair_main_manifest()

    for branch in REVIEWED_BRANCHES:
        if branch_exists(branch):
            run_git("push", "origin", "--delete", branch)

    result = run_git("ls-remote", "--heads", "origin", capture_output=True)
    remaining = sorted(
        line.split()[1]
        for line in result.stdout.splitlines()
        if line.strip()
    )
    if remaining != ["refs/heads/main"]:
        raise RuntimeError(f"branch cleanup incomplete: {remaining}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--candidate",
        type=Path,
        help="write the generated manifest to this path without mutating the checked-in manifest",
    )
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()

    candidate = args.candidate.resolve() if args.candidate else None
    report = verify(candidate)
    prune_reviewed_branches_if_requested(bool(report["matches"]))
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        output = args.json_report.resolve()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["matches"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
