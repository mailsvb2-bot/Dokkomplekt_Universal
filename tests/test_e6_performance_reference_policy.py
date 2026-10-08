from __future__ import annotations

import hashlib
import json
from pathlib import Path

import pytest

from scripts import performance_benchmark_harness as benchmark
from scripts import performance_reference_policy as reference_policy
from scripts import performance_slo_gate as gate


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "performance" / "slo-targets.json"
SPEC = ROOT / "performance" / "corpus-spec.json"


def write_json(path: Path, value: object) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def materialized_fake_corpus(tmp_path: Path) -> Path:
    spec = gate.load_object(SPEC)
    benchmark.validate_corpus_spec(spec)
    corpus_dir = tmp_path / "corpus"
    files: list[dict[str, object]] = []
    for class_name in gate.CANONICAL_CORPUS_CLASSES:
        profile = spec["classes"][class_name]
        for index in range(1, profile["documents"] + 1):
            path = corpus_dir / class_name / f"{class_name}-{index:03d}.docx"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"{class_name}:{index:03d}".encode("ascii"))
            files.append(
                {
                    "class": class_name,
                    "index": index,
                    "sha256": sha256(path),
                    "bytes": path.stat().st_size,
                    "relative_path": path.relative_to(corpus_dir).as_posix(),
                }
            )
    write_json(
        corpus_dir / "corpus-manifest.json",
        {
            "schema": benchmark.CORPUS_MANIFEST_SCHEMA,
            "claim": benchmark.CORPUS_CLAIM,
            "corpus_id": spec["corpus_id"],
            "corpus_spec_sha256": sha256(SPEC),
            "files": files,
        },
    )
    return corpus_dir


def environment(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "environment.json",
        {
            "schema": reference_policy.ENVIRONMENT_SCHEMA,
            "environment": {
                "cpu": "measured-cpu",
                "ram_bytes": 16_000_000_000,
                "storage_filesystem": "measured-filesystem",
                "os_build": "measured-os-build",
                "power_mode": "measured-power-mode",
                "scanner_antivirus_environment": "measured-security-environment",
                "app_version": "18.4.7",
                "fonts_layout_engine": "measured-font-layout",
            },
        },
    )


def bindings(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "bindings.json",
        {
            "schema": reference_policy.BINDINGS_SCHEMA,
            "metric_bindings": {
                metric: f"series-{metric}" for metric in gate.CANONICAL_METRICS
            },
        },
    )


def budgets(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "budgets.json",
        {
            "schema": reference_policy.BUDGETS_SCHEMA,
            "resource_budgets": {
                "cpu_percent_peak": 95,
                "peak_rss_bytes": 2_000_000_000,
                "disk_write_bytes": 5_000_000_000,
                "staging_peak_bytes": 5_000_000_000,
                "cold_start_ms": 3000,
                "ui_response_ms": 100,
                "worker_count": 4,
                "queue_limit": 100,
            },
        },
    )


def special_sources(tmp_path: Path) -> tuple[Path, Path]:
    artifacts_dir = tmp_path / "special-artifacts"
    sources: dict[str, list[dict[str, str]]] = {}
    for class_name in gate.CANONICAL_SPECIAL_CLASSES:
        relative = f"{class_name}/{class_name}.bin"
        artifact = artifacts_dir / relative
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_bytes(f"special:{class_name}".encode("ascii"))
        sources[class_name] = [
            {
                "relative_path": relative,
                "sha256": sha256(artifact),
            }
        ]
    manifest = write_json(
        tmp_path / "special-sources.json",
        {
            "schema": reference_policy.SPECIAL_SOURCES_SCHEMA,
            "sources": sources,
        },
    )
    return manifest, artifacts_dir

def build(tmp_path: Path, corpus_dir: Path) -> dict[str, object]:
    special_path, artifacts_dir = special_sources(tmp_path)
    return reference_policy.build_reference(
        targets_path=TARGETS,
        spec_path=SPEC,
        corpus_dir=corpus_dir,
        environment_path=environment(tmp_path),
        bindings_path=bindings(tmp_path),
        budgets_path=budgets(tmp_path),
        special_sources_path=special_path,
        special_source_artifacts_dir=artifacts_dir,
        reference_id="reference-machine-01",
        min_samples_per_series=3,
    )


