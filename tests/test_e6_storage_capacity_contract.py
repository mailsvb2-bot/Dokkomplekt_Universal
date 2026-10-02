from __future__ import annotations

import json

from source_helpers import project_text


def test_manual_batch_capacity_preflight_happens_before_license_and_stage() -> None:
    commands = project_text("src-tauri/src/subsystems/document_commands.rs")
    body = commands[commands.index("fn render_docx_batch("): commands.index("#[derive(Debug, Deserialize)]\nstruct ScannerRequest")]

    capacity = body.index("ensure_manual_batch_storage_capacity(")
    reserve = body.index("reserve_generation_access(")
    stage = body.index('let stage = stage_parent.join(format!(')
    assert capacity < reserve < stage
    assert "template_sizes" in body
    assert "RetainedUploadedSource::byte_len" in body


def test_capacity_budget_is_cross_platform_and_fail_closed() -> None:
    privacy = project_text("src-tauri/src/privacy_runtime.rs")
    root_cargo = project_text("Cargo.toml")
    tauri_cargo = project_text("src-tauri/Cargo.toml")

    assert 'fs2 = "0.4.3"' in root_cargo
    assert "fs2.workspace = true" in tauri_cargo
    assert "fs2::available_space(stage_parent)" in privacy
    assert "MANUAL_BATCH_MIN_RECOVERY_RESERVE_BYTES" in privacy
    assert "transient_render_bytes" in privacy
    assert "retained_source_bytes" in privacy
    assert "Недостаточно свободного места для безопасного создания комплекта" in privacy
    assert "Комплект не создаётся без проверки диска" in privacy
    assert "manual_batch_storage_gate_fails_closed_below_required_capacity" in privacy


def test_capacity_preflight_is_registered_as_e6_resource_evidence() -> None:
    matrix = json.loads(project_text("verification/autopilot/feature-matrix.json"))
    feature = next(item for item in matrix["features"] if item["id"] == "technical-storage-visibility")
    assert "src-tauri/src/subsystems/document_commands.rs" in feature["evidence"]
    assert "src-tauri/src/universal_intake.rs" in feature["evidence"]
    assert "tests/test_e6_storage_capacity_contract.py" in feature["evidence"]
