from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

from scripts import performance_slo_gate as gate


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "performance" / "slo-targets.json"


class PerformanceSloContractTests(unittest.TestCase):
    def test_canon_targets_are_exact_and_never_claim_existing_measurement(self) -> None:
        data = gate.load_object(TARGETS)
        self.assertEqual(gate.validate_targets(data), [])
        self.assertEqual(data["claim"], "targets_only_not_measured_pass")
        self.assertEqual(
            data["metrics"]["source_analysis"],
            {
                "p50_ms_max": 250,
                "p95_ms_max": 700,
                "conditions": "typical_source_docx_without_ocr",
            },
        )
        self.assertEqual(data["metrics"]["button_to_ready"]["p95_ms_max"], 2000)
        self.assertEqual(data["metrics"]["large_docx_to_ready"]["p95_ms_max"], 5000)
        self.assertEqual(
            data["metrics"]["cold_start_to_interactive_ui"]["p95_ms_max"], 3000
        )
        self.assertEqual(data["metrics"]["visible_action_response"]["p95_ms_max"], 100)
        self.assertEqual(data["metrics"]["prompt_form_ready"]["p95_ms_max"], 150)

    def _reference(self, targets: dict[str, object]) -> dict[str, object]:
        environment = {
            key: f"bound-{key}"
            for key in targets["required_environment_fields"]  # type: ignore[index]
        }
        bindings = {
            metric: f"series-{metric}"
            for metric in gate.CANONICAL_METRICS
        }
        budgets = {
            "cpu_percent_peak": 95,
            "peak_rss_bytes": 2_000_000_000,
            "disk_write_bytes": 5_000_000_000,
            "staging_peak_bytes": 5_000_000_000,
            "cold_start_ms": 3000,
            "ui_response_ms": 100,
            "worker_count": 4,
            "queue_limit": 100,
        }
        return {
            "schema": gate.REFERENCE_SCHEMA,
            "status": "bound",
            "reference_id": "test-reference-machine",
            "corpus_id": "test-performance-corpus",
            "environment": environment,
            "min_samples_per_series": 3,
            "metric_bindings": bindings,
            "resource_budgets": budgets,
        }

    def _evidence(
        self,
        targets_path: Path,
        reference_path: Path,
        reference: dict[str, object],
    ) -> dict[str, object]:
        classes = [
            *gate.CANONICAL_CORPUS_CLASSES,
            *gate.CANONICAL_SPECIAL_CLASSES,
        ]
        complexity = {
            "compressed_bytes": 20 * 1024 * 1024,
            **{
                dimension: 1
                for dimension in gate.CANONICAL_COMPLEXITY_DIMENSIONS
            },
        }
        bound_specs = {
            "source_analysis": {
                "class": "typical_docx",
                "cache_state": "warm_cache",
                "run_kind": "repeat_run",
                "conditions": {"ocr": False},
            },
            "preflight_calculation": {
                "class": "small_docx",
                "cache_state": "warm_cache",
                "run_kind": "repeat_run",
                "conditions": {"human_wait_excluded": True},
            },
            "render_readback_verify": {
                "class": "typical_docx",
                "cache_state": "warm_cache",
                "run_kind": "single_document",
                "conditions": {"runtime_layout": False},
            },
            "button_to_ready": {
                "class": "typical_docx",
                "cache_state": "warm_cache",
                "run_kind": "single_document",
                "conditions": {"questions_present": False},
            },
            "large_docx_to_ready": {
                "class": "large_docx",
                "cache_state": "warm_cache",
                "run_kind": "single_document",
                "complexity": complexity,
            },
            "cold_start_to_interactive_ui": {
                "class": "unclassified",
                "cache_state": "cold_cache",
                "run_kind": "first_run",
                "conditions": {"installed_configuration": True},
            },
            "visible_action_response": {
                "class": "unclassified",
                "cache_state": "warm_cache",
                "run_kind": "repeat_run",
                "conditions": {"long_work_backend": True},
            },
            "prompt_form_ready": {
                "class": "unclassified",
                "cache_state": "warm_cache",
                "run_kind": "repeat_run",
                "conditions": {"prompt_plan_precalculated": True},
            },
        }
        series = []
        for metric, spec in bound_specs.items():
            series.append(
                {
                    "id": f"series-{metric}",
                    "metric": metric,
                    **spec,
                    "warmup_runs": 1,
                    "samples_ms": [1, 1, 1],
                }
            )

        already_covered = {item["class"] for item in series}
        missing_classes = [
            class_name for class_name in classes if class_name not in already_covered
        ]
        for index, class_name in enumerate(missing_classes):
            series.append(
                {
                    "id": f"coverage-{class_name}",
                    "metric": "source_analysis",
                    "class": class_name,
                    "cache_state": gate.CANONICAL_CACHE_STATES[
                        index % len(gate.CANONICAL_CACHE_STATES)
                    ],
                    "run_kind": gate.CANONICAL_RUN_KINDS[
                        index % len(gate.CANONICAL_RUN_KINDS)
                    ],
                    "warmup_runs": 1,
                    "samples_ms": [1, 1, 1],
                }
            )
        return {
            "schema": gate.EVIDENCE_SCHEMA,
            "targets_sha256": gate.sha256_file(targets_path),
            "reference_policy_sha256": gate.sha256_file(reference_path),
            "reference_id": reference["reference_id"],
            "corpus_id": reference["corpus_id"],
            "environment": reference["environment"],
            "protocol": {
                "human_wait_excluded": True,
                "end_to_end_measured_directly": True,
                "drop_to_ready_reported": True,
                "click_to_ready_reported": True,
                "special_classes_separated": True,
                "sample_count_and_warmup_recorded": True,
                "verification_enabled": True,
            },
            "series": series,
            "resources": {
                "cpu_percent_peak": 50,
                "peak_rss_bytes": 1_000_000_000,
                "disk_write_bytes": 1_000_000,
                "staging_peak_bytes": 1_000_000,
                "cold_start_ms": 1000,
                "ui_response_ms": 50,
                "worker_count": 2,
                "queue_limit": 20,
            },
        }

    def _write_json(self, path: Path, value: object) -> None:
        path.write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    def test_target_contract_cannot_silently_drop_required_coverage(self) -> None:
        data = gate.load_object(TARGETS)
        data["required_corpus_classes"] = ["typical_docx"]
        errors = gate.validate_targets(data)
        self.assertTrue(
            any("required_corpus_classes drifted" in error for error in errors),
            errors,
        )

    def test_unbound_reference_machine_can_never_produce_pass(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        reference["status"] = "unbound"
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any("not bound" in error for error in verdict["errors"]),
                verdict["errors"],
            )

    def test_two_samples_are_not_accepted_as_p95(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            for series in evidence["series"]:  # type: ignore[index]
                series["samples_ms"] = [1, 1]
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any("below bound minimum 3" in error for error in verdict["errors"]),
                verdict["errors"],
            )

    def test_exact_contract_can_pass_only_with_bound_machine_corpus_and_budgets(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "PASS", verdict["errors"])

            slow = copy.deepcopy(evidence)
            source_series = next(
                item
                for item in slow["series"]  # type: ignore[index]
                if item["metric"] == "source_analysis"
            )
            source_series["samples_ms"] = [100, 200, 701]
            slow_path = tmp_path / "slow-evidence.json"
            self._write_json(slow_path, slow)
            slow_verdict = gate.evaluate(TARGETS, reference_path, slow_path)
            self.assertEqual(slow_verdict["result"], "FAIL")
            self.assertTrue(
                any("source_analysis: p95" in error for error in slow_verdict["errors"]),
                slow_verdict["errors"],
            )

    def test_incomplete_corpus_and_measurement_split_coverage_cannot_pass(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            for series in evidence["series"]:  # type: ignore[index]
                series["class"] = "typical_docx"
                series["cache_state"] = "warm_cache"
                series["run_kind"] = "single_document"
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "measured corpus/class coverage is incomplete" in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )
            self.assertTrue(
                any(
                    "measured cache-state coverage is incomplete" in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )
            self.assertTrue(
                any(
                    "measured run-kind coverage is incomplete" in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )

    def test_bound_slo_series_must_satisfy_its_own_conditions(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)

            wrong_source = self._evidence(TARGETS, reference_path, reference)
            source = next(
                item
                for item in wrong_source["series"]  # type: ignore[index]
                if item["id"] == "series-source_analysis"
            )
            source["class"] = "ocr"
            wrong_source_path = tmp_path / "wrong-source.json"
            self._write_json(wrong_source_path, wrong_source)
            source_verdict = gate.evaluate(
                TARGETS, reference_path, wrong_source_path
            )
            self.assertEqual(source_verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "source_analysis: bound series class" in error
                    for error in source_verdict["errors"]
                ),
                source_verdict["errors"],
            )

            wrong_cold = self._evidence(TARGETS, reference_path, reference)
            cold = next(
                item
                for item in wrong_cold["series"]  # type: ignore[index]
                if item["id"] == "series-cold_start_to_interactive_ui"
            )
            cold["cache_state"] = "warm_cache"
            cold["run_kind"] = "repeat_run"
            cold_path = tmp_path / "wrong-cold.json"
            self._write_json(cold_path, wrong_cold)
            cold_verdict = gate.evaluate(TARGETS, reference_path, cold_path)
            self.assertEqual(cold_verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "cold_start_to_interactive_ui: bound series cache_state"
                    in error
                    for error in cold_verdict["errors"]
                ),
                cold_verdict["errors"],
            )
            self.assertTrue(
                any(
                    "cold_start_to_interactive_ui: bound series run_kind"
                    in error
                    for error in cold_verdict["errors"]
                ),
                cold_verdict["errors"],
            )

    def test_large_docx_slo_requires_structural_complexity_metadata(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            large = next(
                item
                for item in evidence["series"]  # type: ignore[index]
                if item["id"] == "series-large_docx_to_ready"
            )
            large.pop("complexity")
            evidence_path = tmp_path / "missing-complexity.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "large_docx_to_ready: structural complexity metadata is required"
                    in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )

    def test_bound_condition_flags_cannot_be_omitted_or_faked_by_filler_series(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            button = next(
                item
                for item in evidence["series"]  # type: ignore[index]
                if item["id"] == "series-button_to_ready"
            )
            button["conditions"] = {"questions_present": True}
            evidence_path = tmp_path / "wrong-condition.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "button_to_ready: conditions.questions_present must be False"
                    in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )

    def test_malformed_binding_fails_without_crashing_evaluator(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        reference["metric_bindings"]["source_analysis"] = ["not", "a", "string"]  # type: ignore[index]
        errors = gate.validate_reference(reference, targets)
        self.assertTrue(
            any("metric_bindings.source_analysis" in error for error in errors),
            errors,
        )

    def test_evidence_is_bound_to_exact_targets_and_reference_policy(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            evidence["targets_sha256"] = "0" * 64
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertIn(
                "evidence is not bound to the exact target contract",
                verdict["errors"],
            )




    def test_unclassified_physical_series_does_not_satisfy_class_coverage(self) -> None:
        targets = gate.load_object(TARGETS)
        reference = self._reference(targets)
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            reference_path = tmp_path / "reference.json"
            self._write_json(reference_path, reference)
            evidence = self._evidence(TARGETS, reference_path, reference)
            runtime_layout = next(
                item
                for item in evidence["series"]  # type: ignore[index]
                if item["class"] == "runtime_layout"
            )
            evidence["series"].remove(runtime_layout)  # type: ignore[index]
            physical = next(
                item
                for item in evidence["series"]  # type: ignore[index]
                if item["id"] == "series-visible_action_response"
            )
            physical["class"] = "unclassified"
            evidence_path = tmp_path / "missing-runtime-layout.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any(
                    "measured corpus/class coverage is incomplete" in error
                    and "runtime_layout" in error
                    for error in verdict["errors"]
                ),
                verdict["errors"],
            )

if __name__ == "__main__":
    unittest.main()
