from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "build_performance_evidence",
    ROOT / "scripts" / "build_performance_evidence.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def write_json(path: Path, data: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    return path


def targets(tmp_path: Path) -> Path:
    return write_json(tmp_path / "targets.json", {"schema": "targets-fixture"})


def reference(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "reference.json",
        {
            "schema": MODULE.REFERENCE_SCHEMA,
            "reference_id": "reference-1",
            "corpus_id": "corpus-1",
            "environment": {
                "cpu": "fixture-cpu",
                "ram_bytes": 16_000_000_000,
                "storage_filesystem": "ntfs",
                "os_build": "fixture-os",
                "power_mode": "balanced",
                "scanner_antivirus_environment": "fixture",
                "app_version": "18.4.7",
                "fonts_layout_engine": "fixture",
            },
        },
    )


def resources(tmp_path: Path) -> Path:
    return write_json(
        tmp_path / "resources.json",
        {
            "schema": MODULE.RESOURCE_SCHEMA,
            "resources": {
                "cpu_percent_peak": 50,
                "peak_rss_bytes": 200_000_000,
                "disk_write_bytes": 10_000,
                "staging_peak_bytes": 20_000,
                "cold_start_ms": 1000,
                "ui_response_ms": 50,
                "worker_count": 2,
                "queue_limit": 8,
            },
        },
    )


def trace(
    root: Path,
    run_id: str,
    *,
    pdf: bool = False,
    human_wait_ms: int = 0,
    workload: str = "single_document",
    run_phase: str = "first_run",
) -> None:
    write_json(
        root / f"{run_id}.json",
        {
            "schema": MODULE.TRACE_SCHEMA,
            "context": {
                "run_id": run_id,
                "app_version": "18.4.7",
                "class": "unclassified",
                "cache_state": "unclassified",
                "run_phase": run_phase,
                "workload": workload,
                "batch_size": 1,
                "ocr_used": False,
                "runtime_layout_used": False,
                "pdf_used": pdf,
            },
            "stages": [
                {"stage": "source_open", "duration_ms": 10},
                {"stage": "reference_clone", "duration_ms": 5},
                {"stage": "replay", "duration_ms": 20},
                {"stage": "verify", "duration_ms": 15},
                {"stage": "publish", "duration_ms": 10},
            ],
            "total_machine_ms": 60,
            "end_to_end_ms": 75,
            "human_wait_ms": human_wait_ms,
            "outcome": "completed",
        },
    )


def protocol() -> dict[str, bool]:
    return {flag: True for flag in MODULE.REQUIRED_PROTOCOL_FLAGS}


def series(run_ids: list[str], *, series_id: str = "button", class_name: str = "typical_docx") -> dict:
    return {
        "id": series_id,
        "metric": "button_to_ready",
        "class": class_name,
        "cache_state": "warm_cache",
        "run_kind": "single_document",
        "conditions": {"questions_present": False},
        "warmup_runs": 1,
        "run_ids": run_ids,
        "extractor": {"kind": "end_to_end_ms"},
    }


def plan(tmp_path: Path, rows: list[dict]) -> Path:
    return write_json(
        tmp_path / "plan.json",
        {
            "schema": MODULE.PLAN_SCHEMA,
            "protocol": protocol(),
            "series": rows,
        },
    )


def test_builds_evidence_from_explicit_completed_trace_binding(tmp_path: Path) -> None:
    trace_dir = tmp_path / "traces"
    trace(trace_dir, "run-a")
    result = MODULE.build_evidence(
        targets(tmp_path),
        reference(tmp_path),
        plan(tmp_path, [series(["run-a"])]),
        resources(tmp_path),
        trace_dir,
    )
    assert result["schema"] == MODULE.EVIDENCE_SCHEMA
    assert result["source_trace_count"] == 1
    assert result["series"][0]["samples_ms"] == [75.0]
    assert result["series"][0]["trace_run_ids"] == ["run-a"]
    assert result["environment"]["app_version"] == "18.4.7"
    assert len(result["measurement_plan_sha256"]) == 64
    assert len(result["resource_snapshot_sha256"]) == 64


def test_typical_docx_rejects_pdf_trace(tmp_path: Path) -> None:
    trace_dir = tmp_path / "traces"
    trace(trace_dir, "run-pdf", pdf=True)
    with pytest.raises(ValueError, match="cannot enter typical_docx"):
        MODULE.build_evidence(
            targets(tmp_path),
            reference(tmp_path),
            plan(tmp_path, [series(["run-pdf"])]),
            resources(tmp_path),
            trace_dir,
        )


def test_one_run_cannot_back_multiple_series(tmp_path: Path) -> None:
    trace_dir = tmp_path / "traces"
    trace(trace_dir, "run-a")
    with pytest.raises(ValueError, match="more than one series"):
        MODULE.build_evidence(
            targets(tmp_path),
            reference(tmp_path),
            plan(
                tmp_path,
                [
                    series(["run-a"], series_id="series-a"),
                    series(["run-a"], series_id="series-b"),
                ],
            ),
            resources(tmp_path),
            trace_dir,
        )


def test_end_to_end_extractor_rejects_human_wait(tmp_path: Path) -> None:
    trace_dir = tmp_path / "traces"
    trace(trace_dir, "run-a", human_wait_ms=5)
    with pytest.raises(ValueError, match="human_wait_ms=0"):
        MODULE.build_evidence(
            targets(tmp_path),
            reference(tmp_path),
            plan(tmp_path, [series(["run-a"])]),
            resources(tmp_path),
            trace_dir,
        )


def test_stage_sum_is_per_run_and_requires_every_stage(tmp_path: Path) -> None:
    trace_dir = tmp_path / "traces"
    trace(trace_dir, "run-a")
    row = series(["run-a"])
    row["metric"] = "render_readback_verify"
    row["extractor"] = {"kind": "stage_sum_ms", "stages": ["replay", "verify"]}
    result = MODULE.build_evidence(
        targets(tmp_path),
        reference(tmp_path),
        plan(tmp_path, [row]),
        resources(tmp_path),
        trace_dir,
    )
    assert result["series"][0]["samples_ms"] == [35.0]

    row["extractor"] = {
        "kind": "stage_sum_ms",
        "stages": ["replay", "physical_readback", "verify"],
    }
    with pytest.raises(ValueError, match="physical_readback"):
        MODULE.build_evidence(
            targets(tmp_path),
            reference(tmp_path),
            plan(tmp_path, [row]),
            resources(tmp_path),
            trace_dir,
        )
