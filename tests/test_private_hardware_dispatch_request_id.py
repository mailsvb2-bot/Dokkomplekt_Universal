from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sys
import uuid

import pytest

from source_helpers import project_text


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "dispatch_private_hardware_validation.py"
SPEC = importlib.util.spec_from_file_location("dispatch_private_hardware_validation", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
hardware_dispatch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(hardware_dispatch)


def dispatch_args(*, reboot_phase: str, request_id: str | None) -> argparse.Namespace:
    return argparse.Namespace(
        source_repository="mailsvb2-bot/Dokkomplekt_Universal",
        target_repository="mailsvb2-bot/Dokkomplekt_Hardware_Validation",
        release_sha="a" * 40,
        reboot_phase=reboot_phase,
        request_id=request_id,
        reuse_latest_prepare=False,
        poll_seconds=1,
        queue_timeout_seconds=0,
        timeout_seconds=1,
    )


def test_prepare_without_request_id_generates_canonical_uuid() -> None:
    args = dispatch_args(reboot_phase="prepare", request_id="")
    hardware_dispatch.validate_args(args)

    request_id = hardware_dispatch.resolve_request_id(args)

    assert str(uuid.UUID(request_id)) == request_id
    assert request_id == request_id.lower()


def test_verify_requires_request_id_from_prepare_phase() -> None:
    args = dispatch_args(reboot_phase="verify", request_id="")

    with pytest.raises(RuntimeError, match="verify phase requires request_id or --reuse-latest-prepare"):
        hardware_dispatch.validate_args(args)


def test_verify_reuses_exact_canonical_prepare_request_id() -> None:
    request_id = "01234567-89ab-4def-8123-456789abcdef"
    args = dispatch_args(reboot_phase="verify", request_id=request_id)

    hardware_dispatch.validate_args(args)

    assert hardware_dispatch.resolve_request_id(args) == request_id


def test_noncanonical_request_id_is_rejected() -> None:
    args = dispatch_args(
        reboot_phase="verify",
        request_id="0123456789ab4def8123456789abcdef",
    )

    with pytest.raises(RuntimeError, match="canonical lowercase UUID form"):
        hardware_dispatch.validate_args(args)


def test_public_hardware_workflow_forwards_and_surfaces_request_id() -> None:
    workflow = project_text(".github/workflows/windows-hardware-e2e.yml")

    assert "request_id:" in workflow
    assert "--request-id \"${{ inputs.request_id }}\"" in workflow
    assert "Reuse this exact request ID for the `verify` phase" in workflow


def test_latest_successful_prepare_request_is_reused_for_verify() -> None:
    class Api:
        def runs(self, repository, workflow, ref):
            return [
                {
                    "id": 101,
                    "status": "completed",
                    "conclusion": "success",
                    "created_at": "2026-09-13T07:00:00Z",
                    "display_title": "Dokkomplekt hardware " + "a" * 40 + " prepare 11111111-1111-4111-8111-111111111111",
                },
                {
                    "id": 202,
                    "status": "completed",
                    "conclusion": "success",
                    "created_at": "2026-09-13T08:00:00Z",
                    "display_title": "Dokkomplekt hardware " + "a" * 40 + " prepare 22222222-2222-4222-8222-222222222222",
                },
                {
                    "id": 303,
                    "status": "completed",
                    "conclusion": "failure",
                    "created_at": "2026-09-13T09:00:00Z",
                    "display_title": "Dokkomplekt hardware " + "a" * 40 + " prepare 33333333-3333-4333-8333-333333333333",
                },
            ]

    assert hardware_dispatch.successful_prepare_run(
        Api(), "owner/private", "windows-hardware-e2e.yml", "main", "a" * 40
    ) == ("22222222-2222-4222-8222-222222222222", 202)


def test_latest_prepare_lookup_fails_closed_when_none_succeeded() -> None:
    class Api:
        def runs(self, repository, workflow, ref):
            return []

    import pytest

    with pytest.raises(RuntimeError, match="no successful private prepare run"):
        hardware_dispatch.successful_prepare_run(
            Api(), "owner/private", "windows-hardware-e2e.yml", "main", "a" * 40
        )


def test_run_listing_paginates_beyond_first_page() -> None:
    api = hardware_dispatch.GitHubApi("test-token")
    calls: list[str] = []

    def request(method: str, url: str, payload=None):
        from urllib.parse import parse_qs, urlparse

        calls.append(url)
        page = int(parse_qs(urlparse(url).query)["page"][0])
        if page == 1:
            return {"workflow_runs": [{"id": index} for index in range(100)]}
        if page == 2:
            return {"workflow_runs": [{"id": 1000}]}
        raise AssertionError(url)

    api.request = request  # type: ignore[method-assign]
    runs = api.runs("owner/private", "windows-hardware-e2e.yml", "main")
    assert len(runs) == 101
    assert any("page=2" in url for url in calls)


def test_explicit_verify_resolves_matching_prepare_run_id() -> None:
    request_id = "01234567-89ab-4def-8123-456789abcdef"

    class Api:
        def runs(self, repository, workflow, ref):
            return [{
                "id": 4242,
                "status": "completed",
                "conclusion": "success",
                "created_at": "2026-09-13T08:00:00Z",
                "display_title": "Dokkomplekt hardware " + "a" * 40 + " prepare " + request_id,
            }]

    assert hardware_dispatch.successful_prepare_run(
        Api(), "owner/private", "windows-hardware-e2e.yml", "main", "a" * 40,
        required_request_id=request_id,
    ) == (request_id, 4242)


def test_successful_verify_lookup_is_bound_to_exact_prepare_request() -> None:
    request_id = "01234567-89ab-4def-8123-456789abcdef"

    class Api:
        def runs(self, repository, workflow, ref):
            return [
                {
                    "id": 5001,
                    "status": "completed",
                    "conclusion": "success",
                    "created_at": "2026-09-13T08:30:00Z",
                    "display_title": "Dokkomplekt hardware " + "a" * 40 + " verify " + request_id,
                    "html_url": "https://example.invalid/actions/runs/5001",
                },
                {
                    "id": 5002,
                    "status": "completed",
                    "conclusion": "success",
                    "created_at": "2026-09-13T09:00:00Z",
                    "display_title": "Dokkomplekt hardware " + "a" * 40 + " verify 11111111-1111-4111-8111-111111111111",
                },
            ]

    run = hardware_dispatch.successful_verify_run(
        Api(), "owner/private", "windows-hardware-e2e.yml", "main", "a" * 40, request_id
    )
    assert run is not None
    assert run["id"] == 5001


def test_main_reuses_successful_verify_without_dispatch(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    request_id = "01234567-89ab-4def-8123-456789abcdef"
    prepare = {
        "id": 4242,
        "status": "completed",
        "conclusion": "success",
        "created_at": "2026-09-13T08:00:00Z",
        "display_title": "Dokkomplekt hardware " + "a" * 40 + " prepare " + request_id,
        "html_url": "https://example.invalid/actions/runs/4242",
    }
    verify = {
        "id": 4343,
        "run_number": 99,
        "status": "completed",
        "conclusion": "success",
        "created_at": "2026-09-13T08:45:00Z",
        "display_title": "Dokkomplekt hardware " + "a" * 40 + " verify " + request_id,
        "html_url": "https://example.invalid/actions/runs/4343",
    }

    class FakeApi:
        dispatch_calls = []

        def __init__(self, token: str) -> None:
            assert token == "secret-token"

        def repository(self, full_name: str):
            return {"private": True, "archived": False}

        def runs(self, repository: str, workflow: str, ref: str):
            return [prepare, verify]

        def dispatch(self, repository: str, workflow: str, ref: str, inputs):
            self.dispatch_calls.append((repository, workflow, ref, inputs))

    report = tmp_path / "reuse.json"
    monkeypatch.setattr(hardware_dispatch, "GitHubApi", FakeApi)
    monkeypatch.setenv("DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN", "secret-token")
    monkeypatch.setattr(
        sys,
        "argv",
        [
            str(MODULE_PATH),
            "--source-repository",
            "mailsvb2-bot/Dokkomplekt_Universal",
            "--target-repository",
            "mailsvb2-bot/Dokkomplekt_Hardware_Validation",
            "--workflow",
            "windows-hardware-e2e.yml",
            "--target-ref",
            "main",
            "--release-sha",
            "a" * 40,
            "--reboot-phase",
            "verify",
            "--reuse-latest-prepare",
            "--json-report",
            str(report),
        ],
    )

    assert hardware_dispatch.main() == 0
    assert FakeApi.dispatch_calls == []
    payload = json.loads(report.read_text(encoding="utf-8"))
    assert payload["result"] == "success"
    assert payload["reused_existing_success"] is True
    assert payload["request_id"] == request_id
    assert payload["prepare_run_id"] == 4242
    assert payload["run_id"] == 4343
    assert payload["conclusion"] == "success"
