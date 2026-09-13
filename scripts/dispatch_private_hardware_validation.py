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
PRESTART_STATUSES = frozenset({"pending", "queued", "requested", "waiting"})


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
        results: list[dict[str, Any]] = []
        per_page = 100
        for page in range(1, 101):
            data = self.request(
                "GET",
                f"https://api.github.com/repos/{repository}/actions/workflows/{encoded}/runs"
                f"?event=workflow_dispatch&branch={branch}&per_page={per_page}&page={page}",
            )
            batch = list((data or {}).get("workflow_runs", []))
            results.extend(batch)
            if len(batch) < per_page:
                return results
        raise RuntimeError("private hardware workflow run pagination exceeded 100 pages")

    def cancel_run(self, repository: str, run_id: int) -> None:
        self.request("POST", f"https://api.github.com/repos/{repository}/actions/runs/{run_id}/cancel")


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


def prestart_duration_seconds(run: dict[str, Any], current: dt.datetime) -> int:
    status = str(run.get("status", "")).strip().lower()
    if status not in PRESTART_STATUSES:
        return 0
    created_raw = str(run.get("created_at", "")).strip()
    if not created_raw:
        return 0
    created = parse_time(created_raw)
    return max(0, int((current - created).total_seconds()))


def cancel_private_run(
    api: GitHubApi,
    repository: str,
    run: dict[str, Any] | None,
) -> tuple[bool, str | None]:
    if run is None or str(run.get("status", "")) == "completed":
        return False, None
    try:
        run_id = int(run.get("id"))
    except (TypeError, ValueError):
        return False, "private hardware run has no numeric id; cancellation could not be requested"
    try:
        api.cancel_run(repository, run_id)
    except RuntimeError as exc:
        return False, str(exc)
    return True, None


def run_report(
    args: argparse.Namespace,
    request_id: str,
    run: dict[str, Any] | None,
    *,
    result: str,
    failure: str | None = None,
) -> dict[str, Any]:
    report: dict[str, Any] = {
        "schema": SCHEMA,
        "created_at_utc": iso(now_utc()),
        "result": result,
        "request_id": request_id,
        "source_repository": args.source_repository,
        "target_repository": args.target_repository,
        "target_private": True,
        "release_sha": args.release_sha,
        "reboot_phase": args.reboot_phase,
        "workflow": args.workflow,
        "target_ref": args.target_ref,
    }
    prepare_run_id = getattr(args, "prepare_run_id", None)
    if prepare_run_id is not None:
        report["prepare_run_id"] = prepare_run_id
    if run is not None:
        report.update(
            {
                "run_id": run.get("id"),
                "run_number": run.get("run_number"),
                "run_url": run.get("html_url"),
                "status": run.get("status"),
                "conclusion": run.get("conclusion"),
                "display_title": run.get("display_title"),
            }
        )
    if failure:
        report["failure"] = failure
    return report


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_failure_with_cancellation(
    api: GitHubApi,
    args: argparse.Namespace,
    request_id: str,
    run: dict[str, Any] | None,
    report_path: Path,
    failure: str,
) -> None:
    report = run_report(args, request_id, run, result="failure", failure=failure)
    cancel_requested, cancel_failure = cancel_private_run(api, args.target_repository, run)
    report["cancel_requested"] = cancel_requested
    if cancel_failure:
        report["cancel_failure"] = cancel_failure
    write_report(report_path, report)
    print(failure, file=sys.stderr)
    if cancel_failure:
        print(f"private hardware cancellation failed: {cancel_failure}", file=sys.stderr)


def canonical_request_id(value: str) -> str:
    normalized = value.strip().lower()
    try:
        parsed = uuid.UUID(normalized)
    except ValueError as exc:
        raise RuntimeError(f"request_id must be a canonical UUID: {exc}") from exc
    if str(parsed) != normalized:
        raise RuntimeError("request_id must use canonical lowercase UUID form")
    return normalized


