from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys

import pytest
from docx import Document

from scripts import assemble_performance_evidence as assemble
from scripts import performance_benchmark_harness as harness
from scripts import performance_slo_gate as gate
from scripts import performance_trace_observation as observation


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "performance" / "slo-targets.json"
SPEC = ROOT / "performance" / "corpus-spec.json"


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
            "reference_id": "benchmark-reference-01",
            "corpus_id": "dokkomplekt-e6-synthetic-v1",
            "corpus_spec_sha256": sha256(SPEC),
            "environment": {
                key: ("18.4.7" if key == "app_version" else f"bound-{key}")
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
            "coverage_sources": {},
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


def plan(tmp_path: Path, source_sha256: str, run_kind: str) -> Path:
    return write_json(
        tmp_path / f"{run_kind}-plan.json",
        {
            "schema": observation.PLAN_SCHEMA,
            "reference_id": "benchmark-reference-01",
            "corpus_id": "dokkomplekt-e6-synthetic-v1",
            "series_id": "series-source_analysis",
            "metric": "source_analysis",
            "class": "typical_docx",
            "cache_state": "cold_cache" if run_kind == "first_run" else "warm_cache",
            "run_kind": run_kind,
            "conditions": {"ocr": False},
            "complexity": None,
            "warmup": False,
            "expected_app_version": "18.4.7",
            "expected_source_sha256": source_sha256,
            "expected_workload": "single_document",
            "expected_ocr_used": False,
            "expected_runtime_layout_used": False,
            "expected_pdf_used": False,
        },
    )


def trace_document(source_sha256: str, run_id: str) -> dict[str, object]:
    stages = [
        ("source_open", 2),
        ("source_parse", 3),
        ("candidate_index", 4),
        ("source_resolve", 5),
        ("publish", 2),
    ]
    return {
        "schema": observation.TRACE_SCHEMA,
        "context": {
            "run_id": run_id,
            "app_version": "18.4.7",
            "source_sha256": source_sha256,
            "class": "unclassified",
            "cache_state": "unclassified",
            "run_phase": "unclassified",
            "workload": "single_document",
            "batch_size": 1,
            "ocr_used": False,
            "runtime_layout_used": False,
            "pdf_used": False,
        },
        "stages": [
            {"stage": stage, "duration_ms": duration}
            for stage, duration in stages
        ],
        "total_machine_ms": sum(duration for _, duration in stages),
        "end_to_end_ms": 20,
        "human_wait_ms": 0,
        "outcome": "completed",
    }


def command_files(
    tmp_path: Path, trace: dict[str, object], suffix: str
) -> tuple[Path, Path]:
    cache = write_json(
        tmp_path / f"cache-{suffix}.json",
        {
            "schema": harness.COMMAND_SCHEMA,
            "argv": [
                sys.executable,
                "-c",
                "from pathlib import Path; Path(r'{source}').read_bytes()",
            ],
        },
    )
    encoded = json.dumps(trace, ensure_ascii=False, sort_keys=True)
    measurement = write_json(
        tmp_path / f"measurement-{suffix}.json",
        {
            "schema": harness.COMMAND_SCHEMA,
            "argv": [
                sys.executable,
                "-c",
                "from pathlib import Path; "
                + "Path(r'{trace}').write_text("
                + repr(encoded)
                + ", encoding='utf-8')",
            ],
        },
    )
    return cache, measurement


def test_repository_corpus_spec_is_closed_and_canonical() -> None:
    document = gate.load_object(SPEC)
    harness.validate_corpus_spec(document)
    assert set(document["classes"]) == set(gate.CANONICAL_CORPUS_CLASSES)
    assert document["classes"]["batch_10"]["documents"] == 10
    assert document["classes"]["batch_50"]["documents"] == 50


def test_docx_fixture_materialization_is_byte_deterministic(tmp_path: Path) -> None:
    profile = {
        "documents": 1,
        "paragraphs": 3,
        "paragraph_bytes": 96,
        "tables": 1,
        "rows": 2,
        "cols": 2,
        "header_footer": True,
        "roles": 2,
    }
    first = tmp_path / "first.docx"
    second = tmp_path / "second.docx"
    harness._materialize_document(first, "small_docx", 1, profile)
    harness._materialize_document(second, "small_docx", 1, profile)
    assert sha256(first) == sha256(second)
    assert first.read_bytes().startswith(b"PK")


def test_repeated_blocks_fixture_contains_actual_repeated_payloads(tmp_path: Path) -> None:
    profile = gate.load_object(SPEC)["classes"]["repeated_blocks"]
    target = tmp_path / "repeated.docx"
    harness._materialize_document(target, "repeated_blocks", 1, profile)
    document = Document(target)
    payloads = [paragraph.text for paragraph in document.paragraphs if paragraph.text]
    body = [value for value in payloads if not value.startswith("E6 repeated_blocks")]
    assert len(body) > 16
    assert len(set(body)) < len(body)


def test_benchmark_rejects_alternate_spec_even_with_same_corpus_id(tmp_path: Path) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    plan_path = plan(tmp_path, source_sha256, "first_run")
    alternate = gate.load_object(SPEC)
    alternate["classes"]["typical_docx"]["paragraphs"] += 1
    alternate_spec = write_json(tmp_path / "alternate-spec.json", alternate)
    trace = trace_document(source_sha256, "alternate-spec-run")
    cache_command, measurement_command = command_files(tmp_path, trace, "alternate")

    with pytest.raises(ValueError, match="corpus spec hash differs"):
        harness.run_benchmark(
            spec_path=alternate_spec,
            plan_path=plan_path,
            reference_path=reference(tmp_path),
            source_path=source,
            trace_path=tmp_path / "alternate-trace.json",
            cache_command_path=cache_command,
            measurement_command_path=measurement_command,
            session_dir=tmp_path / "sessions",
            session_id="alternate-spec",
        )


def test_first_run_context_can_bind_unclassified_production_trace(tmp_path: Path) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    plan_path = plan(tmp_path, source_sha256, "first_run")
    trace = trace_document(source_sha256, "first-run-001")
    cache_command, measurement_command = command_files(tmp_path, trace, "first")
    trace_path = tmp_path / "trace-first.json"

    context = harness.run_benchmark(
        spec_path=SPEC,
        plan_path=plan_path,
        reference_path=reference(tmp_path),
        source_path=source,
        trace_path=trace_path,
        cache_command_path=cache_command,
        measurement_command_path=measurement_command,
        session_dir=tmp_path / "sessions",
        session_id="session-01",
    )
    assert context["cache_state"] == "cold_cache"
    assert context["cache_action"] == "cold_reset"
    assert context["run_phase"] == "first_run"
    assert context["prior_trace_sha256"] is None

    context_path = write_json(tmp_path / "contexts" / "first.json", context)
    result = observation.build_observation(
        TARGETS,
        reference(tmp_path),
        plan_path,
        trace_path,
        context_path,
    )
    assert result["benchmark_context_sha256"] == sha256(context_path)
    assert result["cache_state"] == "cold_cache"
    assert result["run_kind"] == "first_run"

    observations_dir = tmp_path / "observations"
    write_json(observations_dir / "first.json", result)
    reference_path = reference(tmp_path)
    evidence = assemble.build_evidence(
        TARGETS,
        reference_path,
        observations_dir,
        protocol(tmp_path),
        resources(tmp_path, reference_path),
        None,
        context_path.parent,
    )
    assert evidence["source_observations"][0]["benchmark_context_sha256"] == sha256(
        context_path
    )


def test_repeat_run_requires_prior_completed_session_and_binds_prior_trace(
    tmp_path: Path,
) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    session_dir = tmp_path / "sessions"

    first_plan = plan(tmp_path, source_sha256, "first_run")
    first_trace = trace_document(source_sha256, "first-run-001")
    first_cache, first_measurement = command_files(tmp_path, first_trace, "first")
    first_trace_path = tmp_path / "trace-first.json"
    first = harness.run_benchmark(
        spec_path=SPEC,
        plan_path=first_plan,
        reference_path=reference(tmp_path),
        source_path=source,
        trace_path=first_trace_path,
        cache_command_path=first_cache,
        measurement_command_path=first_measurement,
        session_dir=session_dir,
        session_id="session-01",
    )

    repeat_plan = plan(tmp_path, source_sha256, "repeat_run")
    repeat_trace = trace_document(source_sha256, "repeat-run-001")
    repeat_cache, repeat_measurement = command_files(tmp_path, repeat_trace, "repeat")
    repeat_trace_path = tmp_path / "trace-repeat.json"
    repeat = harness.run_benchmark(
        spec_path=SPEC,
        plan_path=repeat_plan,
        reference_path=reference(tmp_path),
        source_path=source,
        trace_path=repeat_trace_path,
        cache_command_path=repeat_cache,
        measurement_command_path=repeat_measurement,
        session_dir=session_dir,
        session_id="session-01",
    )
    assert repeat["cache_state"] == "warm_cache"
    assert repeat["cache_action"] == "warm_prime"
    assert repeat["run_phase"] == "repeat_run"
    assert repeat["prior_trace_sha256"] == first["trace_sha256"]


def test_repeat_run_without_prior_session_fails_closed(tmp_path: Path) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    repeat_plan = plan(tmp_path, source_sha256, "repeat_run")
    repeat_trace = trace_document(source_sha256, "repeat-run-001")
    cache, measurement = command_files(tmp_path, repeat_trace, "repeat")

    with pytest.raises(ValueError, match="requires a completed prior measurement"):
        harness.run_benchmark(
            spec_path=SPEC,
            plan_path=repeat_plan,
            reference_path=reference(tmp_path),
            source_path=source,
            trace_path=tmp_path / "trace.json",
            cache_command_path=cache,
            measurement_command_path=measurement,
            session_dir=tmp_path / "sessions",
            session_id="session-01",
        )


def test_assembler_rejects_missing_benchmark_context_artifact(tmp_path: Path) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    plan_path = plan(tmp_path, source_sha256, "first_run")
    trace = trace_document(source_sha256, "first-run-missing-context")
    cache_command, measurement_command = command_files(tmp_path, trace, "missing")
    trace_path = tmp_path / "trace-missing.json"
    context = harness.run_benchmark(
        spec_path=SPEC,
        plan_path=plan_path,
        reference_path=reference(tmp_path),
        source_path=source,
        trace_path=trace_path,
        cache_command_path=cache_command,
        measurement_command_path=measurement_command,
        session_dir=tmp_path / "sessions",
        session_id="session-missing",
    )
    context_path = write_json(tmp_path / "contexts" / "context.json", context)
    reference_path = reference(tmp_path)
    result = observation.build_observation(
        TARGETS, reference_path, plan_path, trace_path, context_path
    )
    observations_dir = tmp_path / "observations"
    write_json(observations_dir / "sample.json", result)
    context_path.unlink()

    with pytest.raises(ValueError, match="must resolve to exactly one file"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations_dir,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            None,
            tmp_path / "contexts",
        )



def test_assembler_revalidates_forged_repeat_context(tmp_path: Path) -> None:
    source = tmp_path / "source.docx"
    source.write_bytes(b"benchmark-source")
    source_sha256 = sha256(source)
    plan_path = plan(tmp_path, source_sha256, "repeat_run")
    trace = trace_document(source_sha256, "repeat-forged")
    trace_path = write_json(tmp_path / "trace-forged.json", trace)
    reference_path = reference(tmp_path)

    forged_context = {
        "schema": harness.CONTEXT_SCHEMA,
        "producer": harness.PRODUCER,
        "claim": harness.CONTEXT_CLAIM,
        "session_id": "forged-repeat",
        "measurement_plan_sha256": sha256(plan_path),
        "corpus_spec_sha256": sha256(SPEC),
        "source_sha256": source_sha256,
        "trace_sha256": sha256(trace_path),
        "cache_state": "warm_cache",
        "run_phase": "repeat_run",
        "cache_action": "warm_prime",
        "cache_command_sha256": "a" * 64,
        "measurement_command_sha256": "b" * 64,
        "prior_trace_sha256": None,
        "cache_duration_ms": 1.0,
        "measurement_duration_ms": 2.0,
    }
    context_path = write_json(tmp_path / "contexts" / "forged.json", forged_context)

    result = {
        "schema": observation.OBSERVATION_SCHEMA,
        "claim": observation.OBSERVATION_CLAIM,
        "targets_sha256": sha256(TARGETS),
        "reference_policy_sha256": sha256(reference_path),
        "measurement_plan_sha256": sha256(plan_path),
        "trace_sha256": sha256(trace_path),
        "benchmark_context_sha256": sha256(context_path),
        "reference_id": "benchmark-reference-01",
        "corpus_id": "dokkomplekt-e6-synthetic-v1",
        "series_id": "series-source_analysis",
        "metric": "source_analysis",
        "class": "typical_docx",
        "cache_state": "warm_cache",
        "run_kind": "repeat_run",
        "conditions": {"ocr": False},
        "complexity": None,
        "warmup": False,
        "sample_ms": 14.0,
        "sample_derivation": "per_run_stage_sum:source_open+source_parse+candidate_index+source_resolve",
        "trace_run_id": "repeat-forged",
        "trace_app_version": "18.4.7",
        "trace_source_sha256": source_sha256,
        "trace_workload": "single_document",
        "trace_batch_size": 1,
        "trace_feature_flags": {
            "ocr_used": False,
            "runtime_layout_used": False,
            "pdf_used": False,
        },
    }
    observations_dir = tmp_path / "observations"
    write_json(observations_dir / "forged.json", result)

    with pytest.raises(ValueError, match="requires prior trace evidence"):
        assemble.build_evidence(
            TARGETS,
            reference_path,
            observations_dir,
            protocol(tmp_path),
            resources(tmp_path, reference_path),
            None,
            context_path.parent,
        )
