from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import assemble_performance_evidence as assemble
from scripts import performance_slo_gate as gate
from scripts import performance_storage_observation as storage
from scripts import performance_storage_probe as probe


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


def reference(tmp_path: Path, class_name: str, source_sha256: str) -> Path:
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
            "coverage_sources": {class_name: [source_sha256]},
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


def plan(tmp_path: Path, class_name: str, source_sha256: str) -> Path:
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
            "expected_source_sha256": source_sha256,
            "measurement_kind": storage.MEASUREMENT_KIND,
        },
    )


def measurement(tmp_path: Path, class_name: str, source_sha256: str) -> Path:
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
            "source_sha256": source_sha256,
        },
    )


def slow_probe(tmp_path: Path) -> tuple[Path, str]:
    source = tmp_path / "source.bin"
    source.write_bytes(b"x" * 8192)
    document = probe._slow_probe(source, throttle_bytes_per_sec=1_000_000)
    probe.validate_probe(document)
    return write_json(tmp_path / "slow-probe.json", document), document["source_sha256"]


def network_probe(tmp_path: Path, source_sha256: str) -> Path:
    document = {
        "schema": probe.SCHEMA,
        "producer": probe.PRODUCER,
        "storage_class": "network_storage",
        "source_sha256": source_sha256,
        "byte_count": 8192,
        "duration_ms": 10.0,
        "observed_bytes_per_sec": 819200.0,
        "verification": {
            "path_kind": "unc",
            "windows_drive_type": "remote",
        },
    }
    probe.validate_probe(document)
    return write_json(tmp_path / "network-probe.json", document)


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


def test_unc_root_requires_server_and_share() -> None:
    assert probe._unc_root(r"\\\\server\\share\\folder\\file.docx") == "\\\\server\\share\\"
    for malformed in (r"\\\\server", "\\\\server\\", r"\\\\\\share"):
        with pytest.raises(ValueError, match="server and share"):
            probe._unc_root(malformed)


def test_slow_probe_performs_controlled_read_and_is_hash_bound(tmp_path: Path) -> None:
    probe_path, source_sha256 = slow_probe(tmp_path)
    document = gate.load_object(probe_path)
    assert document["producer"] == probe.PRODUCER
    assert document["storage_class"] == "slow_storage"
    assert document["source_sha256"] == source_sha256
    assert document["verification"]["mode"] == "controlled_read_throttle"
    assert document["observed_bytes_per_sec"] <= (
        document["verification"]["configured_bytes_per_sec"] * 1.10
    )


@pytest.mark.parametrize("class_name", ["slow_storage", "network_storage"])
def test_storage_observation_is_probe_bound_and_coverage_only(
    tmp_path: Path, class_name: str
) -> None:
    if class_name == "slow_storage":
        probe_path, source_sha256 = slow_probe(tmp_path)
    else:
        source_sha256 = "b" * 64
        probe_path = network_probe(tmp_path, source_sha256)
    reference_path = reference(tmp_path, class_name, source_sha256)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, class_name, source_sha256),
        measurement(tmp_path, class_name, source_sha256),
        probe_path,
    )

    assert observation["schema"] == storage.OBSERVATION_SCHEMA
    assert observation["claim"] == storage.OBSERVATION_CLAIM
    assert observation["class"] == class_name
    assert observation["storage_class"] == class_name
    assert observation["probe_sha256"] == sha256(probe_path)
    assert observation["sample_derivation"] == "direct_storage_condition_measurement"
    assert observation["series_id"] != "series-button_to_ready"


def test_rejects_forged_network_probe_verification(tmp_path: Path) -> None:
    source_sha256 = "b" * 64
    probe_path = network_probe(tmp_path, source_sha256)
    document = gate.load_object(probe_path)
    document["verification"] = {
        "mode": "controlled_read_throttle",
        "configured_bytes_per_sec": 1000,
    }
    write_json(probe_path, document)
    reference_path = reference(tmp_path, "network_storage", source_sha256)
    with pytest.raises(ValueError, match="UNC/DRIVE_REMOTE"):
        storage.build_observation(
            TARGETS,
            reference_path,
            plan(tmp_path, "network_storage", source_sha256),
            measurement(tmp_path, "network_storage", source_sha256),
            probe_path,
        )


def test_storage_source_must_be_predeclared_for_class(tmp_path: Path) -> None:
    probe_path, source_sha256 = slow_probe(tmp_path)
    reference_path = reference(tmp_path, "slow_storage", source_sha256)
    plan_path = plan(tmp_path, "slow_storage", source_sha256)
    document = gate.load_object(plan_path)
    document["expected_source_sha256"] = "c" * 64
    write_json(plan_path, document)
    with pytest.raises(ValueError, match="not predeclared"):
        storage.build_observation(
            TARGETS,
            reference_path,
            plan_path,
            measurement(tmp_path, "slow_storage", source_sha256),
            probe_path,
        )


def test_assembler_accepts_storage_coverage_without_binding_slo(tmp_path: Path) -> None:
    probe_path, source_sha256 = slow_probe(tmp_path)
    reference_path = reference(tmp_path, "slow_storage", source_sha256)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, "slow_storage", source_sha256),
        measurement(tmp_path, "slow_storage", source_sha256),
        probe_path,
    )
    write_json(tmp_path / "observations" / "slow.json", observation)

    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol(tmp_path),
        resources(tmp_path, reference_path),
        tmp_path,
    )
    assert evidence["series"][0]["class"] == "slow_storage"
    assert evidence["series"][0]["id"] == "coverage-slow_storage-button"
    assert evidence["source_observations"][0]["probe_sha256"] == sha256(probe_path)

    evidence_path = write_json(tmp_path / "evidence.json", evidence)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])


def test_assembler_rejects_storage_observation_without_exact_probe_artifact(
    tmp_path: Path,
) -> None:
    probe_path, source_sha256 = slow_probe(tmp_path)
    reference_path = reference(tmp_path, "slow_storage", source_sha256)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, "slow_storage", source_sha256),
        measurement(tmp_path, "slow_storage", source_sha256),
        probe_path,
    )
    observations_dir = tmp_path / "observations"
    write_json(observations_dir / "slow.json", observation)
    probe_path.unlink()

    with pytest.raises(ValueError, match="must resolve to exactly one file"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations_dir,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            tmp_path,
        )


def test_assembler_rejects_probe_artifact_that_differs_from_observation(
    tmp_path: Path,
) -> None:
    probe_path, source_sha256 = slow_probe(tmp_path)
    reference_path = reference(tmp_path, "slow_storage", source_sha256)
    observation = storage.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, "slow_storage", source_sha256),
        measurement(tmp_path, "slow_storage", source_sha256),
        probe_path,
    )
    observations_dir = tmp_path / "observations"
    write_json(observations_dir / "slow.json", observation)

    forged = gate.load_object(probe_path)
    forged["verification"]["configured_bytes_per_sec"] += 1
    write_json(probe_path, forged)

    with pytest.raises(ValueError, match="must resolve to exactly one file"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations_dir,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            tmp_path,
        )
