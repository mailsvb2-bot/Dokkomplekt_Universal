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

from scripts import performance_benchmark_harness as benchmark_harness
from scripts import performance_physical_observation as physical_obs
from scripts import performance_slo_gate as gate
from scripts import performance_storage_observation as storage_obs
from scripts import performance_trace_observation as trace_obs

PROTOCOL_SCHEMA = "dokkomplekt.performance-protocol.v1"
RESOURCE_SCHEMA = "dokkomplekt.performance-resource-snapshot.v1"
EVIDENCE_CLAIM = "assembled_observations_not_slo_verdict"

TRACE_OBSERVATION_KEYS = {
    "schema", "claim", "targets_sha256", "reference_policy_sha256",
    "measurement_plan_sha256", "trace_sha256", "reference_id", "corpus_id",
    "series_id", "metric", "class", "cache_state", "run_kind", "conditions",
    "complexity", "warmup", "sample_ms", "sample_derivation", "trace_run_id",
    "trace_app_version", "trace_source_sha256", "trace_workload", "trace_batch_size",
    "trace_feature_flags", "benchmark_context_sha256",
}
TRACE_FEATURE_KEYS = {"ocr_used", "runtime_layout_used", "pdf_used"}
STORAGE_OBSERVATION_KEYS = {
    "schema", "claim", "targets_sha256", "reference_policy_sha256",
    "measurement_plan_sha256", "measurement_sha256", "reference_id", "corpus_id",
    "series_id", "metric", "class", "cache_state", "run_kind", "conditions",
    "complexity", "warmup", "sample_ms", "sample_derivation", "measurement_id",
    "measurement_app_version", "measurement_kind", "measurement_instrument",
    "installed_build", "source_sha256", "storage_class", "probe_sha256",
    "probe_producer", "probe_verification",
}
PHYSICAL_OBSERVATION_KEYS = {
    "schema", "claim", "targets_sha256", "reference_policy_sha256",
    "measurement_plan_sha256", "measurement_sha256", "reference_id", "corpus_id",
    "series_id", "metric", "class", "cache_state", "run_kind", "conditions",
    "complexity", "warmup", "sample_ms", "sample_derivation", "measurement_id",
    "measurement_app_version", "measurement_kind", "measurement_instrument",
    "installed_build",
}
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


def _validate_trace_observation(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    if set(value) != TRACE_OBSERVATION_KEYS:
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
    if not isinstance(bindings, dict):
        raise ValueError(f"{path}: reference metric_bindings must be an object")
    is_bound_slo_series = bindings.get(metric) == series_id
    if value.get("sample_derivation") != EXPECTED_DERIVATIONS[metric]:
        raise ValueError(f"{path}: sample derivation does not match canonical metric")

    class_name = value.get("class")
    if class_name in {"slow_storage", "network_storage"}:
        raise ValueError(
            f"{path}: trace observations cannot prove slow/network storage coverage"
        )
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
    benchmark_context_sha256 = value.get("benchmark_context_sha256")
    if benchmark_context_sha256 is not None and not _is_sha256(
        benchmark_context_sha256
    ):
        raise ValueError(
            f"{path}: benchmark_context_sha256 must be lowercase SHA-256 or null"
        )
    run_id = value.get("trace_run_id")
    if not isinstance(run_id, str) or not run_id:
        raise ValueError(f"{path}: trace_run_id must be non-empty")
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or value.get("trace_app_version") != environment.get("app_version")
    ):
        raise ValueError(f"{path}: trace app version differs from bound reference")

    trace_source_sha256 = value.get("trace_source_sha256")
    if trace_source_sha256 is not None and (
        not isinstance(trace_source_sha256, str)
        or len(trace_source_sha256) != 64
        or any(character not in "0123456789abcdef" for character in trace_source_sha256)
    ):
        raise ValueError(f"{path}: trace_source_sha256 must be lowercase SHA-256 hex or null")
    if not is_bound_slo_series:
        coverage_sources = reference.get("coverage_sources")
        allowed_sources = (
            coverage_sources.get(class_name)
            if isinstance(coverage_sources, dict)
            else None
        )
        if (
            trace_source_sha256 is None
            or not isinstance(allowed_sources, list)
            or trace_source_sha256 not in allowed_sources
        ):
            raise ValueError(
                f"{path}: non-bound coverage observation is not bound to a predeclared source"
            )

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

    if is_bound_slo_series:
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


