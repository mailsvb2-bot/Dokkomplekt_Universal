from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

from scripts import performance_slo_gate as gate


ROOT = Path(__file__).resolve().parents[1]
TARGETS = ROOT / "verification" / "performance" / "slo-targets.json"


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
        series = []
        for metric in gate.CANONICAL_METRICS:
            series.append(
                {
                    "id": f"series-{metric}",
                    "metric": metric,
                    "class": "contract-fixture",
                    "cache_state": "recorded",
                    "run_kind": "contract-fixture",
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
            "coverage": {
                "measurement_splits": targets["required_measurement_splits"],
                "corpus_classes": targets["required_corpus_classes"],
                "separate_performance_classes": targets["separate_performance_classes"],
                "typical_docx_complexity_dimensions": targets["typical_docx"]["complexity_dimensions"],
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
            evidence["coverage"]["measurement_splits"] = ["warm_cache"]  # type: ignore[index]
            evidence["coverage"]["corpus_classes"] = ["typical_docx"]  # type: ignore[index]
            evidence_path = tmp_path / "evidence.json"
            self._write_json(evidence_path, evidence)

            verdict = gate.evaluate(TARGETS, reference_path, evidence_path)
            self.assertEqual(verdict["result"], "FAIL")
            self.assertTrue(
                any("coverage.measurement_splits is incomplete" in error for error in verdict["errors"]),
                verdict["errors"],
            )
            self.assertTrue(
                any("coverage.corpus_classes is incomplete" in error for error in verdict["errors"]),
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


if __name__ == "__main__":
    unittest.main()
