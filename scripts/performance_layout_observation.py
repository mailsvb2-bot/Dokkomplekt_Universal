#!/usr/bin/env python3
"""Bind one installed-product LayoutProof to Canon E6 runtime_layout coverage."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

from scripts import performance_layout_probe as layout_probe
from scripts import performance_slo_gate as gate

PLAN_SCHEMA = "dokkomplekt.performance-layout-measurement-plan.v1"
OBSERVATION_SCHEMA = "dokkomplekt.performance-layout-observation.v1"
OBSERVATION_CLAIM = "single_sample_runtime_layout_coverage_only_not_slo_verdict"
MEASUREMENT_KIND = "installed_runtime_layout_visual_check"
CLASS_NAME = "runtime_layout"
SAMPLE_DERIVATION = "installed_runtime_layout_check_elapsed"

PLAN_KEYS = {
    "schema", "reference_id", "corpus_id", "series_id", "metric", "class",
    "cache_state", "run_kind", "conditions", "complexity", "warmup",
    "expected_app_version", "expected_source_sha256", "measurement_kind",
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
        and all(ch in "0123456789abcdef" for ch in value)
    )


def _opaque(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 128
        or not all(ch.isascii() and (ch.isalnum() or ch in "-_.") for ch in value)
    ):
        raise ValueError(f"{label} must be an opaque ASCII identifier")
    return value


def _nonnegative(value: Any, label: str) -> float:
    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not math.isfinite(float(value))
        or float(value) < 0
    ):
        raise ValueError(f"{label} must be finite and non-negative")
    return float(value)


def _validate_plan(
    plan: dict[str, Any],
    reference: dict[str, Any],
) -> None:
    if set(plan) != PLAN_KEYS:
        raise ValueError("layout measurement plan keys must be closed")
    if plan.get("schema") != PLAN_SCHEMA:
        raise ValueError(f"layout measurement plan schema must be {PLAN_SCHEMA}")
    if plan.get("reference_id") != reference.get("reference_id"):
        raise ValueError("plan.reference_id differs from bound reference")
    if plan.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError("plan.corpus_id differs from bound reference")
    if plan.get("class") != CLASS_NAME:
        raise ValueError("layout measurement plan.class must be runtime_layout")
    if plan.get("metric") != "button_to_ready":
        raise ValueError("runtime_layout coverage currently requires metric=button_to_ready")
    series_id = _opaque(plan.get("series_id"), "plan.series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict):
        raise ValueError("reference metric_bindings must be an object")
    if bindings.get(plan["metric"]) == series_id:
        raise ValueError("runtime_layout coverage series must remain non-bound")
    if plan.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError("plan.cache_state is not canonical")
    if plan.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError("plan.run_kind is not canonical")
    if not isinstance(plan.get("conditions"), dict):
        raise ValueError("plan.conditions must be an object")
    if plan.get("complexity") is not None:
        raise ValueError("runtime_layout coverage complexity must be null")
    if not isinstance(plan.get("warmup"), bool):
        raise ValueError("plan.warmup must be boolean")
    if plan.get("measurement_kind") != MEASUREMENT_KIND:
        raise ValueError(f"runtime_layout requires measurement_kind={MEASUREMENT_KIND}")
    expected_app = _opaque(plan.get("expected_app_version"), "plan.expected_app_version")
    environment = reference.get("environment")
    if not isinstance(environment, dict) or environment.get("app_version") != expected_app:
        raise ValueError("plan.expected_app_version differs from reference environment")
    source_sha = plan.get("expected_source_sha256")
    if not _is_sha256(source_sha):
        raise ValueError("plan.expected_source_sha256 must be lowercase SHA-256")
    coverage_sources = reference.get("coverage_sources")
    allowed = coverage_sources.get(CLASS_NAME) if isinstance(coverage_sources, dict) else None
    if not isinstance(allowed, list) or source_sha not in allowed:
        raise ValueError("runtime_layout source is not predeclared in bound reference")


def build_observation(
    targets_path: Path,
    reference_path: Path,
    plan_path: Path,
    proof_path: Path,
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
    _validate_plan(plan, reference)
    proof = _load(proof_path)
    layout_probe.validate_proof(proof)
    if proof.get("source_sha256") != plan.get("expected_source_sha256"):
        raise ValueError("LayoutProof source SHA differs from measurement plan")
    if proof.get("app_version") != plan.get("expected_app_version"):
        raise ValueError("LayoutProof app version differs from measurement plan")
    sample_ms = _nonnegative(
        proof.get("layout_check_duration_ms"), "LayoutProof.layout_check_duration_ms"
    )
    return {
        "schema": OBSERVATION_SCHEMA,
        "claim": OBSERVATION_CLAIM,
        "targets_sha256": _sha256(targets_path),
        "reference_policy_sha256": _sha256(reference_path),
        "measurement_plan_sha256": _sha256(plan_path),
        "layout_proof_sha256": _sha256(proof_path),
        "reference_id": reference["reference_id"],
        "corpus_id": reference["corpus_id"],
        "series_id": plan["series_id"],
        "metric": plan["metric"],
        "class": CLASS_NAME,
        "cache_state": plan["cache_state"],
        "run_kind": plan["run_kind"],
        "conditions": plan["conditions"],
        "complexity": None,
        "warmup": plan["warmup"],
        "sample_ms": sample_ms,
        "sample_derivation": SAMPLE_DERIVATION,
        "proof_id": proof["proof_id"],
        "measurement_app_version": proof["app_version"],
        "application_sha256": proof["application_sha256"],
        "source_sha256": proof["source_sha256"],
        "pdf_sha256": proof["pdf_sha256"],
        "converter_sha256": proof["converter_sha256"],
        "converter_version": proof["converter_version"],
        "font_set_sha256": proof["font_set_sha256"],
        "visual_baseline_sha256": proof["visual_baseline_sha256"],
        "layout_os": proof["os"],
        "layout_settings": proof["settings"],
        "visual_verdict": proof["verdict"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Bind LayoutProof to E6 runtime_layout coverage")
    parser.add_argument("--targets", default="performance/slo-targets.json", type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--plan", required=True, type=Path)
    parser.add_argument("--proof", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        observation = build_observation(
            args.targets, args.reference, args.plan, args.proof
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"RUNTIME LAYOUT OBSERVATION FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(observation, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "RUNTIME LAYOUT OBSERVATION OK: "
        f"sample_ms={observation['sample_ms']:.3f}; coverage only, no SLO verdict"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
