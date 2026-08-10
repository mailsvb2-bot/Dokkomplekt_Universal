#!/usr/bin/env python3
"""Resolve production release identity from the checked-out public repository.

GitHub workflow metadata is deliberately not a release identity source because
production hardware validation runs in a separate private repository. The exact
public release is instead bound to the repository that is actually checked out:
its immutable HEAD commit and canonical origin remote.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path
from urllib.parse import urlparse

CANONICAL_REPOSITORY = "mailsvb2-bot/Dokkomplekt_Universal"
_SHA_RE = re.compile(r"^[0-9a-f]{40}$")
_SCP_ORIGIN_RE = re.compile(r"^git@github\.com:(?P<repository>[^?#]+?)(?:\.git)?$")


def normalize_github_origin(value: str) -> str:
    raw = value.strip()
    scp = _SCP_ORIGIN_RE.fullmatch(raw)
    if scp:
        repository = scp.group("repository")
    else:
        parsed = urlparse(raw)
        if parsed.scheme not in {"https", "ssh"}:
            raise ValueError("origin must use HTTPS or SSH")
        if (parsed.hostname or "").lower() != "github.com":
            raise ValueError("origin must be hosted on github.com")
        if parsed.query or parsed.fragment:
            raise ValueError("origin must not contain query or fragment data")
        if parsed.scheme == "https" and (parsed.username or parsed.password):
            raise ValueError("origin URL must not embed credentials")
        if parsed.scheme == "ssh" and parsed.username not in {None, "git"}:
            raise ValueError("SSH origin must use the git account")
        repository = parsed.path.lstrip("/")
        if repository.endswith(".git"):
            repository = repository[:-4]

    repository = repository.strip("/")
    if repository.casefold() != CANONICAL_REPOSITORY.casefold():
        raise ValueError(
            f"unexpected release source repository: {repository!r}; "
            f"expected {CANONICAL_REPOSITORY!r}"
        )
    return CANONICAL_REPOSITORY


def git_value(repo_root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if completed.returncode != 0:
        tail = completed.stderr.strip()[-800:]
        raise RuntimeError(f"git {' '.join(args)} failed: {tail}")
    return completed.stdout.strip()


def resolve_identity(repo_root: Path) -> dict[str, str | int]:
    root = repo_root.resolve()
    head = git_value(root, "rev-parse", "--verify", "HEAD")
    if not _SHA_RE.fullmatch(head):
        raise ValueError("checked-out HEAD is not an exact lowercase 40-character commit SHA")
    origin = git_value(root, "remote", "get-url", "origin")
    repository = normalize_github_origin(origin)
    return {
        "schema": "dokkomplekt.release-source-identity.v1",
        "source_repository": repository,
        "release_sha": head,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    identity = resolve_identity(args.repo_root)
    print(json.dumps(identity, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=__import__("sys").stderr)
        raise SystemExit(1)
