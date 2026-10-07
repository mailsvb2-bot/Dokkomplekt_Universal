from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import assemble_performance_evidence as assemble
from scripts import performance_slo_gate as gate
from scripts import performance_storage_observation as storage


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
            "reference_id": "reference-storage-01",
            "corpus_id": "performance-storage-corpus-01",
            "environment": {
                key: ("18.4.7" if key == "app_version" else f"bound-{key}")
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
            "coverage_sources": {
                "slow_storage": ["a" * 64],
                "network_storage": ["b" * 64],
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


def plan(tmp_path: Path, class_name: str) -> Path:
    source = "a" * 64 if class_name == "slow_storage" else "b" * 64
    return write_json(
        tmp_path / f"{class_name}-plan.json",
        {
            "schema": storage.PLAN_SCHEMA,
            "reference_id": "reference-storage-01",
            "corpus_id": "performance-storage-corpus-01",
            "series_id": f"coverage-{class_name}-button",
            "metric": "button_to_ready",
            "class": class_name,
            "cache_state": "warm_cache",
            "run_kind": "single_document",
            "conditions": {"questions_present": False},
            "complexity": None,
            "warmup": False,
            "expected_app_version": "18.4.7",
            "expected_source_sha256": source,
            "measurement_kind": storage.MEASUREMENT_KIND,
        },
    )


def measurement(
    tmp_path: Path,
    class_name: str,
    *,
    verification_method: str | None = None,
) -> Path:
    source = "a" * 64 if class_name == "slow_storage" else "b" * 64
    expected_method = storage.SUPPORTED_CLASSES[class_name]
    return write_json(
        tmp_path / f"{class_name}-measurement.json",
        {
            "schema": storage.MEASUREMENT_SCHEMA,
            "measurement_id": f"{class_name}-run-001",
            "app_version": "18.4.7",
            "metric": "button_to_ready",
            "measurement_kind": storage.MEASUREMENT_KIND,
            "sample_ms": 240.0,
            "conditions": {"questions_present": False},
            "instrument": "hardware_harness_monotonic_clock",
            "installed_build": True,
            "source_sha256": source,
            "storage_class": class_name,
            "verification_method": verification_method or expected_method,
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
    ref = gate.load_object(reference_path)
    return write_json(
        tmp_path / "resources.json",
        {
            "schema": assemble.RESOURCE_SCHEMA,
            "targets_sha256": sha256(TARGETS),
            "reference_policy_sha256": sha256(reference_path),
            "reference_id": ref["reference_id"],
            "corpus_id": ref["corpus_id"],
            "environment": ref["environment"],
            "resources": {
                "cpu_percent_peak": 50,
                "peak_rss_bytes": 200_000_000,
                "disk_write_bytes": 10_000,
                "staging_peak_bytes": 20_000,
                "cold_start_ms": 1000,
                "ui_response_ms": 50,
                "worker_count": 2,
                "queue_limit": 8,
            },
        },
    )


@pytest.mark.parametrize("class_name", ["slow_storage", "network_storage"])
def test_storage_observation_is_typed_and_coverage_only(
    tmp_path: Path, class_name: str
) -> None:
    reference_path = reference(tmp_path)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, class_name),
        measurement(tmp_path, class_name),
    )

    assert observation["schema"] == storage.OBSERVATION_SCHEMA
    assert observation["claim"] == storage.OBSERVATION_CLAIM
    assert observation["class"] == class_name
    assert observation["storage_class"] == class_name
    assert observation["sample_derivation"] == "direct_storage_condition_measurement"
    assert observation["series_id"] != "series-button_to_ready"


def test_network_storage_requires_unc_verification_method(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    with pytest.raises(ValueError, match="windows_unc_path_probe"):
        storage.build_observation(
            TARGETS,
            reference_path,
            plan(tmp_path, "network_storage"),
            measurement(
                tmp_path,
                "network_storage",
                verification_method="controlled_io_throttle_probe",
            ),
        )


def test_storage_source_must_be_predeclared_for_class(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    plan_path = plan(tmp_path, "slow_storage")
    document = gate.load_object(plan_path)
    document["expected_source_sha256"] = "c" * 64
    write_json(plan_path, document)
    with pytest.raises(ValueError, match="not predeclared"):
        storage.build_observation(
            TARGETS,
            reference_path,
            plan_path,
            measurement(tmp_path, "slow_storage"),
        )


def test_assembler_accepts_storage_coverage_without_binding_slo(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, "slow_storage"),
        measurement(tmp_path, "slow_storage"),
    )
    write_json(tmp_path / "observations" / "slow.json", observation)

    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol(tmp_path),
        resources(tmp_path, reference_path),
    )
    assert evidence["series"][0]["class"] == "slow_storage"
    assert evidence["series"][0]["id"] == "coverage-slow_storage-button"

    evidence_path = write_json(tmp_path / "evidence.json", evidence)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])
