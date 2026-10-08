from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import assemble_performance_evidence as assemble
from scripts import performance_layout_observation as layout
from scripts import performance_layout_probe as probe
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


def reference(tmp_path: Path, source_sha256: str) -> Path:
    targets = gate.load_object(TARGETS)
    return write_json(
        tmp_path / "reference.json",
        {
            "schema": gate.REFERENCE_SCHEMA,
            "status": "bound",
            "reference_id": "reference-layout-01",
            "corpus_id": "performance-layout-corpus-01",
            "environment": {
                key: ("18.4.7" if key == "app_version" else f"bound-{key}")
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
            "coverage_sources": {"runtime_layout": [source_sha256]},
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


def plan(tmp_path: Path, source_sha256: str) -> Path:
    return write_json(
        tmp_path / "layout-plan.json",
        {
            "schema": layout.PLAN_SCHEMA,
            "reference_id": "reference-layout-01",
            "corpus_id": "performance-layout-corpus-01",
            "series_id": "coverage-runtime-layout-button",
            "metric": "button_to_ready",
            "class": "runtime_layout",
            "cache_state": "warm_cache",
            "run_kind": "single_document",
            "conditions": {"runtime_layout": True},
            "complexity": None,
            "warmup": False,
            "expected_app_version": "18.4.7",
            "expected_source_sha256": source_sha256,
            "measurement_kind": layout.MEASUREMENT_KIND,
        },
    )


def proof(tmp_path: Path, source_sha256: str) -> Path:
    document = {
        "schema": probe.PROOF_SCHEMA,
        "producer": probe.PRODUCER,
        "claim": probe.CLAIM,
        "proof_id": "layout-proof-001",
        "application_sha256": "a" * 64,
        "app_version": "18.4.7",
        "source_sha256": source_sha256,
        "pdf_sha256": "b" * 64,
        "converter_sha256": "c" * 64,
        "converter_version": "LibreOffice 26.2.5.2",
        "conversion_duration_ms": 120.0,
        "application_elapsed_ms": 180.0,
        "layout_check_duration_ms": 225.0,
        "os": "Windows-10.0.14393-SP0",
        "font_set_sha256": "d" * 64,
        "font_file_count": 240,
        "visual_baseline_sha256": "e" * 64,
        "settings": {"dpi": 120, "dhash_size": 16, "tolerance": 32},
        "pages": [
            {
                "width": 1020,
                "height": 1320,
                "dhash16": "f" * 64,
                "distance": 3,
            }
        ],
        "verdict": "pass",
    }
    probe.validate_proof(document)
    return write_json(tmp_path / "layout-proofs" / "proof.json", document)


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


def test_layout_observation_requires_passed_hash_bound_layout_proof(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    reference_path = reference(tmp_path, source_sha256)
    proof_path = proof(tmp_path, source_sha256)
    observation = layout.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, source_sha256),
        proof_path,
    )
    assert observation["schema"] == layout.OBSERVATION_SCHEMA
    assert observation["class"] == "runtime_layout"
    assert observation["sample_ms"] == 225.0
    assert observation["sample_derivation"] == layout.SAMPLE_DERIVATION
    assert observation["layout_proof_sha256"] == sha256(proof_path)
    assert observation["source_sha256"] == source_sha256
    assert observation["visual_verdict"] == "pass"
    assert observation["series_id"] != "series-button_to_ready"


def test_layout_source_must_be_predeclared(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    reference_path = reference(tmp_path, source_sha256)
    plan_path = plan(tmp_path, source_sha256)
    document = gate.load_object(plan_path)
    document["expected_source_sha256"] = "2" * 64
    write_json(plan_path, document)
    with pytest.raises(ValueError, match="not predeclared"):
        layout.build_observation(
            TARGETS,
            reference_path,
            plan_path,
            proof(tmp_path, source_sha256),
        )


def test_layout_observation_rejects_failed_visual_proof(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    proof_path = proof(tmp_path, source_sha256)
    document = gate.load_object(proof_path)
    document["verdict"] = "fail"
    write_json(proof_path, document)
    with pytest.raises(ValueError, match="provenance/verdict"):
        layout.build_observation(
            TARGETS,
            reference(tmp_path, source_sha256),
            plan(tmp_path, source_sha256),
            proof_path,
        )


def test_assembler_accepts_layout_coverage_only_with_exact_proof(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    reference_path = reference(tmp_path, source_sha256)
    proof_path = proof(tmp_path, source_sha256)
    observation = layout.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, source_sha256),
        proof_path,
    )
    observations = tmp_path / "observations"
    write_json(observations / "layout.json", observation)

    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        observations,
        protocol(tmp_path),
        resources(tmp_path, reference_path),
        None,
        None,
        proof_path.parent,
    )
    assert evidence["series"][0]["class"] == "runtime_layout"
    assert evidence["series"][0]["id"] == "coverage-runtime-layout-button"
    assert evidence["source_observations"][0]["layout_proof_sha256"] == sha256(proof_path)

    evidence_path = write_json(tmp_path / "evidence.json", evidence)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])


def test_assembler_rejects_missing_layout_proof_artifact(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    reference_path = reference(tmp_path, source_sha256)
    proof_path = proof(tmp_path, source_sha256)
    observation = layout.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, source_sha256),
        proof_path,
    )
    observations = tmp_path / "observations"
    write_json(observations / "layout.json", observation)
    proof_path.unlink()

    with pytest.raises(ValueError, match="must resolve to exactly one file"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            None,
            None,
            proof_path.parent,
        )


def test_assembler_rejects_layout_proof_drift_after_observation(tmp_path: Path) -> None:
    source_sha256 = "1" * 64
    reference_path = reference(tmp_path, source_sha256)
    proof_path = proof(tmp_path, source_sha256)
    observation = layout.build_observation(
        TARGETS,
        reference_path,
        plan(tmp_path, source_sha256),
        proof_path,
    )
    observations = tmp_path / "observations"
    write_json(observations / "layout.json", observation)

    drifted = gate.load_object(proof_path)
    drifted["converter_version"] = "LibreOffice DRIFTED"
    write_json(proof_path, drifted)

    with pytest.raises(ValueError, match="must resolve to exactly one file"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            None,
            None,
            proof_path.parent,
        )
