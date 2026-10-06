from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import assemble_performance_evidence as assemble
from scripts import performance_physical_observation as physical
from scripts import performance_slo_gate as gate


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "performance" / "slo-targets.json"


def write_json(path: Path, value: object) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reference(tmp_path: Path) -> Path:
    targets = gate.load_object(TARGETS)
    return write_json(
        tmp_path / "reference.json",
        {
            "schema": gate.REFERENCE_SCHEMA,
            "status": "bound",
            "reference_id": "reference-machine-physical-01",
            "corpus_id": "performance-corpus-physical-01",
            "environment": {
                key: ("18.4.7" if key == "app_version" else f"bound-{key}")
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
            "resource_budgets": {
                "cpu_percent_peak": 95,
                "peak_rss_bytes": 2_000_000_000,
                "disk_write_bytes": 5_000_000_000,
                "staging_peak_bytes": 5_000_000_000,
                "cold_start_ms": 3000,
                "ui_response_ms": 100,
                "worker_count": 4,
                "queue_limit": 100,
            },
        },
    )


def startup_plan(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "startup-plan.json",
        {
            "schema": physical.PLAN_SCHEMA,
            "reference_id": "reference-machine-physical-01",
            "corpus_id": "performance-corpus-physical-01",
            "series_id": "series-cold_start_to_interactive_ui",
            "metric": "cold_start_to_interactive_ui",
            "class": "typical_docx",
            "cache_state": "cold_cache",
            "run_kind": "first_run",
            "conditions": {"installed_configuration": True},
            "complexity": None,
            "warmup": False,
            "expected_app_version": "18.4.7",
            "measurement_kind": "process_start_to_interactive_ui",
        },
    )


def startup_measurement(
    tmp_path: Path,
    *,
    measurement_id: str = "physical-startup-run-001",
    app_version: str = "18.4.7",
    measurement_kind: str = "process_start_to_interactive_ui",
    installed_build: bool = True,
    sample_ms: float = 900.0,
) -> Path:
    return write_json(
        tmp_path / f"{measurement_id}.json",
        {
            "schema": physical.MEASUREMENT_SCHEMA,
            "measurement_id": measurement_id,
            "app_version": app_version,
            "metric": "cold_start_to_interactive_ui",
            "measurement_kind": measurement_kind,
            "sample_ms": sample_ms,
            "conditions": {"installed_configuration": True},
            "instrument": "hardware_harness_monotonic_clock",
            "installed_build": installed_build,
        },
    )


def protocol(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "protocol.json",
        {
            "schema": assemble.PROTOCOL_SCHEMA,
            "flags": {flag: True for flag in gate.REQUIRED_PROTOCOL_FLAGS},
        },
    )


def resources(tmp_path: Path, reference_path: Path) -> Path:
    reference_doc = gate.load_object(reference_path)
    return write_json(
        tmp_path / "resources.json",
        {
            "schema": assemble.RESOURCE_SCHEMA,
            "targets_sha256": sha256(TARGETS),
            "reference_policy_sha256": sha256(reference_path),
            "reference_id": reference_doc["reference_id"],
            "corpus_id": reference_doc["corpus_id"],
            "environment": reference_doc["environment"],
            "resources": {
                "cpu_percent_peak": 50,
                "peak_rss_bytes": 200_000_000,
                "disk_write_bytes": 10_000,
                "staging_peak_bytes": 20_000,
                "cold_start_ms": 900,
                "ui_response_ms": 50,
                "worker_count": 2,
                "queue_limit": 8,
            },
        },
    )


def test_binds_installed_physical_startup_measurement(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    plan_path = startup_plan(tmp_path)
    measurement_path = startup_measurement(tmp_path)

    observation = physical.build_observation(
        TARGETS, reference_path, plan_path, measurement_path
    )

    assert observation["schema"] == physical.OBSERVATION_SCHEMA
    assert observation["claim"] == physical.OBSERVATION_CLAIM
    assert observation["metric"] == "cold_start_to_interactive_ui"
    assert observation["sample_ms"] == 900.0
    assert observation["installed_build"] is True
    assert observation["measurement_sha256"] == sha256(measurement_path)
    assert observation["sample_derivation"] == (
        "direct_physical_measurement:process_start_to_interactive_ui"
    )


def test_non_bound_physical_series_can_supply_corpus_coverage(
    tmp_path: Path,
) -> None:
    reference_path = reference(tmp_path)
    plan_path = startup_plan(tmp_path)
    plan = gate.load_object(plan_path)
    plan["series_id"] = "coverage-hr-kit-startup"
    plan["class"] = "hr_kit"
    plan["cache_state"] = "warm_cache"
    plan["run_kind"] = "repeat_run"
    write_json(plan_path, plan)

    observation = physical.build_observation(
        TARGETS,
        reference_path,
        plan_path,
        startup_measurement(tmp_path),
    )
    assert observation["series_id"] == "coverage-hr-kit-startup"
    assert observation["class"] == "hr_kit"

    observation_path = write_json(
        tmp_path / "observations" / "coverage-startup.json",
        observation,
    )
    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol(tmp_path),
        resources(tmp_path, reference_path),
    )
    assert evidence["series"][0]["id"] == "coverage-hr-kit-startup"
    assert evidence["source_observations"][0]["observation_sha256"] == sha256(
        observation_path
    )


def test_bound_physical_series_still_requires_canonical_slo_conditions(
    tmp_path: Path,
) -> None:
    reference_path = reference(tmp_path)
    plan_path = startup_plan(tmp_path)
    plan = gate.load_object(plan_path)
    plan["cache_state"] = "warm_cache"
    write_json(plan_path, plan)

    with pytest.raises(ValueError, match="requires cache_state=cold_cache"):
        physical.build_observation(
            TARGETS,
            reference_path,
            plan_path,
            startup_measurement(tmp_path),
        )


def test_rejects_non_installed_measurement(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    with pytest.raises(ValueError, match="installed build"):
        physical.build_observation(
            TARGETS,
            reference_path,
            startup_plan(tmp_path),
            startup_measurement(tmp_path, installed_build=False),
        )


def test_rejects_measurement_kind_drift(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    with pytest.raises(ValueError, match="kind does not match"):
        physical.build_observation(
            TARGETS,
            reference_path,
            startup_plan(tmp_path),
            startup_measurement(
                tmp_path,
                measurement_kind="visible_action_to_ui_response",
            ),
        )


def test_rejects_reference_app_version_drift(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    with pytest.raises(ValueError, match="app_version"):
        physical.build_observation(
            TARGETS,
            reference_path,
            startup_plan(tmp_path),
            startup_measurement(tmp_path, app_version="18.4.6"),
        )


def test_assembler_accepts_physical_observation_without_claiming_pass(
    tmp_path: Path,
) -> None:
    reference_path = reference(tmp_path)
    plan_path = startup_plan(tmp_path)
    measurement_path = startup_measurement(tmp_path)
    observation = physical.build_observation(
        TARGETS, reference_path, plan_path, measurement_path
    )
    observation_path = write_json(
        tmp_path / "observations" / "startup.json",
        observation,
    )
    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol(tmp_path),
        resources(tmp_path, reference_path),
    )

    assert evidence["series"][0]["metric"] == "cold_start_to_interactive_ui"
    assert evidence["series"][0]["samples_ms"] == [900.0]
    assert evidence["source_observations"][0]["measurement_sha256"] == sha256(
        measurement_path
    )
    assert evidence["source_observations"][0]["observation_sha256"] == sha256(
        observation_path
    )

    evidence_path = write_json(tmp_path / "evidence.json", evidence)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])


def test_assembler_rejects_reused_physical_measurement(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    observation = physical.build_observation(
        TARGETS,
        reference_path,
        startup_plan(tmp_path),
        startup_measurement(tmp_path),
    )
    write_json(tmp_path / "observations" / "a.json", observation)
    write_json(tmp_path / "observations" / "b.json", observation)

    with pytest.raises(ValueError, match="measurement_sha256 reused"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resources(tmp_path, reference_path),
        )