def find_successful_phase_run(
    api: GitHubApi,
    repository: str,
    workflow: str,
    ref: str,
    release_sha: str,
    phase: str,
    *,
    required_request_id: str | None = None,
) -> tuple[str, dict[str, Any]] | None:
    if phase not in {"prepare", "verify"}:
        raise RuntimeError("phase must be prepare or verify")
    prefix = f"Dokkomplekt hardware {release_sha} {phase} "
    candidates: list[tuple[dt.datetime, str, dict[str, Any]]] = []
    for run in api.runs(repository, workflow, ref):
        if str(run.get("status", "")).strip().lower() != "completed":
            continue
        if str(run.get("conclusion", "")).strip().lower() != "success":
            continue
        title = str(run.get("display_title", "")).strip()
        if not title.startswith(prefix):
            continue
        raw_request_id = title[len(prefix) :].strip()
        try:
            request_id = canonical_request_id(raw_request_id)
            int(run.get("id"))
        except (RuntimeError, TypeError, ValueError):
            continue
        if required_request_id is not None and request_id != required_request_id:
            continue
        created = parse_time(str(run.get("created_at", "1970-01-01T00:00:00Z")))
        candidates.append((created, request_id, run))
    if not candidates:
        return None
    candidates.sort(key=lambda item: item[0], reverse=True)
    _, request_id, run = candidates[0]
    return request_id, run


def successful_prepare_run(
    api: GitHubApi,
    repository: str,
    workflow: str,
    ref: str,
    release_sha: str,
    *,
    required_request_id: str | None = None,
) -> tuple[str, int]:
    match = find_successful_phase_run(
        api,
        repository,
        workflow,
        ref,
        release_sha,
        "prepare",
        required_request_id=required_request_id,
    )
    if match is None:
        suffix = "" if required_request_id is None else f" and request_id={required_request_id}"
        raise RuntimeError(
            "no successful private prepare run exists for this exact release_sha" + suffix + "; "
            "run the public Windows Hardware E2E workflow with reboot_phase=prepare, "
            "perform the real Windows reboot/logon, then retry the release workflow"
        )
    request_id, run = match
    return request_id, int(run["id"])


def successful_verify_run(
    api: GitHubApi,
    repository: str,
    workflow: str,
    ref: str,
    release_sha: str,
    request_id: str,
) -> dict[str, Any] | None:
    match = find_successful_phase_run(
        api,
        repository,
        workflow,
        ref,
        release_sha,
        "verify",
        required_request_id=request_id,
    )
    return None if match is None else match[1]


def resolve_request_id(args: argparse.Namespace) -> str:
    provided = str(args.request_id or "").strip()
    if provided:
        return canonical_request_id(provided)
    if args.reboot_phase == "verify":
        if args.reuse_latest_prepare:
            return ""
        raise RuntimeError(
            "verify phase requires --request-id from the successful prepare phase, "
            "or --reuse-latest-prepare to resolve the latest successful prepare for this release_sha"
        )
    if args.reuse_latest_prepare:
        raise RuntimeError("--reuse-latest-prepare is valid only with reboot_phase=verify")
    return str(uuid.uuid4())


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
    if args.poll_seconds <= 0:
        raise RuntimeError("poll_seconds must be positive")
    if args.timeout_seconds <= 0:
        raise RuntimeError("timeout_seconds must be positive")
    if args.queue_timeout_seconds < 0:
        raise RuntimeError("queue_timeout_seconds must be zero or positive")
    if str(args.request_id or "").strip():
        canonical_request_id(str(args.request_id))
        if args.reuse_latest_prepare:
            raise RuntimeError("--request-id and --reuse-latest-prepare are mutually exclusive")
    elif args.reboot_phase == "verify" and not args.reuse_latest_prepare:
        raise RuntimeError("verify phase requires request_id or --reuse-latest-prepare")
    elif args.reboot_phase == "prepare" and args.reuse_latest_prepare:
        raise RuntimeError("--reuse-latest-prepare is valid only for verify")


