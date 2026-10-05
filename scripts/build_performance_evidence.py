#!/usr/bin/env python3
"""Build fail-closed Canon E6 performance evidence from persisted runtime traces.

The builder never invents measurements. A measurement plan binds explicit opaque
run_ids to Canon series metadata; this script validates each referenced trace
before extracting per-run samples. The existing performance_slo_gate.py remains
the authority that decides PASS/FAIL against the bound reference policy.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

TRACE_SCHEMA = "dokkomplekt.performance-trace.v1"
PLAN_SCHEMA = "dokkomplekt.performance-measurement-plan.v1"
REFERENCE_SCHEMA = "dokkomplekt.performance-reference.v1"
EVIDENCE_SCHEMA = "dokkomplekt.performance-evidence.v1"
RESOURCE_SCHEMA = "dokkomplekt.performance-resource-snapshot.v1"

CANONICAL_METRICS = {
    "source_analysis",
    "preflight_calculation",
    "render_readback_verify",
    "button_to_ready",
    "large_docx_to_ready",
    "cold_start_to_interactive_ui",
    "visible_action_response",
    "prompt_form_ready",
}
CANONICAL_CLASSES = {
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
}
CANONICAL_CACHE_STATES = {"cold_cache", "warm_cache"}
CANONICAL_RUN_KINDS = {"first_run", "repeat_run", "single_document", "batch_10", "batch_50"}
REQUIRED_PROTOCOL_FLAGS = (
    "human_wait_excluded",
    "end_to_end_measured_directly",
    "drop_to_ready_reported",
    "click_to_ready_reported",
    "special_classes_separated",
    "sample_count_and_warmup_recorded",
    "verification_enabled",
)
RESOURCE_KEYS = (
    "cpu_percent_peak",
    "peak_rss_bytes",
    "disk_write_bytes",
    "staging_peak_bytes",
    "cold_start_ms",
    "ui_response_ms",
    "worker_count",
    "queue_limit",
)
ALLOWED_STAGES = {
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
}


def load_object(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"{path}: root must be an object")
    return data


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_traces(root: Path) -> dict[str, dict[str, Any]]:
    traces: dict[str, dict[str, Any]] = {}
    for path in sorted(root.rglob("*.json")):
        try:
            trace = load_object(path)
        except (OSError, ValueError, json.JSONDecodeError):
            continue
        if trace.get("schema") != TRACE_SCHEMA:
            continue
        context = trace.get("context")
        if not isinstance(context, dict):
            raise ValueError(f"{path}: trace context must be an object")
        run_id = context.get("run_id")
        if not isinstance(run_id, str) or not run_id.strip():
            raise ValueError(f"{path}: trace run_id must be a non-empty string")
        if run_id in traces:
            raise ValueError(f"duplicate trace run_id: {run_id}")
        trace["_source_file"] = str(path)
        traces[run_id] = trace
    if not traces:
        raise ValueError(f"no {TRACE_SCHEMA} traces found under {root}")
    return traces


def stage_map(trace: dict[str, Any]) -> dict[str, float]:
    raw = trace.get("stages")
    if not isinstance(raw, list):
        raise ValueError("trace stages must be an array")
    result: dict[str, float] = {}
    for item in raw:
        if not isinstance(item, dict):
            raise ValueError("trace stage entry must be an object")
        stage = item.get("stage")
        duration = item.get("duration_ms")
        if stage not in ALLOWED_STAGES:
            raise ValueError(f"unsupported trace stage: {stage!r}")
        if stage in result:
            raise ValueError(f"duplicate trace stage: {stage}")
        if not isinstance(duration, (int, float)) or isinstance(duration, bool) or duration < 0:
            raise ValueError(f"invalid duration for stage {stage!r}")
        result[str(stage)] = float(duration)
    return result


def validate_trace_for_series(
    trace: dict[str, Any],
    series: dict[str, Any],
    reference: dict[str, Any],
) -> None:
    if trace.get("outcome") != "completed":
        raise ValueError(f"run {trace.get('context', {}).get('run_id')}: only completed traces may back SLO evidence")
    context = trace.get("context")
    if not isinstance(context, dict):
        raise ValueError("trace context must be an object")

    expected_version = reference.get("environment", {}).get("app_version")
    if context.get("app_version") != expected_version:
        raise ValueError(
            f"run {context.get('run_id')}: app_version {context.get('app_version')!r} "
            f"does not match reference {expected_version!r}"
        )

    run_kind = series["run_kind"]
    workload = context.get("workload")
    run_phase = context.get("run_phase")
    if run_kind == "single_document" and workload != "single_document":
        raise ValueError(f"run {context.get('run_id')}: expected single_document workload")
    if run_kind == "batch_10" and workload != "batch_10":
        raise ValueError(f"run {context.get('run_id')}: expected batch_10 workload")
    if run_kind == "batch_50" and workload != "batch_50":
        raise ValueError(f"run {context.get('run_id')}: expected batch_50 workload")
    if run_kind == "first_run" and run_phase != "first_run":
        raise ValueError(f"run {context.get('run_id')}: expected first_run phase")
    if run_kind == "repeat_run" and run_phase != "repeat_run":
        raise ValueError(f"run {context.get('run_id')}: expected repeat_run phase")

    cache_state = context.get("cache_state")
    if cache_state not in ("unclassified", series["cache_state"]):
        raise ValueError(
            f"run {context.get('run_id')}: trace cache_state {cache_state!r} conflicts with "
            f"series {series['cache_state']!r}"
        )

    class_name = series["class"]
    ocr = context.get("ocr_used") is True
    pdf = context.get("pdf_used") is True
    runtime_layout = context.get("runtime_layout_used") is True
    if class_name == "typical_docx" and (ocr or pdf or runtime_layout):
        raise ValueError(
            f"run {context.get('run_id')}: special OCR/PDF/runtime-layout trace cannot enter typical_docx"
        )
    if class_name == "ocr" and not ocr:
        raise ValueError(f"run {context.get('run_id')}: ocr class requires ocr_used=true")
    if class_name == "pdf" and not pdf:
        raise ValueError(f"run {context.get('run_id')}: pdf class requires pdf_used=true")
    if class_name == "runtime_layout" and not runtime_layout:
        raise ValueError(
            f"run {context.get('run_id')}: runtime_layout class requires runtime_layout_used=true"
        )
    if class_name == "batch_10" and workload != "batch_10":
        raise ValueError(f"run {context.get('run_id')}: batch_10 class requires batch_10 workload")
    if class_name == "batch_50" and workload != "batch_50":
        raise ValueError(f"run {context.get('run_id')}: batch_50 class requires batch_50 workload")


def extract_sample(trace: dict[str, Any], extractor: dict[str, Any]) -> float:
    kind = extractor.get("kind")
    if kind == "end_to_end_ms":
        if trace.get("human_wait_ms") not in (0, 0.0):
            raise ValueError("end_to_end_ms extractor requires human_wait_ms=0")
        value = trace.get("end_to_end_ms")
        if not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0:
            raise ValueError("trace end_to_end_ms must be a non-negative number")
        return float(value)

    stages = stage_map(trace)
    if kind == "stage_ms":
        stage = extractor.get("stage")
        if stage not in ALLOWED_STAGES:
            raise ValueError(f"stage_ms extractor has unsupported stage: {stage!r}")
        if stage not in stages:
            raise ValueError(f"trace does not contain required stage: {stage}")
        return stages[stage]

    if kind == "stage_sum_ms":
        requested = extractor.get("stages")
        if not isinstance(requested, list) or not requested:
            raise ValueError("stage_sum_ms requires a non-empty stages array")
        names = [str(stage) for stage in requested]
        if len(set(names)) != len(names) or any(stage not in ALLOWED_STAGES for stage in names):
            raise ValueError("stage_sum_ms contains duplicate or unsupported stages")
        missing = [stage for stage in names if stage not in stages]
        if missing:
            raise ValueError(f"trace does not contain required stages: {missing}")
        # Sum per run, never percentile-of-stage sums. The evaluator computes
        # percentiles only after these direct per-run samples are materialized.
        return sum(stages[stage] for stage in names)

    raise ValueError(f"unsupported extractor kind: {kind!r}")


def validate_plan(plan: dict[str, Any]) -> list[dict[str, Any]]:
    if plan.get("schema") != PLAN_SCHEMA:
        raise ValueError(f"measurement plan schema must be {PLAN_SCHEMA}")
    protocol = plan.get("protocol")
    if not isinstance(protocol, dict):
        raise ValueError("measurement plan protocol must be an object")
    for flag in REQUIRED_PROTOCOL_FLAGS:
        if protocol.get(flag) is not True:
            raise ValueError(f"measurement plan protocol.{flag} must be true")
    series = plan.get("series")
    if not isinstance(series, list) or not series:
        raise ValueError("measurement plan series must be a non-empty array")
    seen: set[str] = set()
    normalized: list[dict[str, Any]] = []
    for index, item in enumerate(series):
        if not isinstance(item, dict):
            raise ValueError(f"series[{index}] must be an object")
        series_id = item.get("id")
        if not isinstance(series_id, str) or not series_id.strip() or series_id in seen:
            raise ValueError(f"series[{index}].id must be unique and non-empty")
        seen.add(series_id)
        if item.get("metric") not in CANONICAL_METRICS:
            raise ValueError(f"series[{index}].metric is not canonical")
        if item.get("class") not in CANONICAL_CLASSES:
            raise ValueError(f"series[{index}].class is not canonical")
        if item.get("cache_state") not in CANONICAL_CACHE_STATES:
            raise ValueError(f"series[{index}].cache_state is not canonical")
        if item.get("run_kind") not in CANONICAL_RUN_KINDS:
            raise ValueError(f"series[{index}].run_kind is not canonical")
        if not isinstance(item.get("conditions", {}), dict):
            raise ValueError(f"series[{index}].conditions must be an object")
        warmup = item.get("warmup_runs")
        if not isinstance(warmup, int) or isinstance(warmup, bool) or warmup < 0:
            raise ValueError(f"series[{index}].warmup_runs must be a non-negative integer")
        run_ids = item.get("run_ids")
        if not isinstance(run_ids, list) or not run_ids or any(not isinstance(v, str) or not v for v in run_ids):
            raise ValueError(f"series[{index}].run_ids must be a non-empty string array")
        if len(set(run_ids)) != len(run_ids):
            raise ValueError(f"series[{index}].run_ids contains duplicates")
        if not isinstance(item.get("extractor"), dict):
            raise ValueError(f"series[{index}].extractor must be an object")
        normalized.append(item)
    return normalized


def build_evidence(
    targets_path: Path,
    reference_path: Path,
    plan_path: Path,
    resources_path: Path,
    trace_root: Path,
) -> dict[str, Any]:
    reference = load_object(reference_path)
    if reference.get("schema") != REFERENCE_SCHEMA:
        raise ValueError(f"reference schema must be {REFERENCE_SCHEMA}")
    plan = load_object(plan_path)
    series_plan = validate_plan(plan)
    resources_doc = load_object(resources_path)
    if resources_doc.get("schema") != RESOURCE_SCHEMA:
        raise ValueError(f"resource snapshot schema must be {RESOURCE_SCHEMA}")
    resources = resources_doc.get("resources")
    if not isinstance(resources, dict) or set(resources) != set(RESOURCE_KEYS):
        raise ValueError("resource snapshot must define exactly the canonical resource keys")

    traces = load_traces(trace_root)
    used_run_ids: set[str] = set()
    output_series: list[dict[str, Any]] = []
    for series in series_plan:
        samples: list[float] = []
        for run_id in series["run_ids"]:
            if run_id in used_run_ids:
                raise ValueError(f"run_id is assigned to more than one series: {run_id}")
            trace = traces.get(run_id)
            if trace is None:
                raise ValueError(f"measurement plan references missing trace run_id: {run_id}")
            validate_trace_for_series(trace, series, reference)
            samples.append(extract_sample(trace, series["extractor"]))
            used_run_ids.add(run_id)

        row = {
            "id": series["id"],
            "metric": series["metric"],
            "class": series["class"],
            "cache_state": series["cache_state"],
            "run_kind": series["run_kind"],
            "conditions": series.get("conditions", {}),
            "warmup_runs": series["warmup_runs"],
            "samples_ms": samples,
            "trace_run_ids": list(series["run_ids"]),
        }
        if "complexity" in series:
            if not isinstance(series["complexity"], dict):
                raise ValueError(f"series[{series['id']}].complexity must be an object")
            row["complexity"] = series["complexity"]
        output_series.append(row)

    return {
        "schema": EVIDENCE_SCHEMA,
        "targets_sha256": sha256_file(targets_path),
        "reference_policy_sha256": sha256_file(reference_path),
        "reference_id": reference.get("reference_id"),
        "corpus_id": reference.get("corpus_id"),
        "environment": reference.get("environment"),
        "protocol": plan["protocol"],
        "series": output_series,
        "resources": resources,
        "source_trace_count": len(used_run_ids),
        "measurement_plan_sha256": sha256_file(plan_path),
        "resource_snapshot_sha256": sha256_file(resources_path),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Build Canon E6 evidence from runtime performance traces")
    parser.add_argument("--targets", default="performance/slo-targets.json")
    parser.add_argument("--reference", required=True)
    parser.add_argument("--plan", required=True)
    parser.add_argument("--resources", required=True)
    parser.add_argument("--trace-dir", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    try:
        evidence = build_evidence(
            Path(args.targets),
            Path(args.reference),
            Path(args.plan),
            Path(args.resources),
            Path(args.trace_dir),
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}")
        return 2
    encoded = json.dumps(evidence, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(encoded, encoding="utf-8")
    print(f"PERFORMANCE EVIDENCE BUILT: series={len(evidence['series'])}; traces={evidence['source_trace_count']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
