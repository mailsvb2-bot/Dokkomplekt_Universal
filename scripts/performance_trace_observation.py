#!/usr/bin/env python3
"""Bind one privacy-safe runtime performance trace to one predeclared SLO measurement plan.

This tool deliberately does not produce a performance PASS. It converts one exact
runtime trace into one hash-bound observation. A later evidence assembler must
still provide the full Canon corpus/resource coverage required by
performance_slo_gate.py.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

from scripts import performance_slo_gate as gate

TRACE_SCHEMA = "dokkomplekt.performance-trace.v1"
PLAN_SCHEMA = "dokkomplekt.performance-trace-measurement-plan.v1"
OBSERVATION_SCHEMA = "dokkomplekt.performance-observation.v1"
OBSERVATION_CLAIM = "single_sample_only_not_slo_verdict"
BENCHMARK_CONTEXT_SCHEMA = "dokkomplekt.performance-benchmark-context.v1"
BENCHMARK_CONTEXT_PRODUCER = "performance_benchmark_harness.py"
BENCHMARK_CONTEXT_CLAIM = "benchmark_lifecycle_context_only_not_slo_verdict"
BENCHMARK_CONTEXT_KEYS = {
    "schema", "producer", "claim", "session_id", "measurement_plan_sha256",
    "corpus_spec_sha256", "source_sha256", "trace_sha256", "cache_state",
    "run_phase", "cache_action", "cache_command_sha256",
    "measurement_command_sha256", "prior_trace_sha256", "cache_duration_ms",
    "measurement_duration_ms",
}

TRACE_KEYS = {
    "schema",
    "context",
    "stages",
    "total_machine_ms",
    "end_to_end_ms",
    "human_wait_ms",
    "outcome",
}
CONTEXT_KEYS = {
    "run_id",
    "app_version",
    "class",
    "cache_state",
    "run_phase",
    "workload",
    "batch_size",
    "ocr_used",
    "runtime_layout_used",
    "pdf_used",
}
CONTEXT_OPTIONAL_KEYS = {"source_sha256"}
STAGE_KEYS = {"stage", "duration_ms"}
PLAN_KEYS = {
    "schema",
    "reference_id",
    "corpus_id",
    "series_id",
    "metric",
    "class",
    "cache_state",
    "run_kind",
    "conditions",
    "complexity",
    "warmup",
    "expected_app_version",
    "expected_source_sha256",
    "expected_workload",
    "expected_ocr_used",
    "expected_runtime_layout_used",
    "expected_pdf_used",
}

CANONICAL_STAGE_ORDER = (
    "source_open",
    "source_parse",
    "candidate_index",
    "source_resolve",
    "prompt_plan",
    "preflight",
    "reference_clone",
    "replay",
    "physical_readback",
    "verify",
    "publish",
    "recovery",
)
TRACE_WORKLOADS = ("single_document", "batch_10", "batch_50", "other_batch")
TRACE_RUN_PHASES = ("unclassified", "first_run", "repeat_run")
TRACE_CLASSES = (
    "unclassified",
    "small_docx",
    "typical_docx",
    "large_docx",
    "table_heavy",
    "header_footer_heavy",
    "long_text",
    "repeated_blocks",
    "batch_10",
    "batch_50",
    "accounting_table",
    "hr_kit",
    "many_roles",
    "ocr",
    "runtime_layout",
    "pdf",
    "slow_storage",
    "network_storage",
)
TRACE_CACHE_STATES = ("unclassified", "cold_cache", "warm_cache")
TRACE_OUTCOMES = ("completed", "attention", "failed", "cancelled")

TRACE_DERIVED_METRICS: dict[str, tuple[str, ...] | None] = {
    "source_analysis": (
        "source_open",
        "source_parse",
        "candidate_index",
        "source_resolve",
    ),
    "preflight_calculation": ("prompt_plan", "preflight"),
    "render_readback_verify": ("replay", "physical_readback", "verify"),
    "button_to_ready": None,
    "large_docx_to_ready": None,
}


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    if actual != expected:
        raise ValueError(
            f"{label} keys must be closed: missing={sorted(expected - actual)} "
            f"unexpected={sorted(actual - expected)}"
        )


def _nonempty_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{label} must be a non-empty string")
    return value.strip()


def _nonnegative_number(value: Any, label: str) -> float:
    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not math.isfinite(float(value))
        or float(value) < 0
    ):
        raise ValueError(f"{label} must be a finite non-negative number")
    return float(value)


def _validate_trace(trace: dict[str, Any]) -> dict[str, Any]:
    _exact_keys(trace, TRACE_KEYS, "trace")
    if trace.get("schema") != TRACE_SCHEMA:
        raise ValueError(f"trace schema must be {TRACE_SCHEMA}")

    context = trace.get("context")
    if not isinstance(context, dict):
        raise ValueError("trace.context must be an object")
    actual_context_keys = set(context)
    missing_context_keys = CONTEXT_KEYS - actual_context_keys
    unexpected_context_keys = actual_context_keys - CONTEXT_KEYS - CONTEXT_OPTIONAL_KEYS
    if missing_context_keys or unexpected_context_keys:
        raise ValueError(
            "trace.context keys must be closed: "
            f"missing={sorted(missing_context_keys)} "
            f"unexpected={sorted(unexpected_context_keys)}"
        )

    run_id = _nonempty_string(context.get("run_id"), "trace.context.run_id")
    if len(run_id) > 128 or not all(
        character.isascii() and (character.isalnum() or character in "-_.")
        for character in run_id
    ):
        raise ValueError("trace.context.run_id must remain an opaque ASCII identifier")

    app_version = _nonempty_string(
        context.get("app_version"), "trace.context.app_version"
    )
    source_sha256 = context.get("source_sha256")
    if source_sha256 is not None:
        if (
            not isinstance(source_sha256, str)
            or len(source_sha256) != 64
            or any(character not in "0123456789abcdef" for character in source_sha256)
        ):
            raise ValueError("trace.context.source_sha256 must be a lowercase SHA-256 hex digest or null")
    if context.get("class") not in TRACE_CLASSES:
        raise ValueError("trace.context.class is not canonical")
    if context.get("cache_state") not in TRACE_CACHE_STATES:
        raise ValueError("trace.context.cache_state is not canonical")
    if context.get("run_phase") not in TRACE_RUN_PHASES:
        raise ValueError("trace.context.run_phase is not canonical")
    if context.get("workload") not in TRACE_WORKLOADS:
        raise ValueError("trace.context.workload is not canonical")

    batch_size = context.get("batch_size")
    if not isinstance(batch_size, int) or isinstance(batch_size, bool) or batch_size <= 0:
        raise ValueError("trace.context.batch_size must be a positive integer")
    workload = context["workload"]
    exact_sizes = {"single_document": 1, "batch_10": 10, "batch_50": 50}
    if workload in exact_sizes and batch_size != exact_sizes[workload]:
        raise ValueError(f"trace workload {workload} requires batch_size={exact_sizes[workload]}")
    if workload == "other_batch" and batch_size in (1, 10, 50):
        raise ValueError("trace other_batch cannot encode a canonical batch size")

    for flag in ("ocr_used", "runtime_layout_used", "pdf_used"):
        if not isinstance(context.get(flag), bool):
            raise ValueError(f"trace.context.{flag} must be boolean")

    stages = trace.get("stages")
    if not isinstance(stages, list):
        raise ValueError("trace.stages must be an array")
    order = {name: index for index, name in enumerate(CANONICAL_STAGE_ORDER)}
    seen: set[str] = set()
    previous = -1
    stage_durations: dict[str, float] = {}
    for index, item in enumerate(stages):
        if not isinstance(item, dict):
            raise ValueError(f"trace.stages[{index}] must be an object")
        _exact_keys(item, STAGE_KEYS, f"trace.stages[{index}]")
        stage = item.get("stage")
        if stage not in order:
            raise ValueError(f"trace.stages[{index}].stage is not canonical")
        if stage in seen:
            raise ValueError(f"duplicate trace stage: {stage}")
        if order[stage] <= previous:
            raise ValueError("trace stages must follow canonical order")
        seen.add(stage)
        previous = order[stage]
        stage_durations[stage] = _nonnegative_number(
            item.get("duration_ms"), f"trace.stages[{index}].duration_ms"
        )

    total_machine = _nonnegative_number(
        trace.get("total_machine_ms"), "trace.total_machine_ms"
    )
    calculated_total = sum(stage_durations.values())
    if total_machine != calculated_total:
        raise ValueError("trace.total_machine_ms does not match stage durations")
    end_to_end = _nonnegative_number(trace.get("end_to_end_ms"), "trace.end_to_end_ms")
    human_wait = _nonnegative_number(trace.get("human_wait_ms"), "trace.human_wait_ms")
    if human_wait > end_to_end:
        raise ValueError("trace.human_wait_ms cannot exceed end_to_end_ms")
    if total_machine > end_to_end - human_wait:
        raise ValueError("trace machine time exceeds end-to-end minus human wait")

    outcome = trace.get("outcome")
    if outcome not in TRACE_OUTCOMES:
        raise ValueError("trace.outcome is not canonical")
    if outcome != "completed":
        raise ValueError("SLO measurement observations require a completed trace")
    if "publish" not in seen or "recovery" in seen:
        raise ValueError("completed SLO trace must include publish and must not include recovery")

    return {
        "context": context,
        "stage_durations": stage_durations,
        "end_to_end_ms": end_to_end,
        "human_wait_ms": human_wait,
        "run_id": run_id,
        "app_version": app_version,
        "source_sha256": source_sha256,
    }


def _validate_plan(
    plan: dict[str, Any],
    targets: dict[str, Any],
    reference: dict[str, Any],
) -> None:
    _exact_keys(plan, PLAN_KEYS, "plan")
    if plan.get("schema") != PLAN_SCHEMA:
        raise ValueError(f"plan schema must be {PLAN_SCHEMA}")
    if plan.get("reference_id") != reference.get("reference_id"):
        raise ValueError("plan.reference_id does not match the bound reference")
    if plan.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError("plan.corpus_id does not match the bound reference")

    metric = _nonempty_string(plan.get("metric"), "plan.metric")
    if metric not in gate.CANONICAL_METRICS:
        raise ValueError("plan.metric is not a Canon SLO metric")
    if metric not in TRACE_DERIVED_METRICS:
        raise ValueError(
            f"{metric} is not derivable from runtime traces; use physical/UI measurement evidence"
        )
    series_id = _nonempty_string(plan.get("series_id"), "plan.series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict):
        raise ValueError("reference metric_bindings must be an object")
    is_bound_slo_series = bindings.get(metric) == series_id

    class_name = plan.get("class")
    allowed_classes = set(gate.CANONICAL_CORPUS_CLASSES) | set(gate.CANONICAL_SPECIAL_CLASSES)
    if class_name not in allowed_classes:
        raise ValueError("plan.class is not a Canon performance class")
    if plan.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError("plan.cache_state is not canonical")
    if plan.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError("plan.run_kind is not canonical")

    conditions = plan.get("conditions")
    if not isinstance(conditions, dict):
        raise ValueError("plan.conditions must be an object")
    complexity = plan.get("complexity")
    if complexity is not None and not isinstance(complexity, dict):
        raise ValueError("plan.complexity must be an object or null")
    if not isinstance(plan.get("warmup"), bool):
        raise ValueError("plan.warmup must be boolean")

    expected_app_version = _nonempty_string(
        plan.get("expected_app_version"), "plan.expected_app_version"
    )
    expected_source_sha256 = plan.get("expected_source_sha256")
    if expected_source_sha256 is not None and (
        not isinstance(expected_source_sha256, str)
        or len(expected_source_sha256) != 64
        or any(character not in "0123456789abcdef" for character in expected_source_sha256)
    ):
        raise ValueError("plan.expected_source_sha256 must be a lowercase SHA-256 hex digest or null")
    environment = reference.get("environment")
    if not isinstance(environment, dict) or environment.get("app_version") != expected_app_version:
        raise ValueError("plan.expected_app_version must equal reference environment app_version")
    if plan.get("expected_workload") not in TRACE_WORKLOADS:
        raise ValueError("plan.expected_workload is not canonical")
    for key in (
        "expected_ocr_used",
        "expected_runtime_layout_used",
        "expected_pdf_used",
    ):
        if not isinstance(plan.get(key), bool):
            raise ValueError(f"plan.{key} must be boolean")

    # Only the reference-bound series may claim this metric's SLO. Additional
    # predeclared series are allowed solely to provide the Canon's required
    # corpus/cache/run-kind coverage; the evaluator never uses them for the
    # bound metric verdict.
    if class_name == "runtime_layout":
        raise ValueError(
            "runtime_layout coverage requires dedicated runtime-layout execution evidence; "
            "generic runtime traces cannot prove that production layout synthesis executed"
        )

    if not is_bound_slo_series:
        if class_name in {"slow_storage", "network_storage"}:
            raise ValueError(
                f"{class_name} coverage requires dedicated storage-condition evidence; "
                "runtime trace plans cannot prove storage topology"
            )
        if expected_source_sha256 is None:
            raise ValueError("coverage series requires expected_source_sha256")
        coverage_sources = reference.get("coverage_sources")
        if not isinstance(coverage_sources, dict):
            raise ValueError("reference coverage_sources must bind non-bound coverage series")
        allowed_sources = coverage_sources.get(class_name)
        if (
            not isinstance(allowed_sources, list)
            or expected_source_sha256 not in allowed_sources
        ):
            raise ValueError("plan.expected_source_sha256 is not predeclared for this coverage class")

    if is_bound_slo_series:
        requirements = gate.SLO_SERIES_REQUIREMENTS[metric]
        required_class = requirements.get("class")
        if required_class is not None and class_name != required_class:
            raise ValueError(f"{metric} requires class={required_class}")
        required_cache = requirements.get("cache_state")
        if required_cache is not None and plan.get("cache_state") != required_cache:
            raise ValueError(f"{metric} requires cache_state={required_cache}")
        required_run = requirements.get("run_kind")
        if required_run is not None and plan.get("run_kind") != required_run:
            raise ValueError(f"{metric} requires run_kind={required_run}")
        for flag, expected in requirements.get("condition_flags", {}).items():
            if conditions.get(flag) is not expected:
                raise ValueError(f"{metric} requires conditions.{flag}={expected!r}")
        if requirements.get("require_complexity") and not isinstance(complexity, dict):
            raise ValueError(f"{metric} requires structural complexity metadata")

    if class_name == "large_docx":
        if not isinstance(complexity, dict):
            raise ValueError("large_docx measurement requires complexity metadata")
        for dimension in gate.CANONICAL_COMPLEXITY_DIMENSIONS:
            if dimension not in complexity:
                raise ValueError(f"large_docx complexity missing: {dimension}")
            _nonnegative_number(complexity[dimension], f"plan.complexity.{dimension}")
        compressed = _nonnegative_number(
            complexity.get("compressed_bytes"), "plan.complexity.compressed_bytes"
        )
        if compressed > gate.LARGE_DOCX_COMPRESSED_BYTES_MAX:
            raise ValueError("large_docx compressed_bytes exceeds the Canon ~25 MiB bound")

    # Keep plan conditions aligned with trace feature flags for classes that must
    # never be silently mixed into the ordinary DOCX SLO population.
    if class_name == "typical_docx" and any(
        plan[key]
        for key in (
            "expected_ocr_used",
            "expected_runtime_layout_used",
            "expected_pdf_used",
        )
    ):
        raise ValueError("typical_docx plan cannot enable OCR/runtime-layout/PDF flags")
    special_flag = {
        "ocr": "expected_ocr_used",
        "runtime_layout": "expected_runtime_layout_used",
        "pdf": "expected_pdf_used",
    }.get(class_name)
    if special_flag is not None and plan[special_flag] is not True:
        raise ValueError(f"{class_name} plan must enable {special_flag}")


def _validate_benchmark_context(
    path: Path,
    *,
    plan_path: Path,
    trace_path: Path,
    trace_info: dict[str, Any],
    plan: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    _exact_keys(value, BENCHMARK_CONTEXT_KEYS, "benchmark context")
    if (
        value.get("schema") != BENCHMARK_CONTEXT_SCHEMA
        or value.get("producer") != BENCHMARK_CONTEXT_PRODUCER
        or value.get("claim") != BENCHMARK_CONTEXT_CLAIM
    ):
        raise ValueError("benchmark context provenance is invalid")
    if value.get("measurement_plan_sha256") != _sha256(plan_path):
        raise ValueError("benchmark context is not bound to the exact measurement plan")
    if value.get("trace_sha256") != _sha256(trace_path):
        raise ValueError("benchmark context is not bound to the exact trace")
    if value.get("source_sha256") != trace_info.get("source_sha256"):
        raise ValueError("benchmark context source does not match trace source")
    cache_state = value.get("cache_state")
    if cache_state not in gate.CANONICAL_CACHE_STATES:
        raise ValueError("benchmark context cache_state is invalid")
    if cache_state != plan.get("cache_state"):
        raise ValueError("benchmark context cache_state does not match plan.cache_state")
    cache_action = value.get("cache_action")
    expected_action = "cold_reset" if cache_state == "cold_cache" else "warm_prime"
    if cache_action != expected_action:
        raise ValueError("benchmark context cache action does not prove declared cache_state")
    run_phase = value.get("run_phase")
    if run_phase not in ("first_run", "repeat_run", "unclassified"):
        raise ValueError("benchmark context run_phase is invalid")
    run_kind = plan.get("run_kind")
    if run_kind in ("first_run", "repeat_run") and run_phase != run_kind:
        raise ValueError("benchmark context run_phase does not match plan.run_kind")
    prior = value.get("prior_trace_sha256")
    if run_phase == "first_run" and prior is not None:
        raise ValueError("first_run benchmark context cannot contain prior trace evidence")
    if run_phase == "repeat_run" and (
        not isinstance(prior, str)
        or len(prior) != 64
        or any(character not in "0123456789abcdef" for character in prior)
    ):
        raise ValueError("repeat_run benchmark context requires prior trace evidence")
    for key in (
        "corpus_spec_sha256",
        "cache_command_sha256",
        "measurement_command_sha256",
    ):
        candidate = value.get(key)
        if (
            not isinstance(candidate, str)
            or len(candidate) != 64
            or any(character not in "0123456789abcdef" for character in candidate)
        ):
            raise ValueError(f"benchmark context {key} is invalid")
    for key in ("cache_duration_ms", "measurement_duration_ms"):
        _nonnegative_number(value.get(key), f"benchmark context {key}")
    raw_context = trace_info["context"]
    if raw_context["cache_state"] not in ("unclassified", cache_state):
        raise ValueError("trace cache_state conflicts with benchmark context")
    if run_phase != "unclassified" and raw_context["run_phase"] not in (
        "unclassified",
        run_phase,
    ):
        raise ValueError("trace run_phase conflicts with benchmark context")
    return value


def _validate_trace_against_plan(
    trace_info: dict[str, Any],
    plan: dict[str, Any],
    benchmark_context: dict[str, Any] | None = None,
) -> None:
    context = trace_info["context"]
    if trace_info["app_version"] != plan["expected_app_version"]:
        raise ValueError("trace app_version does not match the predeclared measurement plan")
    if plan.get("expected_source_sha256") is not None:
        if trace_info.get("source_sha256") != plan["expected_source_sha256"]:
            raise ValueError("trace source_sha256 does not match the predeclared measurement plan")
    effective_cache_state = context["cache_state"]
    if effective_cache_state == "unclassified" and benchmark_context is not None:
        effective_cache_state = benchmark_context["cache_state"]
    if effective_cache_state != plan["cache_state"]:
        raise ValueError("trace cache_state does not match plan.cache_state")
    if context["workload"] != plan["expected_workload"]:
        raise ValueError("trace workload does not match the predeclared measurement plan")
    flag_pairs = (
        ("ocr_used", "expected_ocr_used"),
        ("runtime_layout_used", "expected_runtime_layout_used"),
        ("pdf_used", "expected_pdf_used"),
    )
    for trace_key, plan_key in flag_pairs:
        if context[trace_key] is not plan[plan_key]:
            raise ValueError(f"trace {trace_key} does not match {plan_key}")

    run_kind = plan["run_kind"]
    run_phase_map = {"first_run": "first_run", "repeat_run": "repeat_run"}
    workload_map = {
        "single_document": "single_document",
        "batch_10": "batch_10",
        "batch_50": "batch_50",
    }
    effective_run_phase = context["run_phase"]
    if effective_run_phase == "unclassified" and benchmark_context is not None:
        effective_run_phase = benchmark_context["run_phase"]
    if run_kind in run_phase_map and effective_run_phase != run_phase_map[run_kind]:
        raise ValueError("trace run_phase does not match plan.run_kind")
    if run_kind in workload_map and context["workload"] != workload_map[run_kind]:
        raise ValueError("trace workload does not match plan.run_kind")

    conditions = plan["conditions"]
    if conditions.get("ocr") is False and context["ocr_used"]:
        raise ValueError("trace used OCR but the bound series forbids OCR")
    if conditions.get("runtime_layout") is False and context["runtime_layout_used"]:
        raise ValueError("trace used runtime layout but the bound series forbids it")
    if conditions.get("questions_present") is False and trace_info["human_wait_ms"] != 0:
        raise ValueError("no-questions series cannot contain human wait time")


def _sample(trace_info: dict[str, Any], metric: str) -> tuple[float, str]:
    stages = TRACE_DERIVED_METRICS[metric]
    if stages is None:
        value = trace_info["end_to_end_ms"] - trace_info["human_wait_ms"]
        return value, "end_to_end_ms_minus_human_wait_ms"

    missing = [stage for stage in stages if stage not in trace_info["stage_durations"]]
    if missing:
        raise ValueError(
            f"{metric} requires runtime stages missing from this trace: {', '.join(missing)}"
        )
    # Sum within each run, never percentile-to-percentile. The SLO gate later
    # computes p50/p95 over these per-run samples.
    value = sum(trace_info["stage_durations"][stage] for stage in stages)
    return value, "per_run_stage_sum:" + "+".join(stages)


def build_observation(
    targets_path: Path,
    reference_path: Path,
    plan_path: Path,
    trace_path: Path,
    benchmark_context_path: Path | None = None,
) -> dict[str, Any]:
    targets = gate.load_object(targets_path)
    target_errors = gate.validate_targets(targets)
    if target_errors:
        raise ValueError("invalid targets: " + "; ".join(target_errors))

    reference = gate.load_object(reference_path)
    reference_errors = gate.validate_reference(reference, targets)
    if reference_errors:
        raise ValueError("invalid reference policy: " + "; ".join(reference_errors))

    plan = _load(plan_path)
    _validate_plan(plan, targets, reference)
    trace = _load(trace_path)
    trace_info = _validate_trace(trace)
    benchmark_context = (
        _validate_benchmark_context(
            benchmark_context_path,
            plan_path=plan_path,
            trace_path=trace_path,
            trace_info=trace_info,
            plan=plan,
        )
        if benchmark_context_path is not None
        else None
    )
    _validate_trace_against_plan(trace_info, plan, benchmark_context)
    sample_ms, derivation = _sample(trace_info, plan["metric"])

    return {
        "schema": OBSERVATION_SCHEMA,
        "claim": OBSERVATION_CLAIM,
        "targets_sha256": _sha256(targets_path),
        "reference_policy_sha256": _sha256(reference_path),
        "measurement_plan_sha256": _sha256(plan_path),
        "trace_sha256": _sha256(trace_path),
        "benchmark_context_sha256": (
            _sha256(benchmark_context_path)
            if benchmark_context_path is not None
            else None
        ),
        "reference_id": reference["reference_id"],
        "corpus_id": reference["corpus_id"],
        "series_id": plan["series_id"],
        "metric": plan["metric"],
        "class": plan["class"],
        "cache_state": plan["cache_state"],
        "run_kind": plan["run_kind"],
        "conditions": plan["conditions"],
        "complexity": plan["complexity"],
        "warmup": plan["warmup"],
        "sample_ms": sample_ms,
        "sample_derivation": derivation,
        "trace_run_id": trace_info["run_id"],
        "trace_app_version": trace_info["app_version"],
        "trace_source_sha256": trace_info["source_sha256"],
        "trace_workload": trace_info["context"]["workload"],
        "trace_batch_size": trace_info["context"]["batch_size"],
        "trace_feature_flags": {
            "ocr_used": trace_info["context"]["ocr_used"],
            "runtime_layout_used": trace_info["context"]["runtime_layout_used"],
            "pdf_used": trace_info["context"]["pdf_used"],
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Bind one runtime performance trace to one predeclared E6 measurement plan"
    )
    parser.add_argument("--targets", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--trace", required=True, type=Path)
    parser.add_argument("--benchmark-context", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)

    try:
        observation = build_observation(
            args.targets, args.reference, args.plan, args.trace, args.benchmark_context
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"PERFORMANCE OBSERVATION FAILED: {exc}")
        return 2

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(observation, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "PERFORMANCE OBSERVATION OK: "
        f"{observation['series_id']} sample_ms={observation['sample_ms']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