def main() -> int:
    parser = argparse.ArgumentParser(description="Dispatch Dokkomplekt hardware validation into a private repository")
    parser.add_argument("--source-repository", required=True)
    parser.add_argument("--target-repository", required=True)
    parser.add_argument("--workflow", default="windows-hardware-e2e.yml")
    parser.add_argument("--target-ref", default="main")
    parser.add_argument("--release-sha", required=True)
    parser.add_argument("--reboot-phase", choices=["prepare", "verify"], required=True)
    parser.add_argument(
        "--request-id",
        help="Correlation UUID. Omit for prepare to generate one; verify must reuse the prepare UUID.",
    )
    parser.add_argument(
        "--reuse-latest-prepare",
        action="store_true",
        help="For verify, resolve the newest successful private prepare request UUID for the exact release SHA.",
    )
    parser.add_argument("--token-env", default="DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN")
    parser.add_argument("--poll-seconds", type=int, default=20)
    parser.add_argument("--queue-timeout-seconds", type=int, default=900)
    parser.add_argument("--timeout-seconds", type=int, default=14400)
    parser.add_argument("--json-report", default="verification/release/PRIVATE_HARDWARE_DISPATCH.json")
    args = parser.parse_args()

    validate_args(args)
    request_id = resolve_request_id(args)
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
    args.prepare_run_id = None
    if args.reboot_phase == "verify":
        required_request_id = None if args.reuse_latest_prepare else request_id
        request_id, args.prepare_run_id = successful_prepare_run(
            api,
            args.target_repository,
            args.workflow,
            args.target_ref,
            args.release_sha,
            required_request_id=required_request_id,
        )
        print(
            f"Resolved successful private prepare correlation: release={args.release_sha} "
            f"request={request_id} prepare_run_id={args.prepare_run_id}",
            flush=True,
        )
        existing_verify = successful_verify_run(
            api,
            args.target_repository,
            args.workflow,
            args.target_ref,
            args.release_sha,
            request_id,
        )
        if existing_verify is not None:
            report_path = Path(args.json_report)
            report = run_report(args, request_id, existing_verify, result="success")
            report["reused_existing_success"] = True
            write_report(report_path, report)
            print(
                "PRIVATE HARDWARE VALIDATION REUSED: "
                f"{existing_verify.get('html_url') or existing_verify.get('id')}",
                flush=True,
            )
            return 0

    started = now_utc()
    inputs = {
        "source_repository": args.source_repository,
        "release_sha": args.release_sha,
        "reboot_phase": args.reboot_phase,
        "request_id": request_id,
        "prepare_run_id": "" if args.prepare_run_id is None else str(args.prepare_run_id),
    }
    report_path = Path(args.json_report)
    print(
        f"Dispatching private hardware validation: target={args.target_repository} "
        f"release={args.release_sha} phase={args.reboot_phase} request={request_id}",
        flush=True,
    )
    api.dispatch(args.target_repository, args.workflow, args.target_ref, inputs)
    write_report(report_path, run_report(args, request_id, None, result="dispatched"))

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
            status = str(run.get("status", "")).strip().lower()
            prestart_seconds = prestart_duration_seconds(run, now_utc())
            print(
                f"Private hardware run #{run.get('run_number')} id={run.get('id')} "
                f"status={status} conclusion={run.get('conclusion')} url={run.get('html_url')}",
                flush=True,
            )
            write_report(report_path, run_report(args, request_id, run, result="pending"))
            if status == "completed":
                break
            if (
                status in PRESTART_STATUSES
                and args.queue_timeout_seconds > 0
                and prestart_seconds >= args.queue_timeout_seconds
            ):
                failure = (
                    f"private hardware workflow remained in pre-start status {status!r} "
                    f"for {prestart_seconds}s; verify that the private dokkomplekt-hardware Windows runner is online and registered, "
                    "the protected private workflow environments are approved, and no concurrency gate "
                    "is blocking the private workflow"
                )
                write_failure_with_cancellation(api, args, request_id, run, report_path, failure)
                return 1
        time.sleep(args.poll_seconds)
    else:
        failure = "timeout while waiting for private hardware workflow"
        write_failure_with_cancellation(api, args, request_id, run, report_path, failure)
        return 1

    assert run is not None
    conclusion = str(run.get("conclusion", ""))
    final_result = "success" if conclusion == "success" else "failure"
    failure = None if conclusion == "success" else f"private hardware validation concluded {conclusion or 'without a conclusion'}"
    report = run_report(args, request_id, run, result=final_result, failure=failure)
    write_report(report_path, report)
    if conclusion != "success":
        print(f"private hardware validation failed: {run.get('html_url')}", file=sys.stderr)
        return 1
    print(f"PRIVATE HARDWARE VALIDATION PASS: {run.get('html_url')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
