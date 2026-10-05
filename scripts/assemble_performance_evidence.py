#!/usr/bin/env python3
"""Assemble Canon E6 evidence only from hash-bound typed observations.

Raw traces and metric extractors are deliberately out of scope here. Metric
derivation belongs to performance_trace_observation.py; the existing
performance_slo_gate.py remains the only SLO verdict authority.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from typing import Any

from scripts import performance_slo_gate as gate
from scripts import performance_trace_observation as trace_obs

PROTOCOL_SCHEMA = "dokkomplekt.performance-protocol.v1"
RESOURCE_SCHEMA = "dokkomplekt.performance-resource-snapshot.v1"
EVIDENCE_CLAIM = "assembled_observations_not_slo_verdict"

OBSERVATION_KEYS = {
    "schema", "claim", "targets_sha256", "reference_policy_sha256",
    "measurement_plan_sha256", "trace_sha256", "reference_id", "corpus_id",
    "series_id", "metric", "class", "cache_state", "run_kind", "conditions",
    "complexity", "warmup", "sample_ms", "sample_derivation", "trace_run_id",
    "trace_app_version", "trace_workload", "trace_batch_size",
    "trace_feature_flags",
}
TRACE_FEATURE_KEYS = {"ocr_used", "runtime_layout_used", "pdf_used"}
PROTOCOL_KEYS = {"schema", "flags"}
RESOURCE_KEYS = {
    "schema", "targets_sha256", "reference_policy_sha256", "reference_id",
    "corpus_id", "environment", "resources",
}

EXPECTED_DERIVATIONS = {
    "source_analysis":
        "per_run_stage_sum:source_open+source_parse+candidate_index+source_resolve",
    "preflight_calculation": "per_run_stage_sum:prompt_plan+preflight",
    "render_readback_verify":
        "per_run_stage_sum:replay+physical_readback+verify",
    "button_to_ready": "end_to_end_ms_minus_human_wait_ms",
    "large_docx_to_ready": "end_to_end_ms_minus_human_wait_ms",
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


def _nonnegative_number(value: Any, label: str) -> float:
    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not math.isfinite(float(value))
        or float(value) < 0
    ):
        raise ValueError(f"{label} must be a finite non-negative number")
    return float(value)


def _validate_protocol(path: Path) -> dict[str, bool]:
    document = _load(path)
    if set(document) != PROTOCOL_KEYS or document.get("schema") != PROTOCOL_SCHEMA:
        raise ValueError(f"protocol must use closed schema {PROTOCOL_SCHEMA}")
    flags = document.get("flags")
    if not isinstance(flags, dict) or set(flags) != set(gate.REQUIRED_PROTOCOL_FLAGS):
        raise ValueError("protocol flags must define exactly the canonical protocol flags")
    for flag in gate.REQUIRED_PROTOCOL_FLAGS:
        if flags.get(flag) is not True:
            raise ValueError(f"protocol.{flag} must be true")
    return {flag: True for flag in gate.REQUIRED_PROTOCOL_FLAGS}


def _validate_resources(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, float | int]:
    document = _load(path)
    if set(document) != RESOURCE_KEYS or document.get("schema") != RESOURCE_SCHEMA:
        raise ValueError(f"resource snapshot must use closed schema {RESOURCE_SCHEMA}")
    if document.get("targets_sha256") != targets_sha256:
        raise ValueError("resource snapshot is not bound to the exact target contract")
    if document.get("reference_policy_sha256") != reference_sha256:
        raise ValueError("resource snapshot is not bound to the exact reference policy")
    if document.get("reference_id") != reference.get("reference_id"):
        raise ValueError("resource snapshot reference_id mismatch")
    if document.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError("resource snapshot corpus_id mismatch")
    if document.get("environment") != reference.get("environment"):
        raise ValueError("resource snapshot environment differs from the bound reference machine")
    resources = document.get("resources")
    if not isinstance(resources, dict) or set(resources) != set(gate.RESOURCE_POLICY_KEYS):
        raise ValueError("resource snapshot must define exactly the canonical resource keys")
    normalized: dict[str, float | int] = {}
    for key in gate.RESOURCE_POLICY_KEYS:
        value = resources[key]
        _nonnegative_number(value, f"resources.{key}")
        normalized[key] = value
    return normalized


def _validate_observation(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    if set(value) != OBSERVATION_KEYS:
        raise ValueError(f"{path}: observation keys must be closed")
    if value.get("schema") != trace_obs.OBSERVATION_SCHEMA:
        raise ValueError(f"{path}: unsupported observation schema")
    if value.get("claim") != trace_obs.OBSERVATION_CLAIM:
        raise ValueError(f"{path}: observation claim is not single-sample-only")
    if value.get("targets_sha256") != targets_sha256:
        raise ValueError(f"{path}: target hash mismatch")
    if value.get("reference_policy_sha256") != reference_sha256:
        raise ValueError(f"{path}: reference hash mismatch")
    if value.get("reference_id") != reference.get("reference_id"):
        raise ValueError(f"{path}: reference_id mismatch")
    if value.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError(f"{path}: corpus_id mismatch")

    metric = value.get("metric")
    if metric not in EXPECTED_DERIVATIONS:
        raise ValueError(f"{path}: metric is not produced by the typed runtime observation layer")
    series_id = value.get("series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict) or bindings.get(metric) != series_id:
        raise ValueError(f"{path}: series_id is not reference-bound for metric")
    if value.get("sample_derivation") != EXPECTED_DERIVATIONS[metric]:
        raise ValueError(f"{path}: sample derivation does not match canonical metric")

    class_name = value.get("class")
    allowed_classes = set(gate.CANONICAL_CORPUS_CLASSES) | set(gate.CANONICAL_SPECIAL_CLASSES)
    if class_name not in allowed_classes:
        raise ValueError(f"{path}: non-canonical performance class")
    if value.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError(f"{path}: non-canonical cache_state")
    if value.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError(f"{path}: non-canonical run_kind")
    if not isinstance(value.get("conditions"), dict):
        raise ValueError(f"{path}: conditions must be an object")
    complexity = value.get("complexity")
    if complexity is not None and not isinstance(complexity, dict):
        raise ValueError(f"{path}: complexity must be an object or null")
    if not isinstance(value.get("warmup"), bool):
        raise ValueError(f"{path}: warmup must be boolean")
    _nonnegative_number(value.get("sample_ms"), f"{path}: sample_ms")

    for key in ("measurement_plan_sha256", "trace_sha256"):
        if not _is_sha256(value.get(key)):
            raise ValueError(f"{path}: {key} must be lowercase SHA-256")
    run_id = value.get("trace_run_id")
    if not isinstance(run_id, str) or not run_id:
        raise ValueError(f"{path}: trace_run_id must be non-empty")
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or value.get("trace_app_version") != environment.get("app_version")
    ):
        raise ValueError(f"{path}: trace app version differs from bound reference")

    feature_flags = value.get("trace_feature_flags")
    if not isinstance(feature_flags, dict) or set(feature_flags) != TRACE_FEATURE_KEYS:
        raise ValueError(f"{path}: trace_feature_flags must be closed")
    if any(not isinstance(feature_flags[key], bool) for key in TRACE_FEATURE_KEYS):
        raise ValueError(f"{path}: trace feature flags must be boolean")
    if class_name == "typical_docx" and any(feature_flags.values()):
        raise ValueError(f"{path}: typical_docx cannot contain OCR/runtime-layout/PDF work")
    special_flag = {
        "ocr": "ocr_used",
        "runtime_layout": "runtime_layout_used",
        "pdf": "pdf_used",
    }.get(class_name)
    if special_flag is not None and feature_flags[special_flag] is not True:
        raise ValueError(f"{path}: {class_name} observation lacks required feature flag")

    requirements = gate.SLO_SERIES_REQUIREMENTS[metric]
    if requirements.get("class") is not None and class_name != requirements["class"]:
        raise ValueError(f"{path}: observation class violates metric binding")
    if (
        requirements.get("cache_state") is not None
        and value.get("cache_state") != requirements["cache_state"]
    ):
        raise ValueError(f"{path}: observation cache_state violates metric binding")
    if (
        requirements.get("run_kind") is not None
        and value.get("run_kind") != requirements["run_kind"]
    ):
        raise ValueError(f"{path}: observation run_kind violates metric binding")
    conditions = value["conditions"]
    for flag, expected in requirements.get("condition_flags", {}).items():
        if conditions.get(flag) is not expected:
            raise ValueError(f"{path}: conditions.{flag} violates metric binding")
    if requirements.get("require_complexity") and not isinstance(complexity, dict):
        raise ValueError(f"{path}: structural complexity is required")
    return value


def _series_signature(observation: dict[str, Any]) -> tuple[Any, ...]:
    return (
        observation["metric"],
        observation["class"],
        observation["cache_state"],
        observation["run_kind"],
        json.dumps(observation["conditions"], sort_keys=True, ensure_ascii=False),
        json.dumps(observation["complexity"], sort_keys=True, ensure_ascii=False),
        observation["sample_derivation"],
    )


def build_evidence(
    targets_path: Path,
    reference_path: Path,
    observations_dir: Path,
    protocol_path: Path,
    resources_path: Path,
) -> dict[str, Any]:
    targets = gate.load_object(targets_path)
    target_errors = gate.validate_targets(targets)
    if target_errors:
        raise ValueError("invalid targets: " + "; ".join(target_errors))
    reference = gate.load_object(reference_path)
    reference_errors = gate.validate_reference(reference, targets)
    if reference_errors:
        raise ValueError("invalid reference policy: " + "; ".join(reference_errors))

    targets_sha256 = _sha256(targets_path)
    reference_sha256 = _sha256(reference_path)
    protocol = _validate_protocol(protocol_path)
    resources = _validate_resources(
        resources_path,
        targets_sha256=targets_sha256,
        reference_sha256=reference_sha256,
        reference=reference,
    )

    if not observations_dir.is_dir():
        raise ValueError("observations path must be a directory")
    paths = sorted(path for path in observations_dir.glob("*.json") if path.is_file())
    if not paths:
        raise ValueError("observation directory is empty")

    groups: dict[str, dict[str, Any]] = {}
    used_trace_hashes: set[str] = set()
    used_run_ids: set[str] = set()
    manifest: list[dict[str, str]] = []
    for path in paths:
        observation = _validate_observation(
            path,
            targets_sha256=targets_sha256,
            reference_sha256=reference_sha256,
            reference=reference,
        )
        trace_hash = observation["trace_sha256"]
        run_id = observation["trace_run_id"]
        if trace_hash in used_trace_hashes:
            raise ValueError(f"trace_sha256 reused by multiple observations: {trace_hash}")
        if run_id in used_run_ids:
            raise ValueError(f"trace_run_id reused by multiple observations: {run_id}")
        used_trace_hashes.add(trace_hash)
        used_run_ids.add(run_id)

        series_id = observation["series_id"]
        signature = _series_signature(observation)
        group = groups.get(series_id)
        if group is None:
            group = {
                "signature": signature,
                "metric": observation["metric"],
                "class": observation["class"],
                "cache_state": observation["cache_state"],
                "run_kind": observation["run_kind"],
                "conditions": observation["conditions"],
                "complexity": observation["complexity"],
                "warmup_runs": 0,
                "samples_ms": [],
            }
            groups[series_id] = group
        elif group["signature"] != signature:
            raise ValueError(f"series metadata drift within {series_id}")

        if observation["warmup"]:
            group["warmup_runs"] += 1
        else:
            group["samples_ms"].append(float(observation["sample_ms"]))

        manifest.append(
            {
                "observation_sha256": _sha256(path),
                "trace_run_id": run_id,
                "trace_sha256": trace_hash,
                "measurement_plan_sha256": observation["measurement_plan_sha256"],
            }
        )

    series: list[dict[str, Any]] = []
    for series_id in sorted(groups):
        group = groups[series_id]
        if not group["samples_ms"]:
            raise ValueError(f"series {series_id} contains only warmup observations")
        row = {
            "id": series_id,
            "metric": group["metric"],
            "class": group["class"],
            "cache_state": group["cache_state"],
            "run_kind": group["run_kind"],
            "conditions": group["conditions"],
            "warmup_runs": group["warmup_runs"],
            "samples_ms": group["samples_ms"],
        }
        if group["complexity"] is not None:
            row["complexity"] = group["complexity"]
        series.append(row)

    return {
        "schema": gate.EVIDENCE_SCHEMA,
        "claim": EVIDENCE_CLAIM,
        "targets_sha256": targets_sha256,
        "reference_policy_sha256": reference_sha256,
        "reference_id": reference["reference_id"],
        "corpus_id": reference["corpus_id"],
        "environment": reference["environment"],
        "protocol": protocol,
        "series": series,
        "resources": resources,
        "protocol_snapshot_sha256": _sha256(protocol_path),
        "resource_snapshot_sha256": _sha256(resources_path),
        "source_observations": manifest,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Assemble partial Canon E6 evidence from typed observations"
    )
    parser.add_argument("--targets", default="performance/slo-targets.json", type=Path)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--observations", required=True, type=Path)
    parser.add_argument("--protocol", required=True, type=Path)
    parser.add_argument("--resources", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        evidence = build_evidence(
            args.targets,
            args.reference,
            args.observations,
            args.protocol,
            args.resources,
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"PERFORMANCE EVIDENCE ASSEMBLY FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(evidence, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "PERFORMANCE EVIDENCE ASSEMBLED: "
        f"series={len(evidence['series'])}; "
        f"observations={len(evidence['source_observations'])}; "
        "no SLO verdict claimed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
