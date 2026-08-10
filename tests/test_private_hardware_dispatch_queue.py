import argparse
import datetime as dt
import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "dispatch_private_hardware_validation.py"
PUBLIC_WORKFLOW = ROOT / ".github" / "workflows" / "windows-hardware-e2e.yml"
spec = importlib.util.spec_from_file_location("dispatch_private_hardware_validation", SCRIPT)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def args() -> argparse.Namespace:
    return argparse.Namespace(
        source_repository="mailsvb2-bot/Dokkomplekt_Universal",
        target_repository="mailsvb2-bot/Dokkomplekt_Hardware_Validation",
        workflow="windows-hardware-e2e.yml",
        target_ref="main",
        release_sha="a" * 40,
        reboot_phase="prepare",
        poll_seconds=20,
        queue_timeout_seconds=900,
        timeout_seconds=14400,
    )


def test_queued_duration_seconds_only_counts_queued_runs():
    now = dt.datetime(2026, 8, 9, 22, 30, tzinfo=dt.timezone.utc)
    run = {"status": "queued", "created_at": "2026-08-09T22:15:00Z"}

    assert module.queued_duration_seconds(run, now) == 900
    assert module.queued_duration_seconds({**run, "status": "in_progress"}, now) == 0


def test_run_report_exposes_private_run_identity():
    run = {
        "id": 123,
        "run_number": 7,
        "html_url": "https://github.com/example/private/actions/runs/123",
        "status": "queued",
        "conclusion": None,
        "display_title": "hardware request-id",
    }

    report = module.run_report(args(), "request-id", run, result="pending")

    assert report["result"] == "pending"
    assert report["run_id"] == 123
    assert report["run_url"].endswith("/123")
    assert report["status"] == "queued"


def test_validate_args_rejects_negative_queue_timeout():
    value = args()
    value.queue_timeout_seconds = -1

    try:
        module.validate_args(value)
    except RuntimeError as exc:
        assert "queue_timeout_seconds" in str(exc)
    else:
        raise AssertionError("negative queue timeout should fail")


def test_public_hardware_dispatch_serializes_prepare_and_verify():
    workflow = PUBLIC_WORKFLOW.read_text(encoding="utf-8")

    assert "group: windows-hardware-e2e\n" in workflow
    assert "group: windows-hardware-e2e-${{ inputs.reboot_phase }}" not in workflow
    assert "cancel-in-progress: false" in workflow
