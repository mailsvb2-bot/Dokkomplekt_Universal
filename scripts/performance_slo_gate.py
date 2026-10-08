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

CANONICAL_METRICS: dict[str, dict[str, int | str]] = {
    "source_analysis": {
        "p50_ms_max": 250,
        "p95_ms_max": 700,
        "conditions": "typical_source_docx_without_ocr",
    },
    "preflight_calculation": {
        "p50_ms_max": 100,
        "p95_ms_max": 300,
        "conditions": "human_response_time_excluded",
    },
    "render_readback_verify": {
        "p50_ms_max": 500,
        "p95_ms_max": 1200,
        "conditions": "typical_single_docx_without_runtime_layout",
    },
    "button_to_ready": {
        "p50_ms_max": 1000,
        "p95_ms_max": 2000,
        "conditions": "single_ordinary_result_no_questions_cache_state_recorded",
    },
    "large_docx_to_ready": {
        "p95_ms_max": 5000,
        "conditions": "approximately_up_to_25mb_with_recorded_structural_complexity",
    },
    "cold_start_to_interactive_ui": {
        "p50_ms_max": 1500,
        "p95_ms_max": 3000,
        "conditions": "clean_start_of_declared_installed_configuration",
    },
    "visible_action_response": {
        "p95_ms_max": 100,
        "conditions": "ui_budget_long_work_runs_in_backend",
    },
    "prompt_form_ready": {
        "p95_ms_max": 150,
        "conditions": "prompt_plan_already_calculated",
    },
}

CANONICAL_COMPLEXITY_DIMENSIONS = (
    "uncompressed_bytes",
    "xml_nodes",
    "stories",
    "tables",
    "images",
    "operation_count",
    "text_lengths",
    "collection_cardinality",
)
CANONICAL_ENVIRONMENT_FIELDS = (
    "cpu",
    "ram_bytes",
    "storage_filesystem",
    "os_build",
    "power_mode",
    "scanner_antivirus_environment",
    "app_version",
    "fonts_layout_engine",
)
CANONICAL_CACHE_STATES = ("cold_cache", "warm_cache")
CANONICAL_RUN_KINDS = (
    "first_run",
    "repeat_run",
    "single_document",
    "batch_10",
    "batch_50",
)
CANONICAL_MEASUREMENT_SPLITS = CANONICAL_CACHE_STATES + CANONICAL_RUN_KINDS
CANONICAL_CORPUS_CLASSES = (
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
)
CANONICAL_SPECIAL_CLASSES = (
    "ocr",
    "runtime_layout",
    "pdf",
    "slow_storage",
    "network_storage",
)
CANONICAL_MEASUREMENT_RULES = (
    "record_sample_count_and_warmup_rules",
    "never_derive_end_to_end_p95_by_summing_stage_p95_values",
    "report_full_drop_to_ready_and_click_to_ready_separately_when_preanalysis_exists",
    "exclude_human_wait_time_from_machine_time",
    "do_not_mix_ocr_runtime_layout_pdf_or_slow_network_storage_into_typical_docx_class",
    "verification_must_remain_enabled_during_measurement",
    "target_miss_must_not_be_reported_as_pass",
)

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


# Machine-readable interpretation of the immutable Canon condition labels above.
# These rules apply to the *bound* series used to claim each SLO, not merely to
# aggregate corpus coverage supplied by unrelated filler series.
SLO_SERIES_REQUIREMENTS: dict[str, dict[str, Any]] = {
    "source_analysis": {
        "class": "typical_docx",
        "condition_flags": {"ocr": False},
    },
    "preflight_calculation": {
        "condition_flags": {"human_wait_excluded": True},
    },
    "render_readback_verify": {
        "class": "typical_docx",
        "run_kind": "single_document",
        "condition_flags": {"runtime_layout": False},
    },
    "button_to_ready": {
        "class": "typical_docx",
        "run_kind": "single_document",
        "condition_flags": {"questions_present": False},
    },
    "large_docx_to_ready": {
        "class": "large_docx",
        "run_kind": "single_document",
        "require_complexity": True,
    },
    "cold_start_to_interactive_ui": {
        "cache_state": "cold_cache",
        "run_kind": "first_run",
        "condition_flags": {"installed_configuration": True},
    },
    "visible_action_response": {
        "condition_flags": {"long_work_backend": True},
    },
    "prompt_form_ready": {
        "condition_flags": {"prompt_plan_precalculated": True},
    },
}

