#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from typing import Any

SCHEMA = "dokkomplekt.private-hardware-dispatch.v1"


def now_utc() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


def iso(value: dt.datetime) -> str:
    return value.isoformat().replace("+00:00", "Z")


class GitHubApi:
    def __init__(self, token: str) -> None:
        self.token = token

    def request(self, method: str, url: str, payload: dict[str, Any] | None = None) -> Any:
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        request = urllib.request.Request(
            url,
            data=body,
            method=method,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "dokkomplekt-private-hardware-dispatch",
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                raw = response.read()
                return None if not raw else json.loads(raw.decode("utf-8"))
        except urllib.error.HTTPError as exc:
            details = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"GitHub API {method} {url} failed: HTTP {exc.code}: {details}") from exc

    def repository(self, full_name: str) -> dict[str, Any]:
        return dict(self.request("GET", f"https://api.github.com/repos/{full_name}") or {})

    def dispatch(self, repository: str, workflow: str, ref: str, inputs: dict[str, str]) -> None:
        encoded = urllib.parse.quote(workflow, safe="")
        self.request(
            "POST",
            f"https://api.github.com/repos/{repository}/actions/workflows/{encoded}/dispatches",
            {"ref": ref, "inputs": inputs},
        )

    def runs(self, repository: str, workflow: str, ref: str) -> list[dict[str, Any]]:
        encoded = urllib.parse.quote(workflow, safe="")
        branch = urllib.parse.quote(ref, safe="")
        data = self.request(
            "GET",
            f"https://api.github.com/repos/{repository}/actions/workflows/{encoded}/runs?event=workflow_dispatch&branch={branch}&per_page=50",
        )
        return list((data or {}).get("workflow_runs", []))


def parse_time(value: str) -> dt.datetime:
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def locate_run(
    api: GitHubApi,
    repository: str,
    workflow: str,
    ref: str,
    request_id: str,
    not_before: dt.datetime,
) -> dict[str, Any] | None:
    matches: list[dict[str, Any]] = []
    for run in api.runs(repository, workflow, ref):
        created = parse_time(str(run.get("created_at", "1970-01-01T00:00:00Z")))
        if created < not_before - dt.timedelta(seconds=10):
            continue
        display_title = str(run.get("display_title", ""))
        if request_id not in display_title:
            continue
        matches.append(run)
    if not matches:
        return None
    matches.sort(key=lambda item: parse_time(str(item["created_at"])), reverse=True)
    return matches[0]


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def validate_args(args: argparse.Namespace) -> None:
    if args.source_repository == args.target_repository:
        raise RuntimeError("hardware validation target must be a separate private repository")
    if "/" not in args.target_repository or args.target_repository.count("/") != 1:
        raise RuntimeError("target repository must be owner/name")
    if "/" not in args.source_repository or args.source_repository.count("/") != 1:
        raise RuntimeError("source repository must be owner/name")
    if len(args.release_sha) != 40 or any(ch not in "0123456789abcdef" for ch in args.release_sha):
        raise RuntimeError("release_sha must be an exact lowercase 40-character commit SHA")
    if args.reboot_phase not in {"prepare", "verify"}:
        raise RuntimeError("reboot_phase must be prepare or verify")


def main() -> int:
    parser = argparse.ArgumentParser(description="Dispatch Dokkomplekt hardware validation into a private repository")
    parser.add_argument("--source-repository", required=True)
    parser.add_argument("--target-repository", required=True)
    parser.add_argument("--workflow", default="windows-hardware-e2e.yml")
    parser.add_argument("--target-ref", default="main")
    parser.add_argument("--release-sha", required=True)
    parser.add_argument("--reboot-phase", choices=["prepare", "verify"], required=True)
    parser.add_argument("--token-env", default="DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN")
    parser.add_argument("--poll-seconds", type=int, default=20)
    parser.add_argument("--timeout-seconds", type=int, default=14400)
    parser.add_argument("--json-report", default="verification/release/PRIVATE_HARDWARE_DISPATCH.json")
    args = parser.parse_args()

    validate_args(args)
    token = os.environ.get(args.token_env, "").strip()
    if not token:
        raise RuntimeError(f"{args.token_env} is required")

    api = GitHubApi(token)
    target = api.repository(args.target_repository)
    if target.get("private") is not True:
        raise RuntimeError(
            f"refusing hardware dispatch because {args.target_repository} is not private; "
            "persistent production self-hosted runners must not be attached to the public source repository"
        )
    if bool(target.get("archived")):
        raise RuntimeError("hardware validation repository is archived")

    request_id = str(uuid.uuid4())
    started = now_utc()
    inputs = {
        "source_repository": args.source_repository,
        "release_sha": args.release_sha,
        "reboot_phase": args.reboot_phase,
        "request_id": request_id,
    }
    print(
        f"Dispatching private hardware validation: target={args.target_repository} "
        f"release={args.release_sha} phase={args.reboot_phase} request={request_id}",
        flush=True,
    )
    api.dispatch(args.target_repository, args.workflow, args.target_ref, inputs)

    deadline = time.monotonic() + args.timeout_seconds
    run: dict[str, Any] | None = None
    while time.monotonic() < deadline:
        candidate = locate_run(
            api,
            args.target_repository,
            args.workflow,
            args.target_ref,
            request_id,
            started,
        )
        if candidate is not None:
            run = candidate
            status = str(run.get("status", ""))
            print(
                f"Private hardware run #{run.get('run_number')} status={status} "
                f"conclusion={run.get('conclusion')}",
                flush=True,
            )
            if status == "completed":
                break
        time.sleep(args.poll_seconds)
    else:
        report = {
            "schema": SCHEMA,
            "created_at_utc": iso(now_utc()),
            "result": "failure",
            "failure": "timeout while waiting for private hardware workflow",
            "request_id": request_id,
            "source_repository": args.source_repository,
            "target_repository": args.target_repository,
            "release_sha": args.release_sha,
            "reboot_phase": args.reboot_phase,
        }
        write_report(Path(args.json_report), report)
        print(report["failure"], file=sys.stderr)
        return 1

    assert run is not None
    conclusion = str(run.get("conclusion", ""))
    report = {
        "schema": SCHEMA,
        "created_at_utc": iso(now_utc()),
        "result": "success" if conclusion == "success" else "failure",
        "request_id": request_id,
        "source_repository": args.source_repository,
        "target_repository": args.target_repository,
        "target_private": True,
        "release_sha": args.release_sha,
        "reboot_phase": args.reboot_phase,
        "workflow": args.workflow,
        "target_ref": args.target_ref,
        "run_id": run.get("id"),
        "run_number": run.get("run_number"),
        "run_url": run.get("html_url"),
        "status": run.get("status"),
        "conclusion": run.get("conclusion"),
        "display_title": run.get("display_title"),
    }
    write_report(Path(args.json_report), report)
    if conclusion != "success":
        print(f"private hardware validation failed: {run.get('html_url')}", file=sys.stderr)
        return 1
    print(f"PRIVATE HARDWARE VALIDATION PASS: {run.get('html_url')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
