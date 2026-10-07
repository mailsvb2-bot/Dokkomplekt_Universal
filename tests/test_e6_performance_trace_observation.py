from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from scripts import performance_slo_gate as gate
from scripts import performance_trace_observation as obs


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "performance" / "slo-targets.json"


class PerformanceTraceObservationTests(unittest.TestCase):
    def _write(self, path: Path, value: object) -> None:
        path.write_text(
            json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    def _reference(self) -> dict[str, object]:
        targets = gate.load_object(TARGETS)
        return {
            "schema": gate.REFERENCE_SCHEMA,
            "status": "bound",
            "reference_id": "reference-machine-01",
            "corpus_id": "performance-corpus-01",
            "environment": {
                key: (
                    "18.4.7"
                    if key == "app_version"
                    else f"bound-{key}"
                )
                for key in targets["required_environment_fields"]
            },
            "min_samples_per_series": 3,
            "metric_bindings": {
                metric: f"series-{metric}"
                for metric in gate.CANONICAL_METRICS
            },
            "coverage_sources": {
                "table_heavy": ["a" * 64],
            },
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
        }

    def _plan(
        self,
        metric: str = "button_to_ready",
        *,
        class_name: str = "typical_docx",
        cache_state: str = "warm_cache",
        run_kind: str = "single_document",
        conditions: dict[str, object] | None = None,
        complexity: dict[str, object] | None = None,
        workload: str = "single_document",
        ocr: bool = False,
        runtime_layout: bool = False,
        pdf: bool = False,
        expected_source_sha256: str | None = None,
    ) -> dict[str, object]:
        if conditions is None:
            conditions = {"questions_present": False}
        return {
            "schema": obs.PLAN_SCHEMA,
            "reference_id": "reference-machine-01",
            "corpus_id": "performance-corpus-01",
            "series_id": f"series-{metric}",
            "metric": metric,
            "class": class_name,
            "cache_state": cache_state,
            "run_kind": run_kind,
            "conditions": conditions,
            "complexity": complexity,
            "warmup": False,
            "expected_app_version": "18.4.7",
            "expected_source_sha256": expected_source_sha256,
            "expected_workload": workload,
            "expected_ocr_used": ocr,
            "expected_runtime_layout_used": runtime_layout,
            "expected_pdf_used": pdf,
        }

    def _trace(
        self,
        *,
        stages: list[tuple[str, int]] | None = None,
        end_to_end_ms: int = 180,
        human_wait_ms: int = 0,
        workload: str = "single_document",
        batch_size: int = 1,
        run_phase: str = "repeat_run",
        cache_state: str = "warm_cache",
        ocr: bool = False,
        runtime_layout: bool = False,
        pdf: bool = False,
        source_sha256: str | None = None,
    ) -> dict[str, object]:
        if stages is None:
            stages = [
                ("source_open", 10),
                ("reference_clone", 10),
                ("replay", 40),
                ("verify", 20),
                ("publish", 30),
            ]
        return {
            "schema": obs.TRACE_SCHEMA,
            "context": {
                "run_id": "abc123",
                "app_version": "18.4.7",
                "source_sha256": source_sha256,
                "class": "unclassified",
                "cache_state": cache_state,
                "run_phase": run_phase,
                "workload": workload,
                "batch_size": batch_size,
                "ocr_used": ocr,
                "runtime_layout_used": runtime_layout,
                "pdf_used": pdf,
            },
            "stages": [
                {"stage": stage, "duration_ms": duration}
                for stage, duration in stages
            ],
            "total_machine_ms": sum(duration for _, duration in stages),
            "end_to_end_ms": end_to_end_ms,
            "human_wait_ms": human_wait_ms,
            "outcome": "completed",
        }

    def _build(
        self,
        tmp: Path,
        plan: dict[str, object],
        trace: dict[str, object],
    ) -> dict[str, object]:
        reference_path = tmp / "reference.json"
        plan_path = tmp / "plan.json"
        trace_path = tmp / "trace.json"
        self._write(reference_path, self._reference())
        self._write(plan_path, plan)
        self._write(trace_path, trace)
        return obs.build_observation(TARGETS, reference_path, plan_path, trace_path)

    def test_storage_classes_require_dedicated_evidence(self) -> None:
        for class_name in ("slow_storage", "network_storage"):
            with self.subTest(class_name=class_name):
                with tempfile.TemporaryDirectory() as raw:
                    with self.assertRaisesRegex(
                        ValueError, "requires dedicated storage-condition evidence"
                    ):
                        self._build(
                            Path(raw),
                            self._plan(
                                class_name=class_name,
                                expected_source_sha256="a" * 64,
                            ),
                            self._trace(source_sha256="a" * 64),
                        )

    def test_cache_state_must_match_predeclared_plan(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(
                ValueError, "trace cache_state does not match plan.cache_state"
            ):
                self._build(
                    Path(raw),
                    self._plan(cache_state="cold_cache"),
                    self._trace(cache_state="warm_cache"),
                )

    def test_button_to_ready_observation_is_hash_bound_and_privacy_safe(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            result = self._build(Path(raw), self._plan(), self._trace())

        self.assertEqual(result["schema"], obs.OBSERVATION_SCHEMA)
        self.assertEqual(result["claim"], obs.OBSERVATION_CLAIM)
        self.assertEqual(result["sample_ms"], 180)
        self.assertEqual(
            result["sample_derivation"], "end_to_end_ms_minus_human_wait_ms"
        )
        for key in (
            "targets_sha256",
            "reference_policy_sha256",
            "measurement_plan_sha256",
            "trace_sha256",
        ):
            self.assertRegex(str(result[key]), r"^[0-9a-f]{64}$")
        encoded = json.dumps(result, ensure_ascii=False)
        for forbidden in (
            "source_path",
            "output_folder",
            "patient",
            "diagnosis",
            "source_text",
        ):
            self.assertNotIn(forbidden, encoded)

    def test_source_analysis_fails_closed_until_all_required_runtime_stages_exist(self) -> None:
        plan = self._plan(
            "source_analysis",
            run_kind="repeat_run",
            conditions={"ocr": False},
        )
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(
                ValueError,
                "requires runtime stages missing.*source_parse.*candidate_index.*source_resolve",
            ):
                self._build(Path(raw), plan, self._trace())

    def test_per_run_stage_sum_is_used_instead_of_summing_percentiles(self) -> None:
        plan = self._plan(
            "render_readback_verify",
            run_kind="single_document",
            conditions={"runtime_layout": False},
        )
        trace = self._trace(
            stages=[
                ("source_open", 5),
                ("reference_clone", 5),
                ("replay", 30),
                ("physical_readback", 20),
                ("verify", 10),
                ("publish", 15),
            ],
            end_to_end_ms=120,
        )
        with tempfile.TemporaryDirectory() as raw:
            result = self._build(Path(raw), plan, trace)

        self.assertEqual(result["sample_ms"], 60)
        self.assertEqual(
            result["sample_derivation"],
            "per_run_stage_sum:replay+physical_readback+verify",
        )

    def test_typical_docx_plan_cannot_relabel_pdf_trace(self) -> None:
        plan = self._plan()
        trace = self._trace(pdf=True)
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "trace pdf_used does not match"):
                self._build(Path(raw), plan, trace)

    def test_non_bound_series_can_supply_corpus_coverage(self) -> None:
        plan = self._plan(
            class_name="table_heavy",
            expected_source_sha256="a" * 64,
        )
        plan["series_id"] = "coverage-table-heavy-button"
        with tempfile.TemporaryDirectory() as raw:
            result = self._build(
                Path(raw),
                plan,
                self._trace(source_sha256="a" * 64),
            )

        self.assertEqual(result["series_id"], "coverage-table-heavy-button")
        self.assertEqual(result["class"], "table_heavy")
        self.assertEqual(result["metric"], "button_to_ready")

    def test_coverage_series_rejects_relabelled_source(self) -> None:
        plan = self._plan(
            class_name="table_heavy",
            expected_source_sha256="a" * 64,
        )
        plan["series_id"] = "coverage-table-heavy-button"
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(
                ValueError, "trace source_sha256 does not match"
            ):
                self._build(
                    Path(raw),
                    plan,
                    self._trace(source_sha256="b" * 64),
                )

        bad_plan = self._plan(
            class_name="table_heavy",
            expected_source_sha256="b" * 64,
        )
        bad_plan["series_id"] = "coverage-table-heavy-button"
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(
                ValueError, "not predeclared for this coverage class"
            ):
                self._build(
                    Path(raw),
                    bad_plan,
                    self._trace(source_sha256="b" * 64),
                )

    def test_bound_series_conditions_and_reference_app_version_cannot_drift(self) -> None:
        plan = self._plan(class_name="table_heavy")
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "requires class=typical_docx"):
                self._build(Path(raw), plan, self._trace())

        plan = self._plan()
        plan["expected_app_version"] = "99.0.0"
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(
                ValueError, "must equal reference environment app_version"
            ):
                self._build(Path(raw), plan, self._trace())

    def test_unexpected_trace_fields_are_rejected_before_export(self) -> None:
        trace = self._trace()
        trace["source_path"] = r"C:\private\patient.docx"
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "trace keys must be closed"):
                self._build(Path(raw), self._plan(), trace)


if __name__ == "__main__":
    unittest.main()
