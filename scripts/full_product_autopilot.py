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
from typing import Any

SCHEMA = "dokkomplekt.full-product-autopilot.v1"
MANDATORY_FEATURES = {
    "install-windows",
    "install-linux",
    "macos-bundle",
    "first-launch",
    "first-run-template-picker",
    "button-creation",
    "restart-persistence",
    "source-intake",
    "template-intelligence",
    "popup-workflow",
    "single-generation",
    "batch-generation",
    "mail-merge",
    "diary-generation",
    "scanner",
    "docx-oracle",
    "visual-golden",
    "synthetic-corpus",
    "watcher",
    "exactly-once",
    "crash-recovery",
    "source-finalization-identity",
    "template-versioning",
    "workspace-publication",
    "license-trial",
    "postgres-concurrency",
    "offline-runtime",
    "ocr-image-pdf",
    "malformed-hostile-inputs",
    "update-rollback",
    "source-provenance",
    "uninstall",
    "windows-gui-console-free",
    "word-print",
    "reboot-watcher",
    "authenticode",
    "installed-post-reboot-output",
}
HOSTED_WORKFLOWS = [
    ("quality-gate.yml", {}),
    ("source-provenance.yml", {}),
    ("macos-smoke.yml", {}),
    ("unsigned-preview.yml", {}),
]
HARDWARE_WORKFLOW = ("windows-hardware-e2e.yml", "release_sha")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def load_matrix(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("feature matrix root must be an object")
    return data


def validate_matrix(root: Path, matrix_path: Path) -> dict[str, Any]:
    data = load_matrix(matrix_path)
    errors: list[str] = []
    if data.get("schema") != SCHEMA:
        errors.append(f"schema must be {SCHEMA}")
    features = data.get("features")
    if not isinstance(features, list):
        errors.append("features must be an array")
        features = []

    seen: set[str] = set()
    categories: dict[str, int] = {}
    scopes: dict[str, int] = {}
    levels: dict[str, int] = {}
    missing_evidence: dict[str, list[str]] = {}
    manual_features: list[str] = []

    for index, feature in enumerate(features):
        if not isinstance(feature, dict):
            errors.append(f"features[{index}] must be an object")
            continue
        feature_id = str(feature.get("id", "")).strip()
        if not feature_id:
            errors.append(f"features[{index}] has no id")
            continue
        if feature_id in seen:
            errors.append(f"duplicate feature id: {feature_id}")
        seen.add(feature_id)

        category = str(feature.get("category", "")).strip()
        scope = str(feature.get("scope", "")).strip()
        level = str(feature.get("level", "")).strip()
        if not category:
            errors.append(f"{feature_id}: category is required")
        if scope not in {"software", "production-hardware"}:
            errors.append(f"{feature_id}: invalid scope {scope!r}")
        if not level:
            errors.append(f"{feature_id}: level is required")
        categories[category] = categories.get(category, 0) + 1
        scopes[scope] = scopes.get(scope, 0) + 1
        levels[level] = levels.get(level, 0) + 1

        if feature.get("automated") is not True:
            manual_features.append(feature_id)

        evidence = feature.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            errors.append(f"{feature_id}: evidence must be a non-empty array")
            continue
        absent: list[str] = []
        for item in evidence:
            rel = str(item).strip()
            if not rel:
                absent.append("<empty>")
                continue
            if not (root / rel).exists():
                absent.append(rel)
        if absent:
            missing_evidence[feature_id] = absent

    absent_features = sorted(MANDATORY_FEATURES - seen)
    unknown_features = sorted(seen - MANDATORY_FEATURES)
    if absent_features:
        errors.append("mandatory features missing: " + ", ".join(absent_features))
    if unknown_features:
        errors.append("unregistered feature ids: " + ", ".join(unknown_features))
    if manual_features:
        errors.append("manual-only features are forbidden: " + ", ".join(sorted(manual_features)))
    if missing_evidence:
        for feature_id, paths in sorted(missing_evidence.items()):
            errors.append(f"{feature_id}: missing evidence: {', '.join(paths)}")
    if scopes.get("software", 0) < 25:
        errors.append("software coverage unexpectedly small")
    if scopes.get("production-hardware", 0) < 5:
        errors.append("production-hardware coverage unexpectedly small")

    return {
        "schema": SCHEMA,
        "created_at_utc": utc_now(),
        "matrix": str(matrix_path),
        "feature_count": len(seen),
        "mandatory_feature_count": len(MANDATORY_FEATURES),
        "categories": dict(sorted(categories.items())),
        "scopes": dict(sorted(scopes.items())),
        "levels": dict(sorted(levels.items())),
        "manual_features": sorted(manual_features),
        "missing_evidence": missing_evidence,
        "errors": errors,
        "valid": not errors,
    }


def markdown_validation(report: dict[str, Any]) -> str:
    status = "PASS" if report["valid"] else "FAIL"
    lines = [
        "# Dokkomplekt Full Product Autopilot — coverage contract",
        "",
        f"**Result:** {status}",
        f"**Features:** {report['feature_count']} / {report['mandatory_feature_count']}",
        "",
        "## Scope coverage",
        "",
        "| Scope | Features |",
        "|---|---:|",
    ]
    for key, value in report["scopes"].items():
        lines.append(f"| {key} | {value} |")
    lines.extend(["", "## Automation levels", "", "| Level | Features |", "|---|---:|"])
    for key, value in report["levels"].items():
        lines.append(f"| {key} | {value} |")
    if report["errors"]:
        lines.extend(["", "## Errors", ""])
        lines.extend(f"- {item}" for item in report["errors"])
    return "\n".join(lines) + "\n"


class GitHubApi:
    def __init__(self, token: str, repository: str) -> None:
        self.token = token
        self.repository = repository
        self.base = f"https://api.github.com/repos/{repository}"

    def request(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
        body = None if payload is None else json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            self.base + path,
            data=body,
            method=method,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "dokkomplekt-full-product-autopilot",
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=30) as response:
                raw = response.read()
                return None if not raw else json.loads(raw.decode("utf-8"))
        except urllib.error.HTTPError as exc:
            details = exc.read().decode("utf-8", errors="replace")
            raise RuntimeError(
                f"GitHub API {method} {path} failed: HTTP {exc.code}: {details}"
            ) from exc

    def dispatch(self, workflow: str, ref: str, inputs: dict[str, str]) -> None:
        payload: dict[str, Any] = {"ref": ref}
        if inputs:
            payload["inputs"] = inputs
        encoded = urllib.parse.quote(workflow, safe="")
        self.request("POST", f"/actions/workflows/{encoded}/dispatches", payload)

    def runs(self, workflow: str, *, event: str | None = None) -> list[dict[str, Any]]:
        encoded = urllib.parse.quote(workflow, safe="")
        query = {"per_page": "100"}
        if event:
            query["event"] = event
        data = self.request(
            "GET",
            f"/actions/workflows/{encoded}/runs?{urllib.parse.urlencode(query)}",
        )
        return list((data or {}).get("workflow_runs", []))


def parse_github_time(value: str) -> dt.datetime:
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def _newest_run(candidates: list[dict[str, Any]]) -> dict[str, Any] | None:
    if not candidates:
        return None
    candidates.sort(
        key=lambda item: parse_github_time(str(item["created_at"])),
        reverse=True,
    )
    return candidates[0]


def locate_dispatched_run(
    api: GitHubApi,
    workflow: str,
    sha: str,
    not_before: dt.datetime,
) -> dict[str, Any] | None:
    candidates: list[dict[str, Any]] = []
    for run in api.runs(workflow, event="workflow_dispatch"):
        if run.get("head_sha") != sha:
            continue
        created = parse_github_time(str(run.get("created_at")))
        if created >= not_before - dt.timedelta(seconds=10):
            candidates.append(run)
    return _newest_run(candidates)


def locate_existing_push_run(
    api: GitHubApi,
    workflow: str,
    sha: str,
    ref: str,
) -> dict[str, Any] | None:
    """Return the authoritative exact-SHA push run without retrying away failures."""
    candidates: list[dict[str, Any]] = []
    for run in api.runs(workflow, event="push"):
        if run.get("head_sha") != sha:
            continue
        head_branch = str(run.get("head_branch") or "")
        if head_branch and head_branch != ref:
            continue
        candidates.append(run)
    return _newest_run(candidates)


def write_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def markdown_dispatch(report: dict[str, Any]) -> str:
    result = report["result"]
    icon = "✅" if result.endswith("PASS") else "❌"
    lines = [
        "# FULL DOKKOMPLEKT AUTOPILOT",
        "",
        f"## {icon} {result}",
        "",
        f"- Scope: `{report['scope']}`",
        f"- Commit: `{report['sha']}`",
        f"- Feature coverage contract: **{'PASS' if report['coverage']['valid'] else 'FAIL'}**",
        f"- Registered capabilities: **{report['coverage']['feature_count']}**",
        "",
        "## Executed workflows",
        "",
        "| Workflow | Source | Conclusion | Run |",
        "|---|---|---|---|",
    ]
    for item in report["workflows"]:
        url = item.get("html_url") or ""
        run = f"[#{item.get('run_number')}]({url})" if url else "-"
        source = item.get("source") or "-"
        lines.append(
            f"| {item['workflow']} | {source} | "
            f"**{item.get('conclusion', item.get('status'))}** | {run} |"
        )
    lines.extend(["", "## Interpretation", ""])
    if report["scope"] == "software":
        lines.append(
            "Hosted CI, real PostgreSQL, browser E2E, Windows installed-app smoke, "
            "Linux packaging and macOS bundle checks were required. Physical "
            "Word/printer/reboot/production Authenticode acceptance was intentionally "
            "not claimed; run scope `production-hardware` on protected `main` for that boundary."
        )
    else:
        lines.append(
            "Hosted checks and the dedicated production Windows hardware gate were all "
            "required, including real Word, printer spooler completion, reboot watcher "
            "evidence and Authenticode evidence."
        )
    if report["failures"]:
        lines.extend(["", "## Failures", ""])
        lines.extend(f"- {item}" for item in report["failures"])
    return "\n".join(lines) + "\n"


def _failure_report(
    args: argparse.Namespace,
    coverage: dict[str, Any],
    failures: list[str],
) -> int:
    result = {
        "schema": SCHEMA,
        "created_at_utc": utc_now(),
        "result": "AUTOPILOT FAIL",
        "scope": args.scope,
        "repository": getattr(args, "repository", None),
        "ref": getattr(args, "ref", None),
        "sha": getattr(args, "sha", None),
        "coverage": coverage,
        "workflows": [],
        "failures": failures,
    }
    write_json(Path(args.json_report), result)
    write_text(Path(args.markdown_report), markdown_dispatch(result))
    return 1


def _refresh_run(
    api: GitHubApi,
    workflow: str,
    state: dict[str, Any],
    sha: str,
    ref: str,
) -> dict[str, Any] | None:
    if state["source"] == "reused-push":
        return locate_existing_push_run(api, workflow, sha, ref)
    return locate_dispatched_run(api, workflow, sha, state["requested_at"])


def dispatch_and_wait(args: argparse.Namespace) -> int:
    root = Path(args.repo_root).resolve()
    matrix_path = (root / args.matrix).resolve()
    coverage = validate_matrix(root, matrix_path)
    if not coverage["valid"]:
        return _failure_report(args, coverage, list(coverage["errors"]))

    if args.scope == "production-hardware" and args.ref != "main":
        return _failure_report(
            args,
            coverage,
            ["production-hardware scope is allowed only from protected main"],
        )

    token = os.environ.get(args.token_env, "").strip()
    if not token:
        raise RuntimeError(f"{args.token_env} is required")
    api = GitHubApi(token, args.repository)

    requested: list[tuple[str, dict[str, str]]] = list(HOSTED_WORKFLOWS)
    if args.scope == "production-hardware":
        requested.append((HARDWARE_WORKFLOW[0], {HARDWARE_WORKFLOW[1]: args.sha}))

    dispatch_records: dict[str, dict[str, Any]] = {}
    hosted_names = {workflow for workflow, _ in HOSTED_WORKFLOWS}
    for workflow, inputs in requested:
        reusable = args.reuse_existing and workflow in hosted_names and args.ref == "main"
        existing = (
            locate_existing_push_run(api, workflow, args.sha, args.ref)
            if reusable
            else None
        )
        if existing is not None:
            print(
                f"Reusing exact-SHA push {workflow} run #{existing.get('run_number')} "
                f"for {args.sha}",
                flush=True,
            )
            dispatch_records[workflow] = {
                "requested_at": None,
                "run": existing,
                "source": "reused-push",
            }
            continue

        started = dt.datetime.now(dt.timezone.utc)
        print(f"Dispatching {workflow} for {args.sha}", flush=True)
        api.dispatch(workflow, args.ref, inputs)
        dispatch_records[workflow] = {
            "requested_at": started,
            "run": None,
            "source": "dispatched",
        }

    deadline = time.monotonic() + args.timeout_seconds
    failures: list[str] = []
    while time.monotonic() < deadline:
        all_complete = True
        for workflow, state in dispatch_records.items():
            run = state["run"]
            if run is None or run.get("status") != "completed":
                refreshed = _refresh_run(api, workflow, state, args.sha, args.ref)
                if refreshed is not None:
                    state["run"] = refreshed
                    run = refreshed
                    if run.get("status") == "completed":
                        print(
                            f"Completed {workflow} run #{run.get('run_number')}: "
                            f"{run.get('conclusion')}",
                            flush=True,
                        )
                if run is None or run.get("status") != "completed":
                    all_complete = False
        if all_complete:
            break
        time.sleep(args.poll_seconds)
    else:
        failures.append("timeout while waiting for authoritative workflows")

    workflows: list[dict[str, Any]] = []
    for workflow, state in dispatch_records.items():
        run = state["run"]
        if run is None:
            workflows.append(
                {
                    "workflow": workflow,
                    "source": state["source"],
                    "status": "not-found",
                    "conclusion": "failure",
                }
            )
            failures.append(f"{workflow}: authoritative run was not found")
            continue
        item = {
            "workflow": workflow,
            "source": state["source"],
            "id": run.get("id"),
            "run_number": run.get("run_number"),
            "status": run.get("status"),
            "conclusion": run.get("conclusion"),
            "html_url": run.get("html_url"),
            "head_sha": run.get("head_sha"),
            "event": run.get("event"),
        }
        workflows.append(item)
        if run.get("head_sha") != args.sha:
            failures.append(f"{workflow}: head SHA changed during verification")
        if run.get("status") != "completed" or run.get("conclusion") != "success":
            failures.append(f"{workflow}: {run.get('status')}/{run.get('conclusion')}")

    if failures:
        result_name = "AUTOPILOT FAIL"
    elif args.scope == "production-hardware":
        result_name = "FULL PRODUCTION PASS"
    else:
        result_name = "SOFTWARE PASS"

    result = {
        "schema": SCHEMA,
        "created_at_utc": utc_now(),
        "result": result_name,
        "scope": args.scope,
        "repository": args.repository,
        "ref": args.ref,
        "sha": args.sha,
        "coverage": coverage,
        "workflows": workflows,
        "failures": failures,
    }
    write_json(Path(args.json_report), result)
    write_text(Path(args.markdown_report), markdown_dispatch(result))
    return 0 if not failures else 1


def validate_only(args: argparse.Namespace) -> int:
    root = Path(args.repo_root).resolve()
    report = validate_matrix(root, (root / args.matrix).resolve())
    write_json(Path(args.json_report), report)
    write_text(Path(args.markdown_report), markdown_validation(report))
    if report["errors"]:
        for error in report["errors"]:
            print(f"ERROR: {error}", file=sys.stderr)
    else:
        print(f"Autopilot coverage contract OK: {report['feature_count']} capabilities")
    return 0 if report["valid"] else 1


def parser() -> argparse.ArgumentParser:
    top = argparse.ArgumentParser(
        description="Dokkomplekt full-product automated verification orchestrator"
    )
    sub = top.add_subparsers(dest="command", required=True)

    validate = sub.add_parser("validate", help="validate the capability/evidence registry")
    validate.add_argument("--repo-root", default=".")
    validate.add_argument("--matrix", default="verification/autopilot/feature-matrix.json")
    validate.add_argument("--json-report", default="verification/autopilot/coverage-report.json")
    validate.add_argument(
        "--markdown-report",
        default="verification/autopilot/coverage-report.md",
    )
    validate.set_defaults(func=validate_only)

    dispatch = sub.add_parser(
        "dispatch",
        help="dispatch and aggregate the complete CI/hardware contour",
    )
    dispatch.add_argument("--repo-root", default=".")
    dispatch.add_argument("--matrix", default="verification/autopilot/feature-matrix.json")
    dispatch.add_argument("--repository", required=True)
    dispatch.add_argument("--ref", required=True)
    dispatch.add_argument("--sha", required=True)
    dispatch.add_argument(
        "--scope",
        choices=["software", "production-hardware"],
        required=True,
    )
    dispatch.add_argument(
        "--reuse-existing",
        action="store_true",
        help=(
            "reuse exact-SHA main push runs for hosted gates; never retry a failed "
            "authoritative push run"
        ),
    )
    dispatch.add_argument("--token-env", default="GH_TOKEN")
    dispatch.add_argument("--poll-seconds", type=int, default=20)
    dispatch.add_argument("--timeout-seconds", type=int, default=12600)
    dispatch.add_argument(
        "--json-report",
        default="verification/autopilot/FULL_AUTOPILOT_REPORT.json",
    )
    dispatch.add_argument(
        "--markdown-report",
        default="verification/autopilot/FULL_AUTOPILOT_REPORT.md",
    )
    dispatch.set_defaults(func=dispatch_and_wait)
    return top


def main() -> int:
    args = parser().parse_args()
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