def test_reference_policy_is_bound_to_exact_corpus_and_measured_inputs(
    tmp_path: Path,
) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    reference = build(tmp_path, corpus_dir)
    errors = gate.validate_reference(reference, gate.load_object(TARGETS))
    assert errors == []
    assert reference["status"] == "bound"
    assert reference["corpus_id"] == "dokkomplekt-e6-synthetic-v1"
    assert reference["corpus_spec_sha256"] == sha256(SPEC)
    assert reference["corpus_manifest_sha256"] == sha256(
        corpus_dir / "corpus-manifest.json"
    )
    assert set(reference["coverage_sources"]) == (
        set(gate.CANONICAL_CORPUS_CLASSES) | set(gate.CANONICAL_SPECIAL_CLASSES)
    )
    assert len(reference["coverage_sources"]["batch_10"]) == 10
    assert len(reference["coverage_sources"]["batch_50"]) == 50
    for class_name in gate.CANONICAL_SPECIAL_CLASSES:
        assert len(reference["coverage_sources"][class_name]) == 1


def test_reference_policy_rejects_tampered_corpus_file(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    target = corpus_dir / "typical_docx" / "typical_docx-001.docx"
    target.write_bytes(b"tampered")
    with pytest.raises(ValueError, match="hash mismatch"):
        build(tmp_path, corpus_dir)


def test_reference_policy_rejects_incomplete_environment(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    env_path = environment(tmp_path)
    document = gate.load_object(env_path)
    del document["environment"]["cpu"]
    write_json(env_path, document)
    with pytest.raises(ValueError, match="exactly the canonical fields"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=env_path,
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_sources(tmp_path)[0],
            special_source_artifacts_dir=special_sources(tmp_path)[1],
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )


def test_reference_policy_rejects_duplicate_metric_series_ids(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    bindings_path = bindings(tmp_path)
    document = gate.load_object(bindings_path)
    metrics = list(gate.CANONICAL_METRICS)
    document["metric_bindings"][metrics[1]] = document["metric_bindings"][metrics[0]]
    write_json(bindings_path, document)
    with pytest.raises(ValueError, match="must be distinct"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings_path,
            budgets_path=budgets(tmp_path),
            special_sources_path=special_sources(tmp_path)[0],
            special_source_artifacts_dir=special_sources(tmp_path)[1],
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )



def test_reference_policy_rejects_missing_special_class_sources(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    document = gate.load_object(special_path)
    del document["sources"]["pdf"]
    write_json(special_path, document)
    with pytest.raises(ValueError, match="exactly the canonical special classes"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )


def test_reference_policy_rejects_invalid_special_source_hash(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    document = gate.load_object(special_path)
    document["sources"]["ocr"][0]["sha256"] = "NOT-A-SHA"
    write_json(special_path, document)
    with pytest.raises(ValueError, match="must be lowercase SHA-256"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )

def test_reference_policy_rejects_missing_special_source_artifact(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    target = artifacts_dir / "pdf" / "pdf.bin"
    target.unlink()
    with pytest.raises(ValueError, match="artifact missing"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )


def test_reference_policy_rejects_tampered_special_source_artifact(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    target = artifacts_dir / "ocr" / "ocr.bin"
    target.write_bytes(b"tampered")
    with pytest.raises(ValueError, match="artifact hash mismatch"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )


def test_reference_policy_rejects_special_source_path_traversal(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    document = gate.load_object(special_path)
    document["sources"]["runtime_layout"][0]["relative_path"] = "../escape.bin"
    write_json(special_path, document)
    with pytest.raises(ValueError, match="must remain inside artifacts directory"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )



@pytest.mark.parametrize("escaped", ["/outside.bin", r"\outside.bin"])
def test_reference_policy_rejects_rooted_special_source_paths(
    tmp_path: Path,
    escaped: str,
) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    document = gate.load_object(special_path)
    document["sources"]["runtime_layout"][0]["relative_path"] = escaped
    write_json(special_path, document)
    with pytest.raises(ValueError, match="must remain inside artifacts directory"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )


def test_reference_policy_rejects_symlink_escape(tmp_path: Path) -> None:
    corpus_dir = materialized_fake_corpus(tmp_path)
    special_path, artifacts_dir = special_sources(tmp_path)
    outside = tmp_path / "outside.bin"
    outside.write_bytes(b"external-special-source")
    link = artifacts_dir / "runtime_layout" / "escape.bin"
    try:
        link.symlink_to(outside)
    except OSError:
        pytest.skip("symlinks are unavailable in this test environment")
    document = gate.load_object(special_path)
    document["sources"]["runtime_layout"][0] = {
        "relative_path": "runtime_layout/escape.bin",
        "sha256": sha256(outside),
    }
    write_json(special_path, document)
    with pytest.raises(ValueError, match="escapes artifacts directory"):
        reference_policy.build_reference(
            targets_path=TARGETS,
            spec_path=SPEC,
            corpus_dir=corpus_dir,
            environment_path=environment(tmp_path),
            bindings_path=bindings(tmp_path),
            budgets_path=budgets(tmp_path),
            special_sources_path=special_path,
            special_source_artifacts_dir=artifacts_dir,
            reference_id="reference-machine-01",
            min_samples_per_series=3,
        )
