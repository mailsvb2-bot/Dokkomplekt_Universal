from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import assemble_performance_evidence as assemble
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
            "reference_id": "reference-machine-01",
            "corpus_id": "performance-corpus-01",
            "environment": {
                key: ("18.4.7" if key == "app_version" else f"bound-{key}")
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
            "coverage_sources": {
                "table_heavy": ["a" * 64],
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
                "cold_start_ms": 1000,
                "ui_response_ms": 50,
                "worker_count": 2,
                "queue_limit": 8,
            },
        },
    )


def observation(
    tmp_path: Path,
    reference_path: Path,
    name: str,
    *,
    run_id: str,
    trace_hash: str,
    warmup: bool = False,
    sample_ms: int = 100,
    derivation: str = "end_to_end_ms_minus_human_wait_ms",
    conditions: dict[str, object] | None = None,
    series_id: str = "series-button_to_ready",
    class_name: str = "typical_docx",
    cache_state: str = "warm_cache",
    run_kind: str = "single_document",
    trace_source_sha256: str | None = None,
) -> Path:
    if conditions is None:
        conditions = {"questions_present": False}
    return write_json(
        tmp_path / "observations" / f"{name}.json",
        {
            "schema": "dokkomplekt.performance-observation.v1",
            "claim": "single_sample_only_not_slo_verdict",
            "targets_sha256": sha256(TARGETS),
            "reference_policy_sha256": sha256(reference_path),
            "measurement_plan_sha256": "1" * 64,
            "trace_sha256": trace_hash,
            "reference_id": "reference-machine-01",
            "corpus_id": "performance-corpus-01",
            "series_id": series_id,
            "metric": "button_to_ready",
            "class": class_name,
            "cache_state": cache_state,
            "run_kind": run_kind,
            "conditions": conditions,
            "complexity": None,
            "warmup": warmup,
            "sample_ms": sample_ms,
            "sample_derivation": derivation,
            "trace_run_id": run_id,
            "trace_app_version": "18.4.7",
            "trace_source_sha256": trace_source_sha256,
            "trace_workload": "single_document",
            "trace_batch_size": 1,
            "trace_feature_flags": {
                "ocr_used": False,
                "runtime_layout_used": False,
                "pdf_used": False,
            },
        },
    )


def build(tmp_path: Path) -> tuple[dict, Path]:
    reference_path = reference(tmp_path)
    protocol_path = protocol(tmp_path)
    resources_path = resources(tmp_path, reference_path)
    result = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol_path,
        resources_path,
    )
    return result, reference_path


def test_assembles_typed_observations_and_excludes_warmup(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    protocol_path = protocol(tmp_path)
    resources_path = resources(tmp_path, reference_path)
    observation(
        tmp_path,
        reference_path,
        "warmup",
        run_id="run-warmup",
        trace_hash="a" * 64,
        warmup=True,
        sample_ms=999,
    )
    observation(
        tmp_path,
        reference_path,
        "sample",
        run_id="run-sample",
        trace_hash="b" * 64,
        sample_ms=120,
    )
    result = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol_path,
        resources_path,
    )
    assert result["schema"] == gate.EVIDENCE_SCHEMA
    assert result["claim"] == assemble.EVIDENCE_CLAIM
    assert result["series"][0]["warmup_runs"] == 1
    assert result["series"][0]["samples_ms"] == [120.0]
    assert len(result["source_observations"]) == 2
    assert result["targets_sha256"] == sha256(TARGETS)
    assert result["reference_policy_sha256"] == sha256(reference_path)


def test_assembler_accepts_non_bound_series_for_required_coverage(
    tmp_path: Path,
) -> None:
    reference_path = reference(tmp_path)
    observation(
        tmp_path,
        reference_path,
        "coverage",
        run_id="run-coverage",
        trace_hash="c" * 64,
        series_id="coverage-table-heavy-button",
        class_name="table_heavy",
        trace_source_sha256="a" * 64,
    )
    result = assemble.build_evidence(
        TARGETS,
        reference_path,
        tmp_path / "observations",
        protocol(tmp_path),
        resources(tmp_path, reference_path),
    )

    assert result["series"][0]["id"] == "coverage-table-heavy-button"
    assert result["series"][0]["class"] == "table_heavy"

    evidence_path = write_json(tmp_path / "coverage-evidence.json", result)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])


def test_assembler_rejects_coverage_source_not_predeclared_for_class(
    tmp_path: Path,
) -> None:
    reference_path = reference(tmp_path)
    observation(
        tmp_path,
        reference_path,
        "coverage-wrong-source",
        run_id="run-coverage-wrong",
        trace_hash="d" * 64,
        series_id="coverage-table-heavy-button",
        class_name="table_heavy",
        trace_source_sha256="b" * 64,
    )
    with pytest.raises(ValueError, match="not bound to a predeclared source"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resources(tmp_path, reference_path),
        )


def test_rejects_metric_derivation_tampering(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    observation(
        tmp_path,
        reference_path,
        "sample",
        run_id="run-a",
        trace_hash="a" * 64,
        derivation="per_run_stage_sum:replay+verify",
    )
    with pytest.raises(ValueError, match="sample derivation"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resources(tmp_path, reference_path),
        )


def test_rejects_one_trace_backing_multiple_observations(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    observation(
        tmp_path,
        reference_path,
        "a",
        run_id="run-a",
        trace_hash="a" * 64,
    )
    observation(
        tmp_path,
        reference_path,
        "b",
        run_id="run-b",
        trace_hash="a" * 64,
    )
    with pytest.raises(ValueError, match="trace_sha256 reused"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resources(tmp_path, reference_path),
        )


def test_rejects_series_metadata_drift(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    observation(
        tmp_path,
        reference_path,
        "a",
        run_id="run-a",
        trace_hash="a" * 64,
    )
    observation(
        tmp_path,
        reference_path,
        "b",
        run_id="run-b",
        trace_hash="b" * 64,
        conditions={"questions_present": False, "extra": True},
    )
    with pytest.raises(ValueError, match="series metadata drift"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resources(tmp_path, reference_path),
        )


def test_resource_snapshot_must_bind_exact_reference(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    resource_path = resources(tmp_path, reference_path)
    document = gate.load_object(resource_path)
    document["reference_policy_sha256"] = "0" * 64
    write_json(resource_path, document)
    observation(
        tmp_path,
        reference_path,
        "sample",
        run_id="run-a",
        trace_hash="a" * 64,
    )
    with pytest.raises(ValueError, match="exact reference policy"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            tmp_path / "observations",
            protocol(tmp_path),
            resource_path,
        )


def test_partial_trace_evidence_cannot_become_false_slo_pass(tmp_path: Path) -> None:
    reference_path = reference(tmp_path)
    for index, sample in enumerate((100, 120, 140), start=1):
        observation(
            tmp_path,
            reference_path,
            f"sample-{index}",
            run_id=f"run-{index}",
            trace_hash=f"{index:x}" * 64,
            sample_ms=sample,
        )
    result, _ = build(tmp_path)
    evidence_path = write_json(tmp_path / "evidence.json", result)
    verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
    assert verdict["result"] == "FAIL"
    assert any("bound series is missing" in error for error in verdict["errors"])
    assert any("coverage is incomplete" in error for error in verdict["errors"])
