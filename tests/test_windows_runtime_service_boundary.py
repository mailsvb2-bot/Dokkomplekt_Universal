from __future__ import annotations

import json
from pathlib import Path

from scripts.release_environment_preflight import (
    WINDOWS_NETWORK_SERVICE_SID,
    validate_windows_runtime_service_boundary,
)


def write_fixture(tmp_path: Path, *, source_outside: bool = False) -> tuple[Path, Path, Path]:
    root = tmp_path / "runtime"
    root.mkdir()
    source = (tmp_path / "outside.bin") if source_outside else (root / "tool.bin")
    source.write_bytes(b"tool")
    license_file = root / "LICENSE.txt"
    license_file.write_text("license", encoding="utf-8")
    inventory = root / "inventory.json"
    inventory.write_text(json.dumps({"schema": 1, "tools": {"fixture": ["tool.bin"]}}), encoding="utf-8")
    manifest = root / "windows-x86_64-manifest.json"
    manifest.write_text(
        json.dumps(
            {
                "schema": 1,
                "target": "windows-x86_64",
                "supply_chain_locked": True,
                "files": [
                    {
                        "tool": "fixture",
                        "source": str(source),
                        "license_file": str(license_file),
                    }
                ],
                "distribution_review": {"inventory_file": str(inventory)},
            }
        ),
        encoding="utf-8",
    )
    Path(str(manifest) + ".sig").write_text("signature", encoding="utf-8")
    evidence = tmp_path / "RUNTIME_SERVICE_ACL.json"
    evidence.write_text(
        json.dumps(
            {
                "schema": "dokkomplekt.runtime-service-acl.v2",
                "runtime_root": str(root),
                "manifest_path": str(manifest),
                "service_sid": WINDOWS_NETWORK_SERVICE_SID,
                "access": "ReadAndExecute",
                "recursive_acl_applied": True,
            }
        ),
        encoding="utf-8",
    )
    return root, manifest, evidence


def test_runtime_service_boundary_accepts_network_service_session_zero_and_bounded_tree(tmp_path: Path) -> None:
    root, manifest, evidence = write_fixture(tmp_path)
    errors = validate_windows_runtime_service_boundary(
        manifest,
        root,
        evidence,
        current_sid=WINDOWS_NETWORK_SERVICE_SID,
        session_id=0,
    )
    assert errors == []


def test_runtime_service_boundary_rejects_interactive_or_wrong_identity(tmp_path: Path) -> None:
    root, manifest, evidence = write_fixture(tmp_path)
    errors = validate_windows_runtime_service_boundary(
        manifest,
        root,
        evidence,
        current_sid="S-1-5-18",
        session_id=3,
    )
    joined = "\n".join(errors)
    assert "must execute as Network Service SID" in joined
    assert "must execute in Windows Session 0" in joined


def test_runtime_service_boundary_rejects_manifest_references_outside_bounded_root(tmp_path: Path) -> None:
    root, manifest, evidence = write_fixture(tmp_path, source_outside=True)
    errors = validate_windows_runtime_service_boundary(
        manifest,
        root,
        evidence,
        current_sid=WINDOWS_NETWORK_SERVICE_SID,
        session_id=0,
    )
    assert any("files[0].source escapes fixed runtime root" in error for error in errors)


def test_runtime_service_boundary_rejects_stale_acl_evidence(tmp_path: Path) -> None:
    root, manifest, evidence = write_fixture(tmp_path)
    payload = json.loads(evidence.read_text(encoding="utf-8"))
    payload["service_sid"] = "S-1-5-18"
    evidence.write_text(json.dumps(payload), encoding="utf-8")
    errors = validate_windows_runtime_service_boundary(
        manifest,
        root,
        evidence,
        current_sid=WINDOWS_NETWORK_SERVICE_SID,
        session_id=0,
    )
    assert any("runtime ACL evidence SID mismatch" in error for error in errors)
