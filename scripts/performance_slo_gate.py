#!/usr/bin/env python3
"""Fail-closed evaluator for Canon E6 performance evidence.

This module deliberately separates immutable target SLOs from a concrete
reference-machine policy. A performance PASS is impossible until a bound
reference policy supplies the exact machine/corpus identity, sample floor,
series bindings and resource budgets.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

TARGET_SCHEMA = "dokkomplekt.performance-slo.v1"
REFERENCE_SCHEMA = "dokkomplekt.performance-reference.v1"
EVIDENCE_SCHEMA = "dokkomplekt.performance-evidence.v1"

CANONICAL_METRICS: dict[str, dict[str, int]] = {
    "source_analysis": {"p50_ms_max": 250, "p95_ms_max": 700},
    "preflight_calculation": {"p50_ms_max": 100, "p95_ms_max": 300},
    "render_readback_verify": {"p50_ms_max": 500, "p95_ms_max": 1200},
    "button_to_ready": {"p50_ms_max": 1000, "p95_ms_max": 2000},
    "large_docx_to_ready": {"p95_ms_max": 5000},
    "cold_start_to_interactive_ui": {"p50_ms_max": 1500, "p95_ms_max": 3000},
    "visible_action_response": {"p95_ms_max": 100},
    "prompt_form_ready": {"p95_ms_max": 150},
}

REQUIRED_PROTOCOL_FLAGS = (
    "human_wait_excluded",
    "end_to_end_measured_directly",
    "drop_to_ready_reported",
    "click_to_ready_reported",
    "special_classes_separated",
    "sample_count_and_warmup_recorded",
    "verification_enabled",
)

RESOURCE_POLICY_KEYS = (
    "cpu_percent_peak",
    "peak_rss_bytes",
    "disk_write_bytes",
    "staging_peak_bytes",
    "cold_start_ms",
    "ui_response_ms",
    "worker_count",
    "queue_limit",
)


def load_object(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"{path}: root must be an object")
    return data


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_nonempty_string(value: Any, label: str, errors: list[str]) -> str:
    if not isinstance(value, str) or not value.strip():
        errors.append(f"{label} must be a non-empty string")
        return ""
    return value.strip()


def validate_targets(data: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if data.get("schema") != TARGET_SCHEMA:
        errors.append(f"targets schema must be {TARGET_SCHEMA}")
    if data.get("claim") != "targets_only_not_measured_pass":
        errors.append("targets must explicitly state targets_only_not_measured_pass")
    if data.get("quantile_method") != "nearest_rank":
        errors.append("quantile_method must be nearest_rank")

    metrics = data.get("metrics")
    if not isinstance(metrics, dict):
        errors.append("metrics must be an object")
        metrics = {}
    if set(metrics) != set(CANONICAL_METRICS):
        errors.append(
            "metric set mismatch: "
            f"expected={sorted(CANONICAL_METRICS)} actual={sorted(metrics)}"
        )
    for metric, expected in CANONICAL_METRICS.items():
        actual = metrics.get(metric)
        if not isinstance(actual, dict):
            errors.append(f"{metric}: metric definition missing")
            continue
        for key, value in expected.items():
            if actual.get(key) != value:
                errors.append(f"{metric}.{key} must be {value}")
        require_nonempty_string(actual.get("conditions"), f"{metric}.conditions", errors)

    typical = data.get("typical_docx")
    if not isinstance(typical, dict):
        errors.append("typical_docx must be an object")
    else:
        if typical.get("compressed_bytes_guideline_max") != 5 * 1024 * 1024:
            errors.append("typical_docx compressed guideline must be 5 MiB")
        dimensions = typical.get("complexity_dimensions")
        if not isinstance(dimensions, list) or len(dimensions) < 8:
            errors.append("typical_docx must record structural complexity dimensions")

    for key in (
        "required_environment_fields",
        "required_measurement_splits",
        "required_resource_measurements",
        "required_corpus_classes",
        "separate_performance_classes",
        "measurement_rules",
    ):
        value = data.get(key)
        if not isinstance(value, list) or not value or not all(
            isinstance(item, str) and item.strip() for item in value
        ):
            errors.append(f"{key} must be a non-empty string array")
    return errors


def validate_reference(
    policy: dict[str, Any],
    targets: dict[str, Any],
) -> list[str]:
    errors: list[str] = []
    if policy.get("schema") != REFERENCE_SCHEMA:
        errors.append(f"reference schema must be {REFERENCE_SCHEMA}")
    if policy.get("status") != "bound":
        errors.append("reference policy is not bound; measured PASS is forbidden")
    require_nonempty_string(policy.get("reference_id"), "reference_id", errors)
    require_nonempty_string(policy.get("corpus_id"), "corpus_id", errors)

    environment = policy.get("environment")
    if not isinstance(environment, dict):
        errors.append("reference environment must be an object")
        environment = {}
    for key in targets.get("required_environment_fields", []):
        value = environment.get(key)
        if value is None or value == "" or value == [] or value == {}:
            errors.append(f"reference environment field missing: {key}")

    min_samples = policy.get("min_samples_per_series")
    if not isinstance(min_samples, int) or isinstance(min_samples, bool) or min_samples < 3:
        errors.append("min_samples_per_series must be an integer >= 3")

    bindings = policy.get("metric_bindings")
    if not isinstance(bindings, dict):
        errors.append("metric_bindings must be an object")
        bindings = {}
    if set(bindings) != set(CANONICAL_METRICS):
        errors.append("metric_bindings must bind every Canon SLO metric exactly once")
    normalized_binding_ids: list[str] = []
    for metric, series_id in bindings.items():
        if metric not in CANONICAL_METRICS:
            continue
        normalized = require_nonempty_string(
            series_id, f"metric_bindings.{metric}", errors
        )
        if normalized:
            normalized_binding_ids.append(normalized)
    if len(set(normalized_binding_ids)) != len(normalized_binding_ids):
        errors.append("each metric must bind to a distinct measured series")

    budgets = policy.get("resource_budgets")
    if not isinstance(budgets, dict):
        errors.append("resource_budgets must be an object")
        budgets = {}
    if set(budgets) != set(RESOURCE_POLICY_KEYS):
        errors.append(
            "resource_budgets must define exactly: " + ", ".join(RESOURCE_POLICY_KEYS)
        )
    for key in RESOURCE_POLICY_KEYS:
        value = budgets.get(key)
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(float(value))
            or float(value) <= 0
        ):
            errors.append(f"resource_budgets.{key} must be a finite positive number")
    return errors


def nearest_rank(samples: list[float], percentile: float) -> float:
    if not samples:
        raise ValueError("cannot compute percentile for an empty sample")
    ordered = sorted(samples)
    rank = max(1, math.ceil(percentile * len(ordered)))
    return ordered[rank - 1]


def numeric_samples(raw: Any, label: str, errors: list[str]) -> list[float]:
    if not isinstance(raw, list):
        errors.append(f"{label} must be an array")
        return []
    values: list[float] = []
    for index, value in enumerate(raw):
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(float(value))
            or float(value) < 0
        ):
            errors.append(f"{label}[{index}] must be a finite non-negative number")
            continue
        values.append(float(value))
    return values


def evaluate(
    targets_path: Path,
    reference_path: Path,
    evidence_path: Path,
) -> dict[str, Any]:
    targets = load_object(targets_path)
    policy = load_object(reference_path)
    evidence = load_object(evidence_path)
    errors = validate_targets(targets)
    errors.extend(validate_reference(policy, targets))

    if evidence.get("schema") != EVIDENCE_SCHEMA:
        errors.append(f"evidence schema must be {EVIDENCE_SCHEMA}")
    if evidence.get("targets_sha256") != sha256_file(targets_path):
        errors.append("evidence is not bound to the exact target contract")
    if evidence.get("reference_policy_sha256") != sha256_file(reference_path):
        errors.append("evidence is not bound to the exact reference-machine policy")
    if evidence.get("reference_id") != policy.get("reference_id"):
        errors.append("evidence reference_id does not match the bound reference policy")
    if evidence.get("corpus_id") != policy.get("corpus_id"):
        errors.append("evidence corpus_id does not match the bound reference policy")
    if evidence.get("environment") != policy.get("environment"):
        errors.append("evidence environment differs from the bound reference machine")

    protocol = evidence.get("protocol")
    if not isinstance(protocol, dict):
        errors.append("evidence protocol must be an object")
        protocol = {}
    for flag in REQUIRED_PROTOCOL_FLAGS:
        if protocol.get(flag) is not True:
            errors.append(f"protocol.{flag} must be true")

    coverage = evidence.get("coverage")
    if not isinstance(coverage, dict):
        errors.append("evidence coverage must be an object")
        coverage = {}
    for evidence_key, target_key in (
        ("measurement_splits", "required_measurement_splits"),
        ("corpus_classes", "required_corpus_classes"),
        ("separate_performance_classes", "separate_performance_classes"),
        ("typical_docx_complexity_dimensions", "complexity_dimensions"),
    ):
        actual_raw = coverage.get(evidence_key)
        if evidence_key == "typical_docx_complexity_dimensions":
            expected_raw = (targets.get("typical_docx") or {}).get(target_key, [])
        else:
            expected_raw = targets.get(target_key, [])
        actual = {
            item.strip()
            for item in actual_raw
            if isinstance(item, str) and item.strip()
        } if isinstance(actual_raw, list) else set()
        expected = {
            item.strip()
            for item in expected_raw
            if isinstance(item, str) and item.strip()
        } if isinstance(expected_raw, list) else set()
        missing = sorted(expected - actual)
        if missing:
            errors.append(
                f"coverage.{evidence_key} is incomplete; missing={missing}"
            )

    series_raw = evidence.get("series")
    if not isinstance(series_raw, list):
        errors.append("evidence series must be an array")
        series_raw = []
    series_by_id: dict[str, dict[str, Any]] = {}
    for index, series in enumerate(series_raw):
        if not isinstance(series, dict):
            errors.append(f"series[{index}] must be an object")
            continue
        series_id = require_nonempty_string(series.get("id"), f"series[{index}].id", errors)
        if series_id in series_by_id:
            errors.append(f"duplicate series id: {series_id}")
            continue
        require_nonempty_string(series.get("metric"), f"series[{index}].metric", errors)
        require_nonempty_string(series.get("class"), f"series[{index}].class", errors)
        require_nonempty_string(series.get("cache_state"), f"series[{index}].cache_state", errors)
        require_nonempty_string(series.get("run_kind"), f"series[{index}].run_kind", errors)
        warmup_runs = series.get("warmup_runs")
        if not isinstance(warmup_runs, int) or isinstance(warmup_runs, bool) or warmup_runs < 0:
            errors.append(f"series[{index}].warmup_runs must be a non-negative integer")
        series_by_id[series_id] = series

    metric_results: dict[str, Any] = {}
    min_samples = policy.get("min_samples_per_series")
    min_samples = min_samples if isinstance(min_samples, int) and min_samples >= 3 else 3
    bindings = policy.get("metric_bindings") if isinstance(policy.get("metric_bindings"), dict) else {}
    for metric, target in CANONICAL_METRICS.items():
        series_id = bindings.get(metric)
        series = series_by_id.get(series_id) if isinstance(series_id, str) else None
        if series is None:
            errors.append(f"{metric}: bound series is missing: {series_id!r}")
            continue
        if series.get("metric") != metric:
            errors.append(f"{metric}: bound series reports metric {series.get('metric')!r}")
            continue
        samples = numeric_samples(series.get("samples_ms"), f"series[{series_id}].samples_ms", errors)
        if len(samples) < min_samples:
            errors.append(
                f"{metric}: {len(samples)} samples are below bound minimum {min_samples}"
            )
            continue
        result: dict[str, float | int | str | bool] = {
            "series_id": series_id,
            "sample_count": len(samples),
        }
        if "p50_ms_max" in target:
            p50 = nearest_rank(samples, 0.50)
            result["p50_ms"] = p50
            result["p50_ms_max"] = target["p50_ms_max"]
            result["p50_pass"] = p50 <= target["p50_ms_max"]
            if p50 > target["p50_ms_max"]:
                errors.append(
                    f"{metric}: p50 {p50:.3f} ms exceeds {target['p50_ms_max']} ms"
                )
        if "p95_ms_max" in target:
            p95 = nearest_rank(samples, 0.95)
            result["p95_ms"] = p95
            result["p95_ms_max"] = target["p95_ms_max"]
            result["p95_pass"] = p95 <= target["p95_ms_max"]
            if p95 > target["p95_ms_max"]:
                errors.append(
                    f"{metric}: p95 {p95:.3f} ms exceeds {target['p95_ms_max']} ms"
                )
        metric_results[metric] = result

    resources = evidence.get("resources")
    if not isinstance(resources, dict):
        errors.append("evidence resources must be an object")
        resources = {}
    resource_results: dict[str, Any] = {}
    budgets = policy.get("resource_budgets") if isinstance(policy.get("resource_budgets"), dict) else {}
    for key in RESOURCE_POLICY_KEYS:
        measured = resources.get(key)
        budget = budgets.get(key)
        if (
            not isinstance(measured, (int, float))
            or isinstance(measured, bool)
            or not math.isfinite(float(measured))
            or float(measured) < 0
        ):
            errors.append(f"resources.{key} must be a finite non-negative number")
            continue
        if (
            not isinstance(budget, (int, float))
            or isinstance(budget, bool)
            or not math.isfinite(float(budget))
            or float(budget) <= 0
        ):
            continue
        passed = float(measured) <= float(budget)
        resource_results[key] = {
            "measured": measured,
            "budget": budget,
            "pass": passed,
        }
        if not passed:
            errors.append(f"resource budget exceeded: {key}={measured} > {budget}")

    return {
        "schema": "dokkomplekt.performance-verdict.v1",
        "result": "PASS" if not errors else "FAIL",
        "targets_sha256": sha256_file(targets_path),
        "reference_policy_sha256": sha256_file(reference_path),
        "reference_id": policy.get("reference_id"),
        "corpus_id": policy.get("corpus_id"),
        "metric_results": metric_results,
        "resource_results": resource_results,
        "errors": errors,
    }


def command_validate_targets(args: argparse.Namespace) -> int:
    data = load_object(Path(args.targets))
    errors = validate_targets(data)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PERFORMANCE TARGET CONTRACT OK: targets only; no measured PASS claimed")
    return 0


def command_validate_reference(args: argparse.Namespace) -> int:
    targets = load_object(Path(args.targets))
    policy = load_object(Path(args.reference))
    errors = validate_targets(targets)
    errors.extend(validate_reference(policy, targets))
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        return 1
    print("PERFORMANCE REFERENCE POLICY OK: machine/corpus/resource budgets are bound")
    return 0


def command_evaluate(args: argparse.Namespace) -> int:
    report = evaluate(Path(args.targets), Path(args.reference), Path(args.evidence))
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.report:
        Path(args.report).parent.mkdir(parents=True, exist_ok=True)
        Path(args.report).write_text(encoded, encoding="utf-8")
    print(encoded, end="")
    return 0 if report["result"] == "PASS" else 1


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description="Evaluate Canon E6 performance evidence")
    sub = root.add_subparsers(dest="command", required=True)

    validate_targets_cmd = sub.add_parser("validate-targets")
    validate_targets_cmd.add_argument(
        "--targets", default="performance/slo-targets.json"
    )
    validate_targets_cmd.set_defaults(func=command_validate_targets)

    validate_reference_cmd = sub.add_parser("validate-reference")
    validate_reference_cmd.add_argument(
        "--targets", default="performance/slo-targets.json"
    )
    validate_reference_cmd.add_argument("--reference", required=True)
    validate_reference_cmd.set_defaults(func=command_validate_reference)

    evaluate_cmd = sub.add_parser("evaluate")
    evaluate_cmd.add_argument(
        "--targets", default="performance/slo-targets.json"
    )
    evaluate_cmd.add_argument("--reference", required=True)
    evaluate_cmd.add_argument("--evidence", required=True)
    evaluate_cmd.add_argument("--report")
    evaluate_cmd.set_defaults(func=command_evaluate)
    return root


def main() -> int:
    args = parser().parse_args()
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