def _validate_physical_observation(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    if set(value) != PHYSICAL_OBSERVATION_KEYS:
        raise ValueError(f"{path}: physical observation keys must be closed")
    if value.get("schema") != physical_obs.OBSERVATION_SCHEMA:
        raise ValueError(f"{path}: unsupported physical observation schema")
    if value.get("claim") != physical_obs.OBSERVATION_CLAIM:
        raise ValueError(f"{path}: physical observation claim is not single-sample-only")
    if value.get("targets_sha256") != targets_sha256:
        raise ValueError(f"{path}: target hash mismatch")
    if value.get("reference_policy_sha256") != reference_sha256:
        raise ValueError(f"{path}: reference hash mismatch")
    if value.get("reference_id") != reference.get("reference_id"):
        raise ValueError(f"{path}: reference_id mismatch")
    if value.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError(f"{path}: corpus_id mismatch")

    metric = value.get("metric")
    measurement_kind = physical_obs.PHYSICAL_METRICS.get(metric)
    if measurement_kind is None:
        raise ValueError(f"{path}: metric is not produced by the physical/UI observation layer")
    series_id = value.get("series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict) or bindings.get(metric) != series_id:
        raise ValueError(f"{path}: series_id is not reference-bound for metric")
    if value.get("measurement_kind") != measurement_kind:
        raise ValueError(f"{path}: physical measurement kind does not match metric")
    if value.get("sample_derivation") != "direct_physical_measurement:" + measurement_kind:
        raise ValueError(f"{path}: physical sample derivation does not match metric")

    class_name = value.get("class")
    if class_name != physical_obs.PHYSICAL_CLASS:
        raise ValueError(
            f"{path}: physical/UI observation class must remain unclassified"
        )
    if value.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError(f"{path}: non-canonical cache_state")
    if value.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError(f"{path}: non-canonical run_kind")
    conditions = value.get("conditions")
    if not isinstance(conditions, dict):
        raise ValueError(f"{path}: conditions must be an object")
    complexity = value.get("complexity")
    if complexity is not None and not isinstance(complexity, dict):
        raise ValueError(f"{path}: complexity must be an object or null")
    if not isinstance(value.get("warmup"), bool):
        raise ValueError(f"{path}: warmup must be boolean")
    _nonnegative_number(value.get("sample_ms"), f"{path}: sample_ms")

    for key in ("measurement_plan_sha256", "measurement_sha256"):
        if not _is_sha256(value.get(key)):
            raise ValueError(f"{path}: {key} must be lowercase SHA-256")
    measurement_id = value.get("measurement_id")
    if (
        not isinstance(measurement_id, str)
        or not measurement_id
        or len(measurement_id) > 128
        or not all(
            character.isascii() and (character.isalnum() or character in "-_.")
            for character in measurement_id
        )
    ):
        raise ValueError(f"{path}: measurement_id must be an opaque ASCII identifier")
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or value.get("measurement_app_version") != environment.get("app_version")
    ):
        raise ValueError(f"{path}: measurement app version differs from bound reference")
    if value.get("measurement_instrument") not in physical_obs.ALLOWED_INSTRUMENTS:
        raise ValueError(f"{path}: physical measurement instrument is not approved")
    if value.get("installed_build") is not True:
        raise ValueError(f"{path}: physical observation requires an installed build")

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
    for flag, expected in requirements.get("condition_flags", {}).items():
        if conditions.get(flag) is not expected:
            raise ValueError(f"{path}: conditions.{flag} violates metric binding")
    return value


def _validate_storage_observation(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    if set(value) != STORAGE_OBSERVATION_KEYS:
        raise ValueError(f"{path}: storage observation keys must be closed")
    if value.get("schema") != storage_obs.OBSERVATION_SCHEMA:
        raise ValueError(f"{path}: unsupported storage observation schema")
    if value.get("claim") != storage_obs.OBSERVATION_CLAIM:
        raise ValueError(f"{path}: storage observation claim is not coverage-only")
    if value.get("targets_sha256") != targets_sha256:
        raise ValueError(f"{path}: target hash mismatch")
    if value.get("reference_policy_sha256") != reference_sha256:
        raise ValueError(f"{path}: reference hash mismatch")
    if value.get("reference_id") != reference.get("reference_id"):
        raise ValueError(f"{path}: reference_id mismatch")
    if value.get("corpus_id") != reference.get("corpus_id"):
        raise ValueError(f"{path}: corpus_id mismatch")
    if value.get("metric") != "button_to_ready":
        raise ValueError(f"{path}: storage coverage metric must be button_to_ready")

    series_id = value.get("series_id")
    bindings = reference.get("metric_bindings")
    if not isinstance(bindings, dict):
        raise ValueError(f"{path}: reference metric_bindings must be an object")
    if bindings.get("button_to_ready") == series_id:
        raise ValueError(f"{path}: storage coverage series must remain non-bound")

    class_name = value.get("class")
    if class_name not in storage_obs.SUPPORTED_CLASSES:
        raise ValueError(f"{path}: unsupported storage coverage class")
    if value.get("storage_class") != class_name:
        raise ValueError(f"{path}: storage_class does not match observation class")
    if not _is_sha256(value.get("probe_sha256")):
        raise ValueError(f"{path}: probe_sha256 must be lowercase SHA-256")
    if value.get("probe_producer") != storage_obs.storage_probe.PRODUCER:
        raise ValueError(f"{path}: storage probe producer is not canonical")
    probe_verification = value.get("probe_verification")
    if not isinstance(probe_verification, dict):
        raise ValueError(f"{path}: storage probe verification must be an object")
    if class_name == "network_storage":
        if probe_verification != {"path_kind": "unc", "windows_drive_type": "remote"}:
            raise ValueError(f"{path}: network storage probe is not UNC/remote verified")
    else:
        if set(probe_verification) != {"mode", "configured_bytes_per_sec"}:
            raise ValueError(f"{path}: slow storage probe verification keys are invalid")
        if probe_verification.get("mode") != "controlled_read_throttle":
            raise ValueError(f"{path}: slow storage probe did not use controlled throttle")
        cap = probe_verification.get("configured_bytes_per_sec")
        if not isinstance(cap, int) or isinstance(cap, bool) or cap <= 0:
            raise ValueError(f"{path}: slow storage probe throttle cap is invalid")
    if value.get("cache_state") not in gate.CANONICAL_CACHE_STATES:
        raise ValueError(f"{path}: non-canonical cache_state")
    if value.get("run_kind") not in gate.CANONICAL_RUN_KINDS:
        raise ValueError(f"{path}: non-canonical run_kind")
    if not isinstance(value.get("conditions"), dict):
        raise ValueError(f"{path}: conditions must be an object")
    if value.get("complexity") is not None:
        raise ValueError(f"{path}: storage coverage complexity must be null")
    if not isinstance(value.get("warmup"), bool):
        raise ValueError(f"{path}: warmup must be boolean")
    _nonnegative_number(value.get("sample_ms"), f"{path}: sample_ms")
    if value.get("sample_derivation") != "direct_storage_condition_measurement":
        raise ValueError(f"{path}: storage sample derivation is not direct")

    for key in ("measurement_plan_sha256", "measurement_sha256", "source_sha256"):
        if not _is_sha256(value.get(key)):
            raise ValueError(f"{path}: {key} must be lowercase SHA-256")
    coverage_sources = reference.get("coverage_sources")
    allowed_sources = (
        coverage_sources.get(class_name)
        if isinstance(coverage_sources, dict)
        else None
    )
    if not isinstance(allowed_sources, list) or value["source_sha256"] not in allowed_sources:
        raise ValueError(f"{path}: storage source is not predeclared for this class")

    measurement_id = value.get("measurement_id")
    if (
        not isinstance(measurement_id, str)
        or not measurement_id
        or len(measurement_id) > 128
        or not all(
            character.isascii() and (character.isalnum() or character in "-_.")
            for character in measurement_id
        )
    ):
        raise ValueError(f"{path}: measurement_id must be an opaque ASCII identifier")
    environment = reference.get("environment")
    if (
        not isinstance(environment, dict)
        or value.get("measurement_app_version") != environment.get("app_version")
    ):
        raise ValueError(f"{path}: measurement app version differs from bound reference")
    if value.get("measurement_kind") != storage_obs.MEASUREMENT_KIND:
        raise ValueError(f"{path}: storage measurement kind is invalid")
    if value.get("measurement_instrument") not in storage_obs.ALLOWED_INSTRUMENTS:
        raise ValueError(f"{path}: storage measurement instrument is not approved")
    if value.get("installed_build") is not True:
        raise ValueError(f"{path}: storage observation requires an installed build")
    return value


def _validate_observation(
    path: Path,
    *,
    targets_sha256: str,
    reference_sha256: str,
    reference: dict[str, Any],
) -> dict[str, Any]:
    value = _load(path)
    schema = value.get("schema")
    if schema == trace_obs.OBSERVATION_SCHEMA:
        return _validate_trace_observation(
            path,
            targets_sha256=targets_sha256,
            reference_sha256=reference_sha256,
            reference=reference,
        )
    if schema == physical_obs.OBSERVATION_SCHEMA:
        return _validate_physical_observation(
            path,
            targets_sha256=targets_sha256,
            reference_sha256=reference_sha256,
            reference=reference,
        )
    if schema == storage_obs.OBSERVATION_SCHEMA:
        return _validate_storage_observation(
            path,
            targets_sha256=targets_sha256,
            reference_sha256=reference_sha256,
            reference=reference,
        )
    raise ValueError(f"{path}: unsupported observation schema: {schema!r}")


def _validate_benchmark_context_artifact(
    observation: dict[str, Any],
    benchmark_contexts_dir: Path | None,
) -> None:
    expected_sha256 = observation.get("benchmark_context_sha256")
    if expected_sha256 is None:
        return
    if not _is_sha256(expected_sha256):
        raise ValueError("benchmark_context_sha256 must be lowercase SHA-256 or null")
    if benchmark_contexts_dir is None or not benchmark_contexts_dir.is_dir():
        raise ValueError(
            "benchmark-bound trace observation requires a benchmark-contexts directory"
        )
    matches = [
        path
        for path in sorted(benchmark_contexts_dir.glob("*.json"))
        if path.is_file() and _sha256(path) == expected_sha256
    ]
    if len(matches) != 1:
        raise ValueError(
            f"benchmark context hash {expected_sha256} must resolve to exactly one file"
        )
    context = _load(matches[0])
    benchmark_harness.validate_context(context)
    if (
        context.get("schema") != trace_obs.BENCHMARK_CONTEXT_SCHEMA
        or context.get("producer") != trace_obs.BENCHMARK_CONTEXT_PRODUCER
        or context.get("claim") != trace_obs.BENCHMARK_CONTEXT_CLAIM
    ):
        raise ValueError("benchmark context artifact provenance is invalid")
    if context.get("measurement_plan_sha256") != observation.get("measurement_plan_sha256"):
        raise ValueError("benchmark context artifact plan differs from observation")
    if context.get("trace_sha256") != observation.get("trace_sha256"):
        raise ValueError("benchmark context artifact trace differs from observation")
    if context.get("source_sha256") != observation.get("trace_source_sha256"):
        raise ValueError("benchmark context artifact source differs from observation")
    if context.get("cache_state") != observation.get("cache_state"):
        raise ValueError("benchmark context artifact cache_state differs from observation")
    run_kind = observation.get("run_kind")
    if run_kind in ("first_run", "repeat_run") and context.get("run_phase") != run_kind:
        raise ValueError("benchmark context artifact run_phase differs from observation")


def _validate_storage_probe_artifact(
    observation: dict[str, Any],
    storage_probes_dir: Path | None,
) -> None:
    if storage_probes_dir is None or not storage_probes_dir.is_dir():
        raise ValueError(
            "storage observation requires a storage-probes directory containing the exact probe artifact"
        )
    expected_sha256 = observation["probe_sha256"]
    matches = [
        path
        for path in sorted(storage_probes_dir.glob("*.json"))
        if path.is_file() and _sha256(path) == expected_sha256
    ]
    if len(matches) != 1:
        raise ValueError(
            f"storage probe artifact hash {expected_sha256} must resolve to exactly one file"
        )
    probe = _load(matches[0])
    storage_obs.storage_probe.validate_probe(probe)
    if probe.get("producer") != observation.get("probe_producer"):
        raise ValueError("storage probe artifact producer differs from observation")
    if probe.get("storage_class") != observation.get("storage_class"):
        raise ValueError("storage probe artifact class differs from observation")
    if probe.get("source_sha256") != observation.get("source_sha256"):
        raise ValueError("storage probe artifact source differs from observation")
    if probe.get("verification") != observation.get("probe_verification"):
        raise ValueError("storage probe artifact verification differs from observation")


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
    storage_probes_dir: Path | None = None,
    benchmark_contexts_dir: Path | None = None,
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
    used_measurement_hashes: set[str] = set()
    used_measurement_ids: set[str] = set()
    manifest: list[dict[str, str]] = []
    for path in paths:
        observation = _validate_observation(
            path,
            targets_sha256=targets_sha256,
            reference_sha256=reference_sha256,
            reference=reference,
        )
        if observation["schema"] == trace_obs.OBSERVATION_SCHEMA:
            _validate_benchmark_context_artifact(
                observation, benchmark_contexts_dir
            )
            trace_hash = observation["trace_sha256"]
            run_id = observation["trace_run_id"]
            if trace_hash in used_trace_hashes:
                raise ValueError(f"trace_sha256 reused by multiple observations: {trace_hash}")
            if run_id in used_run_ids:
                raise ValueError(f"trace_run_id reused by multiple observations: {run_id}")
            used_trace_hashes.add(trace_hash)
            used_run_ids.add(run_id)
            manifest_entry = {
                "observation_sha256": _sha256(path),
                "trace_run_id": run_id,
                "trace_sha256": trace_hash,
                "measurement_plan_sha256": observation["measurement_plan_sha256"],
            }
            if observation.get("benchmark_context_sha256") is not None:
                manifest_entry["benchmark_context_sha256"] = observation[
                    "benchmark_context_sha256"
                ]
            if observation.get("trace_source_sha256") is not None:
                manifest_entry["trace_source_sha256"] = observation["trace_source_sha256"]
        else:
            measurement_hash = observation["measurement_sha256"]
            measurement_id = observation["measurement_id"]
            if measurement_hash in used_measurement_hashes:
                raise ValueError(
                    f"measurement_sha256 reused by multiple observations: {measurement_hash}"
                )
            if measurement_id in used_measurement_ids:
                raise ValueError(
                    f"measurement_id reused by multiple observations: {measurement_id}"
                )
            used_measurement_hashes.add(measurement_hash)
            used_measurement_ids.add(measurement_id)
            manifest_entry = {
                "observation_sha256": _sha256(path),
                "measurement_id": measurement_id,
                "measurement_sha256": measurement_hash,
                "measurement_plan_sha256": observation["measurement_plan_sha256"],
            }
            if observation["schema"] == storage_obs.OBSERVATION_SCHEMA:
                _validate_storage_probe_artifact(observation, storage_probes_dir)
                manifest_entry["probe_sha256"] = observation["probe_sha256"]
                manifest_entry["source_sha256"] = observation["source_sha256"]

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

        manifest.append(manifest_entry)

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
    parser.add_argument(
        "--storage-probes",
        type=Path,
        help="Directory containing exact storage probe artifacts referenced by storage observations",
    )
    parser.add_argument(
        "--benchmark-contexts",
        type=Path,
        help="Directory containing exact benchmark lifecycle contexts referenced by trace observations",
    )
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        evidence = build_evidence(
            args.targets,
            args.reference,
            args.observations,
            args.protocol,
            args.resources,
            args.storage_probes,
            args.benchmark_contexts,
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
