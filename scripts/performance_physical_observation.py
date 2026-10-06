#!/usr/bin/env python3
"""Bind one physical/UI timing measurement to one predeclared Canon E6 plan.

The producer accepts direct monotonic-clock measurements from an installed
reference-machine harness. It never derives these UI/startup metrics from
runtime traces and never emits an SLO verdict.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

from scripts import performance_slo_gate as gate

PLAN_SCHEMA = "dokkomplekt.performance-physical-measurement-plan.v1"
MEASUREMENT_SCHEMA = "dokkomplekt.performance-physical-measurement.v1"
OBSERVATION_SCHEMA = "dokkomplekt.performance-physical-observation.v1"
OBSERVATION_CLAIM = "single_sample_only_not_slo_verdict"

PHYSICAL_METRICS = {
    "cold_start_to_interactive_ui": "process_start_to_interactive_ui",
    "visible_action_response": "visible_action_to_ui_response",
    "prompt_form_ready": "prompt_plan_to_form_ready",
}
ALLOWED_INSTRUMENTS = {
    "hardware_harness_monotonic_clock",
    "windows_etw_monotonic_clock",
    "uia_monotonic_clock",
}

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
    "measurement_kind",
}
MEASUREMENT_KEYS = {
    "schema",
    "measurement_id",
    "app_version",
    "metric",
    "measurement_kind",
    "sample_ms",
    "conditions",
    "instrument",
    "installed_build",
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


def _opaque_identifier(value: Any, label: str) -> str:
    text = _nonempty_string(value, label)
    if len(text) > 128 or not all(
        character.isascii() and (character.isalnum() or character in "-_.")
        for character in text
    ):
        raise ValueError(f"{label} must be an opaque ASCII identifier")
    return text


def _nonnegative_number(value: Any, label: str) -> float:
    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not math.isfinite(float(value))
        or float(value) < 0
    ):
        raise ValueError(f"{label} must be a finite non-negative number")
    return float(value)


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
    expected_kind = PHYSICAL_METRICS.get(metric)
    if expected_kind is None:
        raise ValueError(
            f"{metric} is not a physical/UI metric; use the runtime-trace observation producer"
        )
    if plan.get("measurement_kind") != expected_kind:
        raise ValueError(f"{metric} requires measurement_kind={expected_kind}")

    series_id = _nonempty_string(plan.get("series_id"), "plan.series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict) or bindings.get(metric) != series_id:
        raise ValueError("plan.series_id is not the reference-bound series for this metric")

    class_name = plan.get("class")
    allowed_classes = set(gate.CANONICAL_CORPUS_CLASSES) | set(gate.CANONICAL_SPECIAL_CLASSES)
    if class_name not in allowed_classes:
        raise ValueError("plan.class is not a Canon performance class")
    if plan.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError("plan.cache_state is not canonical")
    if plan.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError("plan.run_kind is not canonical")
    if not isinstance(plan.get("conditions"), dict):
        raise ValueError("plan.conditions must be an object")
    complexity = plan.get("complexity")
    if complexity is not None and not isinstance(complexity, dict):
        raise ValueError("plan.complexity must be an object or null")
    if not isinstance(plan.get("warmup"), bool):
        raise ValueError("plan.warmup must be boolean")

    expected_app_version = _opaque_identifier(
        plan.get("expected_app_version"), "plan.expected_app_version"
    )
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or environment.get("app_version") != expected_app_version
    ):
        raise ValueError("plan.expected_app_version must equal reference environment app_version")

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
        if plan["conditions"].get(flag) is not expected:
            raise ValueError(f"{metric} requires conditions.{flag}={expected!r}")


def _validate_measurement(measurement: dict[str, Any], plan: dict[str, Any]) -> str:
    _exact_keys(measurement, MEASUREMENT_KEYS, "measurement")
    if measurement.get("schema") != MEASUREMENT_SCHEMA:
        raise ValueError(f"measurement schema must be {MEASUREMENT_SCHEMA}")
    measurement_id = _opaque_identifier(
        measurement.get("measurement_id"), "measurement.measurement_id"
    )
    if measurement.get("app_version") != plan.get("expected_app_version"):
        raise ValueError("measurement app_version does not match the predeclared plan")
    if measurement.get("metric") != plan.get("metric"):
        raise ValueError("measurement metric does not match the predeclared plan")
    if measurement.get("measurement_kind") != plan.get("measurement_kind"):
        raise ValueError("measurement kind does not match the predeclared plan")
    if measurement.get("conditions") != plan.get("conditions"):
        raise ValueError("measurement conditions do not exactly match the predeclared plan")
    if measurement.get("instrument") not in ALLOWED_INSTRUMENTS:
        raise ValueError("measurement instrument is not an approved monotonic-clock harness")
    if measurement.get("installed_build") is not True:
        raise ValueError("physical/UI SLO measurements require an installed build")
    _nonnegative_number(measurement.get("sample_ms"), "measurement.sample_ms")
    return measurement_id


def build_observation(
    targets_path: Path,
    reference_path: Path,
    plan_path: Path,
    measurement_path: Path,
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
    measurement = _load(measurement_path)
    measurement_id = _validate_measurement(measurement, plan)

    return {
        "schema": OBSERVATION_SCHEMA,
        "claim": OBSERVATION_CLAIM,
        "targets_sha256": _sha256(targets_path),
        "reference_policy_sha256": _sha256(reference_path),
        "measurement_plan_sha256": _sha256(plan_path),
        "measurement_sha256": _sha256(measurement_path),
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
        "sample_ms": float(measurement["sample_ms"]),
        "sample_derivation": "direct_physical_measurement:" + plan["measurement_kind"],
        "measurement_id": measurement_id,
        "measurement_app_version": measurement["app_version"],
        "measurement_kind": measurement["measurement_kind"],
        "measurement_instrument": measurement["instrument"],
        "installed_build": True,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Bind one installed physical/UI timing to one E6 measurement plan"
    )
    parser.add_argument("--targets", required=True, type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--measurement", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        observation = build_observation(
            args.targets, args.reference, args.plan, args.measurement
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"PHYSICAL PERFORMANCE OBSERVATION FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(observation, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "PHYSICAL PERFORMANCE OBSERVATION OK: "
        f"{observation['metric']}={observation['sample_ms']:.3f} ms; "
        "single sample only, no SLO verdict"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
