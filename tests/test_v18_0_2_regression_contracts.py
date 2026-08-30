from __future__ import annotations

import json
import unittest
from datetime import date, timedelta
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return project_text(path)


class FormulaFailClosedContractTest(unittest.TestCase):
    def test_expression_parser_replaced_literal_fallback(self) -> None:
        source = text("crates/dokkomplekt-core/src/template_engine.rs")
        self.assertIn("struct ExpressionParser", source)
        self.assertIn("fn lex_expression", source)
        self.assertIn("fn parse_add_sub", source)
        self.assertIn("fn parse_mul_div", source)
        self.assertNotIn("Ok(operand(s, &lookup))", source)
        self.assertIn("Не найдено поле формулы", source)

    def test_reported_regressions_are_locked_by_rust_tests(self) -> None:
        source = text("crates/dokkomplekt-core/src/template_engine.rs")
        for expression in [
            "{{= amount.total + amount.vat + amount.fee}}",
            "{{= (3 + 2) * 2}}",
            "{{=amount.total-amount.vat}}",
            "{{= amount.total + amount.unknown + 1}}",
        ]:
            self.assertIn(expression, source)


class MorphologyAndNumbersContractTest(unittest.TestCase):
    def test_female_and_hyphenated_names_are_regression_locked(self) -> None:
        source = text("crates/dokkomplekt-morph/src/lib.rs")
        self.assertIn("Ивановой Марии Петровны", source)
        self.assertIn("Петрова-Водкина Кузьмы Сергеевича", source)
        self.assertIn("PersonNamePart::GivenName", source)
        self.assertIn("core.contains('-')", source)

    def test_large_ranks_and_negative_money_are_locked(self) -> None:
        source = text("crates/dokkomplekt-morph/src/lib.rs")
        self.assertIn('5 => ["квадриллион"', source)
        self.assertIn('6 => ["квинтиллион"', source)
        self.assertIn("number_to_words_ru(i64::MIN)", source)
        self.assertIn("минус сто двадцать три рубля 45 копеек", source)


class CalendarContractTest(unittest.TestCase):
    def _calendar(self) -> tuple[set[int], set[int], set[date], set[date]]:
        complete: set[int] = set()
        provisional: set[int] = set()
        holidays: set[date] = set()
        working: set[date] = set()
        for raw in text("resources/production_calendar_ru.tsv").splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            left, kind = line.split("\t")
            if len(left) == 4 and left.isdigit():
                (complete if kind == "complete" else provisional).add(int(left))
            elif kind == "holiday":
                holidays.add(date.fromisoformat(left))
            elif kind == "working":
                working.add(date.fromisoformat(left))
            else:
                self.fail(f"unexpected calendar row: {line}")
        return complete, provisional, holidays, working

    def test_supported_years_are_explicit_and_2027_is_fail_closed(self) -> None:
        complete, provisional, _, _ = self._calendar()
        self.assertEqual(complete, {2025, 2026})
        self.assertEqual(provisional, {2027})
        source = text("crates/dokkomplekt-refdata/src/lib.rs")
        self.assertIn("UnsupportedYear", source)
        self.assertNotIn("unwrap_or_else(|_|", source)

    def test_known_2025_2026_transfers_are_present(self) -> None:
        _, _, holidays, working = self._calendar()
        for item in [
            date(2025, 5, 2),
            date(2025, 5, 8),
            date(2025, 6, 13),
            date(2025, 11, 3),
            date(2025, 12, 31),
            date(2026, 3, 9),
            date(2026, 5, 11),
        ]:
            self.assertIn(item, holidays)
        self.assertIn(date(2025, 11, 1), working)

    def test_calendar_probe_matches_expected_and_refuses_2027(self) -> None:
        complete, _, holidays, working = self._calendar()

        def add_working_days(start: date, amount: int) -> date:
            if start.year not in complete:
                raise ValueError(start.year)
            step = 1 if amount >= 0 else -1
            remaining = abs(amount)
            current = start
            while remaining:
                current += timedelta(days=step)
                if current.year not in complete:
                    raise ValueError(current.year)
                is_working = current in working or (
                    current not in holidays and current.weekday() < 5
                )
                if is_working:
                    remaining -= 1
            return current

        self.assertEqual(add_working_days(date(2025, 12, 31), 1), date(2026, 1, 9))
        with self.assertRaisesRegex(ValueError, "2027"):
            add_working_days(date(2027, 12, 30), 3)


class ValidationAndDateContractTest(unittest.TestCase):
    def test_vin_scope_and_european_probe_are_locked(self) -> None:
        source = text("crates/dokkomplekt-core/src/validators.rs")
        self.assertIn("WVWZZZ1JZ3W386752", source)
        self.assertIn("matches!(first, '1' | '2' | '3' | '4' | '5')", source)

    def test_two_digit_year_uses_reference_pivot(self) -> None:
        source = text("crates/dokkomplekt-core/src/date_parser.rs")
        self.assertIn("reference_year.saturating_add(10)", source)
        self.assertIn('Some("12.05.1987")', source)
        self.assertIn('Some("12.05.2035")', source)


class DesktopSafetyAndPrintingContractTest(unittest.TestCase):
    def test_app_data_resolver_and_print_commands_are_wired(self) -> None:
        backend = text("src-tauri/src/main.rs")
        frontend = text("src/lib/api.ts")
        self.assertIn("fn reject_parent_traversal", backend)
        self.assertIn("Ожидался относительный путь внутри app_data", backend)
        self.assertIn("fn print_files", backend)
        self.assertIn("ShellExecuteW", backend)
        self.assertIn('"document-batch-ready"', backend)
        self.assertIn("ПРОВЕРИТЬ_КОМПЛЕКТ.txt", backend)
        for command in ["print_files", "open_in_file_manager"]:
            self.assertIn(f"'{command}'", frontend)

    def test_version_is_consistent_in_primary_manifests(self) -> None:
        expected = text("VERSION").strip()
        self.assertEqual(expected, "18.4.4")
        self.assertEqual(json.loads(text("package.json"))["version"], expected)
        self.assertEqual(json.loads(text("src-tauri/tauri.conf.json"))["version"], expected)


if __name__ == "__main__":
    unittest.main()
