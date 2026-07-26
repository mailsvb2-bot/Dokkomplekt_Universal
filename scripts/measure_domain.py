#!/usr/bin/env python3
"""Measure Dokkomplekt corpus quality without exposing document values.

The input is the local JSON export produced by the desktop application. The
export contains only installation-keyed HMAC fingerprints, provenance,
confidence and document identifiers. Metrics are therefore computed by equality
of fingerprints; raw names, dates, identifiers and source text are unnecessary.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Iterable

SCHEMA = "dokkomplekt.ground-truth-corpus.v1"
REPORT_SCHEMA = "dokkomplekt.domain-metrics.v2"


def _domain_name(value: Any) -> str:
    if isinstance(value, str):
        return value.lower()
    if isinstance(value, dict) and len(value) == 1:
        key = next(iter(value))
        return str(key).lower()
    return "unknown"


def _load(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("schema") != SCHEMA:
        raise ValueError(f"unsupported corpus schema: {data.get('schema')!r}")
    entries = data.get("entries")
    if not isinstance(entries, list):
        raise ValueError("corpus entries must be an array")
    return data


def _entry(item: Any) -> dict[str, Any]:
    if not isinstance(item, dict):
        raise ValueError("corpus entry item must be an object")
    value = item.get("entry", item)
    if not isinstance(value, dict):
        raise ValueError("entry must be an object")
    return value


_RISK_REGISTRY_PATH = Path(__file__).resolve().parents[1] / "resources" / "field_risk_registry.json"
_RISK_REGISTRY: dict[str, Any] | None = None


def _risk_registry() -> dict[str, Any]:
    """Тот же реестр, что читает Rust-рантайм (automation_quality.rs).

    До 18.4.0 здесь жила независимая реализация классификации риска,
    расходившаяся с рантаймом. Например, subject.address был High
    в automation_quality.rs, но не попадал в _is_high_risk вовсе,
    а medical.diagnosis_code отсутствовал в точном множестве.
    Калибровка вычисляла порог на одной популяции полей, а рантайм
    применял его к другой: подписанный артефакт авторизовал автопечать
    для полей, ошибки на которых он никогда не измерял.
    """
    global _RISK_REGISTRY
    if _RISK_REGISTRY is None:
        _RISK_REGISTRY = json.loads(_RISK_REGISTRY_PATH.read_text(encoding="utf-8"))
        if _RISK_REGISTRY.get("schema") != "dokkomplekt.field-risk-registry.v1":
            raise ValueError(f"unsupported risk registry schema: {_RISK_REGISTRY.get('schema')!r}")
    return _RISK_REGISTRY


def field_risk(field_id: str) -> str:
    """Побитовый эквивалент dokkomplekt_core::field_risk.

    Семантика зафиксирована в самом реестре и обязана совпадать:
      segments = split('.'); words = segments each split('_');
      правило срабатывает при вхождении токена в words ИЛИ в segments.
    Подстрочного сравнения нет.
    """
    registry = _risk_registry()
    lowered = field_id.strip().lower()
    segments = [segment for segment in lowered.split(".") if segment]
    words = {word for segment in segments for word in segment.split("_") if word}
    segment_set = set(segments)
    first_segment = segments[0] if segments else ""
    for risk in registry["order"]:
        rule = registry["rules"][risk]
        if lowered in set(rule["exact"]):
            return risk
        if any(token in words or token in segment_set for token in rule["tokens"]):
            return risk
        if first_segment in set(rule["prefixes"]):
            return risk
    return "low"


def _is_high_risk(field_id: str) -> bool:
    """Поля, по которым калибруются пороги автоматизации.

    High и Critical по общему реестру — ровно та популяция, которую
    рантайм считает материальной."""
    return field_risk(field_id) in ("high", "critical")


def _legacy_is_high_risk(field_id: str) -> bool:
    """Классификация до 18.4.0. Сохранена только для регрессионного теста,
    доказывающего факт расхождения. В расчётах не используется."""
    exact = {
        "subject.name",
        "subject.birth_date",
        "subject.snils",
        "org.inn",
        "org.kpp",
        "org.ogrn",
        "counterparty.inn",
        "counterparty.kpp",
        "medical.diagnosis",
        "medical.icd10",
        "medical.treatment",
    }
    return (
        field_id in exact
        or field_id.endswith(".date")
        or field_id.endswith("_date")
        or field_id.endswith(".amount")
        or field_id.endswith(".number")
        or field_id.endswith("_number") and not field_id.endswith("phone_number")
    )


def _field_rows(entries: Iterable[dict[str, Any]]) -> Iterable[tuple[str, float, bool]]:
    for entry in entries:
        final = {
            str(row.get("field_id")): str(row.get("value_sha256"))
            for row in entry.get("final_accepted", [])
            if isinstance(row, dict)
        }
        for proposal in entry.get("model_proposals", []):
            if not isinstance(proposal, dict):
                continue
            field_id = str(proposal.get("field_id", ""))
            value = str(proposal.get("value_sha256", ""))
            confidence = float(proposal.get("confidence", 0.0))
            yield field_id, max(0.0, min(1.0, confidence)), final.get(field_id) == value


def _bool_metric(evaluation: dict[str, Any], key: str) -> bool | None:
    value = evaluation.get(key)
    return value if isinstance(value, bool) else None


def _ratio(numerator: int, denominator: int) -> float | None:
    return numerator / denominator if denominator else None


def measure(
    data: dict[str, Any],
    *,
    domain: str | None = None,
    auto_threshold: float | None = None,
) -> dict[str, Any]:
    all_entries = [_entry(item) for item in data["entries"]]
    normalized_domain = domain.lower() if domain else None
    entries = [
        item
        for item in all_entries
        if normalized_domain is None or _domain_name(item.get("domain")) == normalized_domain
    ]
    rows = list(_field_rows(entries))
    high_risk = [row for row in rows if _is_high_risk(row[0])]

    final_field_count = 0
    matched_final_fields = 0
    kit_exact: list[bool] = []
    kit_precisions: list[float] = []
    kit_recalls: list[float] = []

    zero_touch_evaluated = 0
    zero_touch_completed = 0
    blocked_evaluated = 0
    false_blocks = 0
    generation_evaluated = 0
    wrong_generations = 0
    auto_print_evaluated = 0
    wrong_auto_prints = 0
    formatting_scores: list[float] = []
    processing_times_ms: list[float] = []

    for entry in entries:
        final = {
            str(row.get("field_id")): str(row.get("value_sha256"))
            for row in entry.get("final_accepted", [])
            if isinstance(row, dict) and row.get("field_id")
        }
        proposed = {
            str(row.get("field_id")): str(row.get("value_sha256"))
            for row in entry.get("model_proposals", [])
            if isinstance(row, dict) and row.get("field_id")
        }
        final_field_count += len(final)
        matched_final_fields += sum(1 for field_id, value in final.items() if proposed.get(field_id) == value)

        proposed_kit = set(map(str, entry.get("proposed_kit_documents", [])))
        actual_kit = set(map(str, entry.get("kit_documents", [])))
        if proposed_kit:
            matching = len(proposed_kit & actual_kit)
            kit_exact.append(proposed_kit == actual_kit)
            kit_precisions.append(matching / max(1, len(proposed_kit)))
            kit_recalls.append(matching / max(1, len(actual_kit)))

        evaluation = entry.get("evaluation", {})
        if not isinstance(evaluation, dict):
            evaluation = {}

        attempted = _bool_metric(evaluation, "zero_touch_attempted")
        completed = _bool_metric(evaluation, "zero_touch_completed")
        if attempted is True and completed is not None:
            zero_touch_evaluated += 1
            zero_touch_completed += int(completed)

        blocked = _bool_metric(evaluation, "automation_blocked")
        false_positive = _bool_metric(evaluation, "block_was_false_positive")
        if blocked is True and false_positive is not None:
            blocked_evaluated += 1
            false_blocks += int(false_positive)

        generation_correct = _bool_metric(evaluation, "generation_correct")
        if generation_correct is not None:
            generation_evaluated += 1
            wrong_generations += int(not generation_correct)

        auto_print_attempted = _bool_metric(evaluation, "auto_print_attempted")
        auto_print_correct = _bool_metric(evaluation, "auto_print_correct")
        if auto_print_attempted is True and auto_print_correct is not None:
            auto_print_evaluated += 1
            wrong_auto_prints += int(not auto_print_correct)

        formatting = evaluation.get("formatting_retention")
        if isinstance(formatting, (int, float)) and not isinstance(formatting, bool):
            formatting_scores.append(max(0.0, min(1.0, float(formatting))))
        processing = evaluation.get("processing_time_ms")
        if isinstance(processing, (int, float)) and not isinstance(processing, bool) and processing >= 0:
            processing_times_ms.append(float(processing))

    def accuracy(values: list[tuple[str, float, bool]]) -> float | None:
        return sum(1 for _, _, correct in values if correct) / len(values) if values else None

    report: dict[str, Any] = {
        "schema": REPORT_SCHEMA,
        "domain": normalized_domain or "all",
        "entry_count": len(entries),
        "field_observations": len(rows),
        "field_accuracy": accuracy(rows),
        "field_recall": _ratio(matched_final_fields, final_field_count),
        "final_field_observations": final_field_count,
        "high_risk_observations": len(high_risk),
        "high_risk_field_accuracy": accuracy(high_risk),
        "kit_compared_entries": len(kit_exact),
        "kit_completeness": sum(kit_exact) / len(kit_exact) if kit_exact else None,
        "bundle_selection_accuracy": sum(kit_exact) / len(kit_exact) if kit_exact else None,
        "bundle_precision": sum(kit_precisions) / len(kit_precisions) if kit_precisions else None,
        "bundle_recall": sum(kit_recalls) / len(kit_recalls) if kit_recalls else None,
        "zero_touch_evaluated_entries": zero_touch_evaluated,
        "zero_touch_rate": _ratio(zero_touch_completed, zero_touch_evaluated),
        "blocked_evaluated_entries": blocked_evaluated,
        "false_block_rate": _ratio(false_blocks, blocked_evaluated),
        "generation_evaluated_entries": generation_evaluated,
        "wrong_generation_rate": _ratio(wrong_generations, generation_evaluated),
        "auto_print_evaluated_entries": auto_print_evaluated,
        "wrong_auto_print_rate": _ratio(wrong_auto_prints, auto_print_evaluated),
        "formatting_evaluated_entries": len(formatting_scores),
        "formatting_retention_rate": (sum(formatting_scores) / len(formatting_scores) if formatting_scores else None),
        "timed_entries": len(processing_times_ms),
        "average_processing_time_ms": (sum(processing_times_ms) / len(processing_times_ms) if processing_times_ms else None),
    }
    if auto_threshold is not None:
        selected = [row for row in rows if row[1] >= auto_threshold]
        errors = sum(1 for _, _, correct in selected if not correct)
        report.update(
            {
                "auto_threshold": auto_threshold,
                "auto_bucket_observations": len(selected),
                "auto_bucket_errors": errors,
                "auto_bucket_error_rate": errors / len(selected) if selected else None,
                "auto_bucket_coverage": len(selected) / len(rows) if rows else None,
            }
        )
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("corpus", type=Path)
    parser.add_argument("--domain")
    parser.add_argument("--auto-threshold", type=float)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--min-high-risk-accuracy", type=float)
    parser.add_argument("--min-kit-completeness", type=float)
    parser.add_argument("--max-auto-error-rate", type=float)
    args = parser.parse_args(argv)
    try:
        report = measure(
            _load(args.corpus),
            domain=args.domain,
            auto_threshold=args.auto_threshold,
        )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    encoded = json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")

    failures: list[str] = []
    checks = [
        (args.min_high_risk_accuracy, report.get("high_risk_field_accuracy"), "high-risk field accuracy"),
        (args.min_kit_completeness, report.get("kit_completeness"), "kit completeness"),
    ]
    for target, actual, label in checks:
        if target is not None and (actual is None or actual < target):
            failures.append(f"{label}: {actual!r} < {target}")
    if args.max_auto_error_rate is not None:
        actual = report.get("auto_bucket_error_rate")
        if actual is None or actual > args.max_auto_error_rate:
            failures.append(f"auto-bucket error rate: {actual!r} > {args.max_auto_error_rate}")
    if failures:
        print("QUALITY GATE FAILED: " + "; ".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
