#!/usr/bin/env python3
"""Bind verified storage-condition timings to Canon E6 coverage.

This producer is intentionally limited to the non-bound slow_storage and
network_storage coverage classes. A source SHA alone cannot prove storage
conditions, so measurements must come from an installed hardware harness with
a class-specific verification method.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

from scripts import performance_slo_gate as gate
from scripts import performance_storage_probe as storage_probe

PLAN_SCHEMA = "dokkomplekt.performance-storage-measurement-plan.v1"
MEASUREMENT_SCHEMA = "dokkomplekt.performance-storage-measurement.v1"
OBSERVATION_SCHEMA = "dokkomplekt.performance-storage-observation.v1"
OBSERVATION_CLAIM = "single_sample_storage_coverage_only_not_slo_verdict"
MEASUREMENT_KIND = "storage_condition_end_to_end"
SUPPORTED_CLASSES = {
    "slow_storage": "controlled_io_throttle_probe",
    "network_storage": "windows_unc_path_probe",
}
ALLOWED_INSTRUMENTS = {
    "hardware_harness_monotonic_clock",
    "windows_etw_monotonic_clock",
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
    "expected_source_sha256",
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
    "source_sha256",
}


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _is_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def _exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    actual = set(value)
    if actual != expected:
        raise ValueError(
            f"{label} keys must be closed: missing={sorted(expected - actual)} "
            f"unexpected={sorted(actual - expected)}"
        )


def _opaque_identifier(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 128
        or not all(
            character.isascii() and (character.isalnum() or character in "-_.")
            for character in value
        )
    ):
        raise ValueError(f"{label} must be an opaque ASCII identifier")
    return value


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

    metric = plan.get("metric")
    if metric != "button_to_ready":
        raise ValueError("storage coverage currently requires metric=button_to_ready")
    series_id = _opaque_identifier(plan.get("series_id"), "plan.series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict):
        raise ValueError("reference metric_bindings must be an object")
    if bindings.get(metric) == series_id:
        raise ValueError("storage coverage series must remain non-bound")

    class_name = plan.get("class")
    if class_name not in SUPPORTED_CLASSES:
        raise ValueError("plan.class must be slow_storage or network_storage")
    if plan.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError("plan.cache_state is not canonical")
    if plan.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError("plan.run_kind is not canonical")
    if not isinstance(plan.get("conditions"), dict):
        raise ValueError("plan.conditions must be an object")
    if plan.get("complexity") is not None:
        raise ValueError("storage coverage complexity must be null")
    if not isinstance(plan.get("warmup"), bool):
        raise ValueError("plan.warmup must be boolean")
    if plan.get("measurement_kind") != MEASUREMENT_KIND:
        raise ValueError(f"storage coverage requires measurement_kind={MEASUREMENT_KIND}")

    expected_app_version = _opaque_identifier(
        plan.get("expected_app_version"), "plan.expected_app_version"
    )
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or environment.get("app_version") != expected_app_version
    ):
        raise ValueError("plan.expected_app_version must equal reference environment app_version")

    source_sha256 = plan.get("expected_source_sha256")
    if not _is_sha256(source_sha256):
        raise ValueError("plan.expected_source_sha256 must be a lowercase SHA-256 digest")
    coverage_sources = reference.get("coverage_sources")
    allowed_sources = (
        coverage_sources.get(class_name)
        if isinstance(coverage_sources, dict)
        else None
    )
    if not isinstance(allowed_sources, list) or source_sha256 not in allowed_sources:
        raise ValueError("plan source is not predeclared for this storage coverage class")


def _validate_measurement(measurement: dict[str, Any], plan: dict[str, Any]) -> str:
    _exact_keys(measurement, MEASUREMENT_KEYS, "measurement")
    if measurement.get("schema") != MEASUREMENT_SCHEMA:
        raise ValueError(f"measurement schema must be {MEASUREMENT_SCHEMA}")
    measurement_id = _opaque_identifier(
        measurement.get("measurement_id"), "measurement.measurement_id"
    )
    for key in ("app_version", "metric", "measurement_kind", "conditions"):
        expected_key = "expected_app_version" if key == "app_version" else key
        if measurement.get(key) != plan.get(expected_key):
            raise ValueError(f"measurement {key} does not match the predeclared plan")
    if measurement.get("source_sha256") != plan.get("expected_source_sha256"):
        raise ValueError("measurement source_sha256 does not match the predeclared plan")
    if measurement.get("instrument") not in ALLOWED_INSTRUMENTS:
        raise ValueError("measurement instrument is not an approved hardware timing source")
    if measurement.get("installed_build") is not True:
        raise ValueError("storage coverage requires an installed build")
    _nonnegative_number(measurement.get("sample_ms"), "measurement.sample_ms")
    return measurement_id


def build_observation(
    targets_path: Path,
    reference_path: Path,
    plan_path: Path,
    measurement_path: Path,
    probe_path: Path,
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
    probe = _load(probe_path)
    storage_probe.validate_probe(probe)
    if probe.get("storage_class") != plan.get("class"):
        raise ValueError("probe storage_class does not match the predeclared plan")
    if probe.get("source_sha256") != plan.get("expected_source_sha256"):
        raise ValueError("probe source_sha256 does not match the predeclared plan")
    if measurement.get("source_sha256") != probe.get("source_sha256"):
        raise ValueError("measurement source_sha256 does not match probe source_sha256")

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
        "complexity": None,
        "warmup": plan["warmup"],
        "sample_ms": float(measurement["sample_ms"]),
        "sample_derivation": "direct_storage_condition_measurement",
        "measurement_id": measurement_id,
        "measurement_app_version": measurement["app_version"],
        "measurement_kind": measurement["measurement_kind"],
        "measurement_instrument": measurement["instrument"],
        "installed_build": True,
        "source_sha256": measurement["source_sha256"],
        "storage_class": probe["storage_class"],
        "probe_sha256": _sha256(probe_path),
        "probe_producer": probe["producer"],
        "probe_verification": probe["verification"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Bind one verified storage-condition timing to Canon E6 coverage"
    )
    parser.add_argument("--targets", default="performance/slo-targets.json", type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--measurement", required=True, type=Path)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        observation = build_observation(
            args.targets, args.reference, args.plan, args.measurement, args.probe
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"STORAGE PERFORMANCE OBSERVATION FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(observation, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "STORAGE PERFORMANCE OBSERVATION OK: "
        f"{observation['class']} sample_ms={observation['sample_ms']:.3f}; "
        "coverage only, no SLO verdict"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