LARGE_DOCX_COMPRESSED_BYTES_MAX = 25 * 1024 * 1024


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
        if actual != expected:
            errors.append(f"{metric}: target contract drifted from Canon")

    typical = data.get("typical_docx")
    if not isinstance(typical, dict):
        errors.append("typical_docx must be an object")
    else:
        if typical.get("compressed_bytes_guideline_max") != 5 * 1024 * 1024:
            errors.append("typical_docx compressed guideline must be 5 MiB")
        if tuple(typical.get("complexity_dimensions") or ()) != CANONICAL_COMPLEXITY_DIMENSIONS:
            errors.append("typical_docx complexity dimensions drifted from Canon")

    exact_arrays = {
        "required_environment_fields": CANONICAL_ENVIRONMENT_FIELDS,
        "required_measurement_splits": CANONICAL_MEASUREMENT_SPLITS,
        "required_resource_measurements": RESOURCE_POLICY_KEYS,
        "required_corpus_classes": CANONICAL_CORPUS_CLASSES,
        "separate_performance_classes": CANONICAL_SPECIAL_CLASSES,
        "measurement_rules": CANONICAL_MEASUREMENT_RULES,
    }
    for key, expected in exact_arrays.items():
        actual = data.get(key)
        if not isinstance(actual, list) or tuple(actual) != expected:
            errors.append(f"{key} drifted from the Canon performance contract")
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
        metric = require_nonempty_string(
            series.get("metric"), f"series[{index}].metric", errors
        )
        if metric and metric not in CANONICAL_METRICS:
            errors.append(f"series[{index}].metric is not a Canon SLO metric: {metric}")
        class_name = require_nonempty_string(
            series.get("class"), f"series[{index}].class", errors
        )
        allowed_series_classes = (
            set(CANONICAL_CORPUS_CLASSES)
            | set(CANONICAL_SPECIAL_CLASSES)
            | {"unclassified"}
        )
        if class_name and class_name not in allowed_series_classes:
            errors.append(f"series[{index}].class is not a Canon class: {class_name}")
        cache_state = require_nonempty_string(
            series.get("cache_state"), f"series[{index}].cache_state", errors
        )
        if cache_state and cache_state not in CANONICAL_CACHE_STATES:
            errors.append(
                f"series[{index}].cache_state is not canonical: {cache_state}"
            )
        run_kind = require_nonempty_string(
            series.get("run_kind"), f"series[{index}].run_kind", errors
        )
        if run_kind and run_kind not in CANONICAL_RUN_KINDS:
            errors.append(f"series[{index}].run_kind is not canonical: {run_kind}")
        conditions = series.get("conditions")
        if conditions is not None and not isinstance(conditions, dict):
            errors.append(f"series[{index}].conditions must be an object when present")
        complexity = series.get("complexity")
        if complexity is not None and not isinstance(complexity, dict):
            errors.append(f"series[{index}].complexity must be an object when present")
        warmup_runs = series.get("warmup_runs")
        if not isinstance(warmup_runs, int) or isinstance(warmup_runs, bool) or warmup_runs < 0:
            errors.append(f"series[{index}].warmup_runs must be a non-negative integer")
        series_by_id[series_id] = series

    metric_results: dict[str, Any] = {}
    min_samples = policy.get("min_samples_per_series")
    min_samples = min_samples if isinstance(min_samples, int) and min_samples >= 3 else 3

    measured_classes: set[str] = set()
    measured_cache_states: set[str] = set()
    measured_run_kinds: set[str] = set()
    for series_id, series in series_by_id.items():
        samples = numeric_samples(
            series.get("samples_ms"), f"series[{series_id}].samples_ms", errors
        )
        if len(samples) < min_samples:
            errors.append(
                f"series[{series_id}]: {len(samples)} samples are below bound minimum {min_samples}"
            )
            continue
        series_class = str(series.get("class", "")).strip()
        if series_class != "unclassified":
            measured_classes.add(series_class)
        measured_cache_states.add(str(series.get("cache_state", "")).strip())
        measured_run_kinds.add(str(series.get("run_kind", "")).strip())

    missing_classes = sorted(
        (set(CANONICAL_CORPUS_CLASSES) | set(CANONICAL_SPECIAL_CLASSES))
        - measured_classes
    )
    if missing_classes:
        errors.append(f"measured corpus/class coverage is incomplete; missing={missing_classes}")
    missing_cache_states = sorted(set(CANONICAL_CACHE_STATES) - measured_cache_states)
    if missing_cache_states:
        errors.append(
            f"measured cache-state coverage is incomplete; missing={missing_cache_states}"
        )
    missing_run_kinds = sorted(set(CANONICAL_RUN_KINDS) - measured_run_kinds)
    if missing_run_kinds:
        errors.append(f"measured run-kind coverage is incomplete; missing={missing_run_kinds}")
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

        requirements = SLO_SERIES_REQUIREMENTS[metric]
        required_class = requirements.get("class")
        if required_class is not None and series.get("class") != required_class:
            errors.append(
                f"{metric}: bound series class {series.get('class')!r} "
                f"does not satisfy {target['conditions']!r}; expected {required_class!r}"
            )
        required_cache_state = requirements.get("cache_state")
        if required_cache_state is not None and series.get("cache_state") != required_cache_state:
            errors.append(
                f"{metric}: bound series cache_state {series.get('cache_state')!r} "
                f"does not satisfy {target['conditions']!r}; expected {required_cache_state!r}"
            )
        required_run_kind = requirements.get("run_kind")
        if required_run_kind is not None and series.get("run_kind") != required_run_kind:
            errors.append(
                f"{metric}: bound series run_kind {series.get('run_kind')!r} "
                f"does not satisfy {target['conditions']!r}; expected {required_run_kind!r}"
            )
        condition_flags = requirements.get("condition_flags", {})
        series_conditions = series.get("conditions")
        if not isinstance(series_conditions, dict):
            series_conditions = {}
        for flag, expected in condition_flags.items():
            if series_conditions.get(flag) is not expected:
                errors.append(
                    f"{metric}: conditions.{flag} must be {expected!r} "
                    f"for {target['conditions']!r}"
                )

        if requirements.get("require_complexity") is True:
            complexity = series.get("complexity")
            if not isinstance(complexity, dict):
                errors.append(
                    f"{metric}: structural complexity metadata is required "
                    f"for {target['conditions']!r}"
                )
            else:
                compressed_bytes = complexity.get("compressed_bytes")
                if (
                    not isinstance(compressed_bytes, (int, float))
                    or isinstance(compressed_bytes, bool)
                    or not math.isfinite(float(compressed_bytes))
                    or float(compressed_bytes) <= 0
                    or float(compressed_bytes) > LARGE_DOCX_COMPRESSED_BYTES_MAX
                ):
                    errors.append(
                        f"{metric}: complexity.compressed_bytes must be in "
                        f"(0, {LARGE_DOCX_COMPRESSED_BYTES_MAX}]"
                    )
                for dimension in CANONICAL_COMPLEXITY_DIMENSIONS:
                    value = complexity.get(dimension)
                    if (
                        not isinstance(value, (int, float))
                        or isinstance(value, bool)
                        or not math.isfinite(float(value))
                        or float(value) < 0
                    ):
                        errors.append(
                            f"{metric}: complexity.{dimension} must be a finite "
                            "non-negative number"
                        )

        samples = numeric_samples(
            series.get("samples_ms"), f"series[{series_id}].samples_ms", errors
        )
        if len(samples) < min_samples:
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
