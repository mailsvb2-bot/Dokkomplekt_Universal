from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "full_product_autopilot.py"
WORKFLOW = ROOT / ".github" / "workflows" / "full-product-autopilot.yml"
AUTHORITATIVE_HOSTED = (
    ROOT / ".github" / "workflows" / "quality-gate.yml",
    ROOT / ".github" / "workflows" / "source-provenance.yml",
    ROOT / ".github" / "workflows" / "macos-smoke.yml",
    ROOT / ".github" / "workflows" / "unsigned-preview.yml",
)


def load_autopilot_module():
    spec = importlib.util.spec_from_file_location("full_product_autopilot_reuse", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FakeApi:
    def __init__(self, runs):
        self._runs = list(runs)
        self.events: list[str | None] = []

    def runs(self, workflow: str, *, event: str | None = None):
        assert workflow == "quality-gate.yml"
        self.events.append(event)
        # Intentionally adversarial: return the mixed set even when the caller asks
        # for push runs, proving the selector itself fails closed on run.event.
        return list(self._runs)


def test_exact_sha_failed_push_run_remains_authoritative() -> None:
    module = load_autopilot_module()
    sha = "a" * 40
    api = FakeApi(
        [
            {
                "id": 101,
                "event": "push",
                "head_sha": sha,
                "head_branch": "main",
                "status": "completed",
                "conclusion": "failure",
                "run_number": 10,
                "created_at": "2026-08-12T12:00:00Z",
            },
            {
                "id": 102,
                "event": "workflow_dispatch",
                "head_sha": sha,
                "head_branch": "main",
                "status": "completed",
                "conclusion": "success",
                "run_number": 11,
                "created_at": "2026-08-12T12:01:00Z",
            },
            {
                "id": 103,
                "event": "push",
                "head_sha": "b" * 40,
                "head_branch": "main",
                "status": "completed",
                "conclusion": "success",
                "run_number": 12,
                "created_at": "2026-08-12T12:02:00Z",
            },
        ]
    )

    run = module.locate_existing_push_run(api, "quality-gate.yml", sha, "main")

    assert run is not None
    assert run["id"] == 101
    assert run["conclusion"] == "failure"
    assert api.events == ["push"]


def test_wrong_branch_push_run_is_not_reused() -> None:
    module = load_autopilot_module()
    sha = "c" * 40
    api = FakeApi(
        [
            {
                "id": 201,
                "event": "push",
                "head_sha": sha,
                "head_branch": "feature",
                "status": "completed",
                "conclusion": "success",
                "run_number": 20,
                "created_at": "2026-08-12T12:00:00Z",
            }
        ]
    )

    assert module.locate_existing_push_run(api, "quality-gate.yml", sha, "main") is None


def test_main_push_autopilot_reuses_existing_gates_but_manual_run_does_not() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    source = SCRIPT.read_text(encoding="utf-8")

    assert "--reuse-existing" in workflow
    assert "github.event_name == 'push' && '--reuse-existing' || ''" in workflow
    assert 'dispatch.add_argument(\n        "--reuse-existing"' in source
    assert 'state["source"] == "reused-push"' in source
    assert 'workflow in hosted_names and args.ref == "main"' in source
    assert 'locate_existing_push_run(api, workflow, args.sha, args.ref)' in source


def test_reuse_lookup_is_exact_sha_and_push_only() -> None:
    source = SCRIPT.read_text(encoding="utf-8")
    assert 'api.runs(workflow, event="push")' in source
    assert 'if run.get("event") != "push":' in source
    assert 'if run.get("head_sha") != sha:' in source
    assert 'if head_branch and head_branch != ref:' in source
    assert "never retry a failed" in source


def test_every_authoritative_main_push_gate_is_concurrency_isolated_by_sha() -> None:
    expression = "github.event_name == 'push' && github.sha || github.ref"
    for path in AUTHORITATIVE_HOSTED:
        workflow = path.read_text(encoding="utf-8")
        assert "cancel-in-progress: true" in workflow, path.name
        assert expression in workflow, path.name


def test_pr_updates_still_share_ref_concurrency_group() -> None:
    # The expression deliberately falls back to github.ref outside push events,
    # so stale commits within one pull request can still be superseded.
    for path in AUTHORITATIVE_HOSTED:
        workflow = path.read_text(encoding="utf-8")
        assert "|| github.ref" in workflow, path.name
