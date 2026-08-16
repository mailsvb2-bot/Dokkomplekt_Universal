from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path
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

    with pytest.raises(RuntimeError, match="verify phase requires request_id from the prepare phase"):
        hardware_dispatch.validate_args(args)


def test_verify_reuses_exact_canonical_prepare_request_id() -> None:
    request_id = "01234567-89ab-4def-8123-456789abcdef"
    args = dispatch_args(reboot_phase="verify", request_id=request_id)

    hardware_dispatch.validate_args(args)

    assert hardware_dispatch.resolve_request_id(args) == request_id


def test_noncanonical_request_id_is_rejected() -> None:
    args = dispatch_args(
        reboot_phase="verify",
        request_id="01234567-89AB-4DEF-8123-456789ABCDEF",
    )

    with pytest.raises(RuntimeError, match="canonical lowercase UUID form"):
        hardware_dispatch.validate_args(args)


def test_public_hardware_workflow_forwards_and_surfaces_request_id() -> None:
    workflow = project_text(".github/workflows/windows-hardware-e2e.yml")

    assert "request_id:" in workflow
    assert "--request-id \"${{ inputs.request_id }}\"" in workflow
    assert "Reuse this exact request ID for the `verify` phase" in workflow
