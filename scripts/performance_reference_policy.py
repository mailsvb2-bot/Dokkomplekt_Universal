#!/usr/bin/env python3
"""Build a bound Canon E6 reference policy from measured, hash-verified inputs.

The builder never invents machine properties, metric bindings or resource budgets.
It verifies the materialized corpus against the pinned corpus spec, then emits the
reference policy consumed by performance_slo_gate.py.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path, PureWindowsPath
from typing import Any

from scripts import performance_benchmark_harness as benchmark
from scripts import performance_slo_gate as gate

ENVIRONMENT_SCHEMA = "dokkomplekt.performance-environment-snapshot.v1"
BINDINGS_SCHEMA = "dokkomplekt.performance-metric-bindings.v1"
BUDGETS_SCHEMA = "dokkomplekt.performance-resource-budgets.v1"
SPECIAL_SOURCES_SCHEMA = "dokkomplekt.performance-special-coverage-sources.v2"


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _opaque(value: Any, label: str) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 128
        or not all(ch.isascii() and (ch.isalnum() or ch in "-_.") for ch in value)
    ):
        raise ValueError(f"{label} must be an opaque ASCII identifier")
    return value


def _verify_corpus(spec_path: Path, corpus_dir: Path) -> tuple[dict[str, list[str]], str]:
    spec = _load(spec_path)
    benchmark.validate_corpus_spec(spec)
    manifest_path = corpus_dir / "corpus-manifest.json"
    manifest = _load(manifest_path)
    if set(manifest) != {
        "schema", "claim", "corpus_id", "corpus_spec_sha256", "files"
    }:
        raise ValueError("corpus manifest keys must be closed")
    if manifest.get("schema") != benchmark.CORPUS_MANIFEST_SCHEMA:
        raise ValueError("corpus manifest schema is invalid")
    if manifest.get("claim") != benchmark.CORPUS_CLAIM:
        raise ValueError("corpus manifest claim is invalid")
    if manifest.get("corpus_id") != spec.get("corpus_id"):
        raise ValueError("corpus manifest corpus_id differs from pinned spec")
    if manifest.get("corpus_spec_sha256") != _sha256(spec_path):
        raise ValueError("corpus manifest is not bound to the exact pinned spec")
    files = manifest.get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("corpus manifest files must be a non-empty array")

    coverage: dict[str, list[str]] = {
        class_name: [] for class_name in gate.CANONICAL_CORPUS_CLASSES
    }
    counts = {class_name: 0 for class_name in gate.CANONICAL_CORPUS_CLASSES}
    seen_paths: set[str] = set()
    seen_hashes: set[str] = set()
    for index, item in enumerate(files):
        if not isinstance(item, dict) or set(item) != {
            "class", "index", "sha256", "bytes", "relative_path"
        }:
            raise ValueError(f"corpus manifest files[{index}] keys must be closed")
        class_name = item.get("class")
        if class_name not in coverage:
            raise ValueError(f"corpus manifest has non-canonical class: {class_name!r}")
        relative = item.get("relative_path")
        if not isinstance(relative, str) or not relative:
            raise ValueError("corpus manifest relative_path must be non-empty")
        rel_path = Path(relative)
        if rel_path.is_absolute() or ".." in rel_path.parts:
            raise ValueError("corpus manifest relative_path must remain inside corpus directory")
        if relative in seen_paths:
            raise ValueError(f"duplicate corpus relative_path: {relative}")
        seen_paths.add(relative)
        actual_path = corpus_dir / rel_path
        if not actual_path.is_file():
            raise ValueError(f"corpus file missing: {relative}")
        actual_sha = _sha256(actual_path)
        if item.get("sha256") != actual_sha:
            raise ValueError(f"corpus file hash mismatch: {relative}")
        if item.get("bytes") != actual_path.stat().st_size:
            raise ValueError(f"corpus file size mismatch: {relative}")
        if actual_sha in seen_hashes:
            raise ValueError(f"duplicate corpus file content hash: {actual_sha}")
        seen_hashes.add(actual_sha)
        counts[class_name] += 1
        coverage[class_name].append(actual_sha)

    for class_name, profile in spec["classes"].items():
        if counts[class_name] != profile["documents"]:
            raise ValueError(
                f"{class_name}: corpus document count {counts[class_name]} "
                f"does not match pinned spec {profile['documents']}"
            )
        coverage[class_name].sort()
    return coverage, _sha256(manifest_path)


def _load_special_sources(
    path: Path,
    artifacts_dir: Path,
) -> dict[str, list[str]]:
    document = _load(path)
    if set(document) != {"schema", "sources"}:
        raise ValueError("special coverage source manifest keys must be closed")
    if document.get("schema") != SPECIAL_SOURCES_SCHEMA:
        raise ValueError("special coverage source manifest schema is invalid")
    if not artifacts_dir.is_dir():
        raise ValueError("special coverage artifacts directory does not exist")
    artifacts_root = artifacts_dir.resolve(strict=True)
    sources = document.get("sources")
    expected = set(gate.CANONICAL_SPECIAL_CLASSES)
    if not isinstance(sources, dict) or set(sources) != expected:
        raise ValueError(
            "special coverage sources must define exactly the canonical special classes"
        )
    normalized: dict[str, list[str]] = {}
    globally_seen_paths: set[str] = set()
    for class_name in gate.CANONICAL_SPECIAL_CLASSES:
        values = sources.get(class_name)
        if not isinstance(values, list) or not values:
            raise ValueError(
                f"special coverage sources for {class_name} must be non-empty"
            )
        seen_hashes: set[str] = set()
        normalized_values: list[str] = []
        for index, value in enumerate(values):
            if not isinstance(value, dict) or set(value) != {"relative_path", "sha256"}:
                raise ValueError(
                    f"special coverage sources {class_name}[{index}] keys must be closed"
                )
            relative = value.get("relative_path")
            if not isinstance(relative, str) or not relative:
                raise ValueError(
                    f"special coverage sources {class_name}[{index}].relative_path "
                    "must be non-empty"
                )
            relative_path = Path(relative)
            windows_path = PureWindowsPath(relative)
            if (
                relative_path.is_absolute()
                or windows_path.drive
                or windows_path.root
                or ".." in relative_path.parts
            ):
                raise ValueError(
                    f"special coverage sources {class_name}[{index}].relative_path "
                    "must remain inside artifacts directory"
                )
            normalized_relative = relative_path.as_posix()
            if normalized_relative in globally_seen_paths:
                raise ValueError(
                    f"duplicate special coverage artifact path: {normalized_relative}"
                )
            globally_seen_paths.add(normalized_relative)
            expected_sha = value.get("sha256")
            if (
                not isinstance(expected_sha, str)
                or len(expected_sha) != 64
                or any(ch not in "0123456789abcdef" for ch in expected_sha)
            ):
                raise ValueError(
                    f"special coverage sources {class_name}[{index}].sha256 "
                    "must be lowercase SHA-256"
                )
            artifact = artifacts_root / relative_path
            try:
                resolved_artifact = artifact.resolve(strict=True)
            except OSError as exc:
                raise ValueError(
                    f"special coverage artifact missing: {normalized_relative}"
                ) from exc
            try:
                resolved_artifact.relative_to(artifacts_root)
            except ValueError as exc:
                raise ValueError(
                    f"special coverage artifact escapes artifacts directory: "
                    f"{normalized_relative}"
                ) from exc
            if not resolved_artifact.is_file():
                raise ValueError(
                    f"special coverage artifact missing: {normalized_relative}"
                )
            actual_sha = _sha256(resolved_artifact)
            if actual_sha != expected_sha:
                raise ValueError(
                    f"special coverage artifact hash mismatch: {normalized_relative}"
                )
            if actual_sha in seen_hashes:
                raise ValueError(
                    f"duplicate special coverage source for {class_name}: {actual_sha}"
                )
            seen_hashes.add(actual_sha)
            normalized_values.append(actual_sha)
        normalized[class_name] = sorted(normalized_values)
    return normalized

def build_reference(
    *,
    targets_path: Path,
    spec_path: Path,
    corpus_dir: Path,
    environment_path: Path,
    bindings_path: Path,
    budgets_path: Path,
    special_sources_path: Path,
    special_source_artifacts_dir: Path,
    reference_id: str,
    min_samples_per_series: int,
) -> dict[str, Any]:
    targets = gate.load_object(targets_path)
    target_errors = gate.validate_targets(targets)
    if target_errors:
        raise ValueError("invalid targets: " + "; ".join(target_errors))
    reference_id = _opaque(reference_id, "reference_id")
    if (
        not isinstance(min_samples_per_series, int)
        or isinstance(min_samples_per_series, bool)
        or min_samples_per_series < 3
    ):
        raise ValueError("min_samples_per_series must be an integer >= 3")

    coverage_sources, corpus_manifest_sha256 = _verify_corpus(spec_path, corpus_dir)
    coverage_sources.update(
        _load_special_sources(special_sources_path, special_source_artifacts_dir)
    )
    spec = _load(spec_path)

    environment_document = _load(environment_path)
    if set(environment_document) != {"schema", "environment"}:
        raise ValueError("environment snapshot keys must be closed")
    if environment_document.get("schema") != ENVIRONMENT_SCHEMA:
        raise ValueError("environment snapshot schema is invalid")
    environment = environment_document.get("environment")
    if not isinstance(environment, dict) or set(environment) != set(
        gate.CANONICAL_ENVIRONMENT_FIELDS
    ):
        raise ValueError("environment must define exactly the canonical fields")
    for key, value in environment.items():
        if value is None or value == "" or value == [] or value == {}:
            raise ValueError(f"environment field missing: {key}")

    bindings_document = _load(bindings_path)
    if set(bindings_document) != {"schema", "metric_bindings"}:
        raise ValueError("metric bindings keys must be closed")
    if bindings_document.get("schema") != BINDINGS_SCHEMA:
        raise ValueError("metric bindings schema is invalid")
    bindings = bindings_document.get("metric_bindings")
    if not isinstance(bindings, dict) or set(bindings) != set(gate.CANONICAL_METRICS):
        raise ValueError("metric bindings must define every Canon metric exactly once")
    normalized_bindings = [
        _opaque(value, f"metric_bindings.{metric}")
        for metric, value in bindings.items()
    ]
    if len(set(normalized_bindings)) != len(normalized_bindings):
        raise ValueError("metric binding series ids must be distinct")

    budgets_document = _load(budgets_path)
    if set(budgets_document) != {"schema", "resource_budgets"}:
        raise ValueError("resource budgets keys must be closed")
    if budgets_document.get("schema") != BUDGETS_SCHEMA:
        raise ValueError("resource budgets schema is invalid")
    budgets = budgets_document.get("resource_budgets")
    if not isinstance(budgets, dict) or set(budgets) != set(gate.RESOURCE_POLICY_KEYS):
        raise ValueError("resource budgets must define exactly the canonical keys")
    for key, value in budgets.items():
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(float(value))
            or float(value) <= 0
        ):
            raise ValueError(f"resource_budgets.{key} must be finite and positive")

    reference = {
        "schema": gate.REFERENCE_SCHEMA,
        "status": "bound",
        "reference_id": reference_id,
        "corpus_id": spec["corpus_id"],
        "corpus_spec_sha256": _sha256(spec_path),
        "corpus_manifest_sha256": corpus_manifest_sha256,
        "environment": environment,
        "min_samples_per_series": min_samples_per_series,
        "metric_bindings": bindings,
        "coverage_sources": coverage_sources,
        "resource_budgets": budgets,
    }
    errors = gate.validate_reference(reference, targets)
    if errors:
        raise ValueError("built reference is invalid: " + "; ".join(errors))
    return reference


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Build a bound Canon E6 reference policy from measured inputs"
    )
    parser.add_argument("--targets", default="performance/slo-targets.json", type=Path)
    parser.add_argument("--spec", default="performance/corpus-spec.json", type=Path)
    parser.add_argument("--corpus-dir", required=True, type=Path)
    parser.add_argument("--environment", required=True, type=Path)
    parser.add_argument("--bindings", required=True, type=Path)
    parser.add_argument("--budgets", required=True, type=Path)
    parser.add_argument("--special-sources", required=True, type=Path)
    parser.add_argument("--special-source-artifacts", required=True, type=Path)
    parser.add_argument("--reference-id", required=True)
    parser.add_argument("--min-samples", type=int, default=3)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        reference = build_reference(
            targets_path=args.targets,
            spec_path=args.spec,
            corpus_dir=args.corpus_dir,
            environment_path=args.environment,
            bindings_path=args.bindings,
            budgets_path=args.budgets,
            special_sources_path=args.special_sources,
            special_source_artifacts_dir=args.special_source_artifacts,
            reference_id=args.reference_id,
            min_samples_per_series=args.min_samples,
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"PERFORMANCE REFERENCE BUILD FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(reference, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "PERFORMANCE REFERENCE BOUND: "
        f"{reference['reference_id']} corpus={reference['corpus_id']}; "
        "no SLO verdict claimed"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
