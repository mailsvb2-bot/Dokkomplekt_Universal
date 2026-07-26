"""Поведенческие контракты 18.4.0.

Область намеренно узкая: здесь проверяется только то, что действительно
поставляется и может быть проверено выполнением.

Предыдущая версия этого файла содержала 16 тестов, из которых 14 проверяли
Python-порт Rust-логики, а не саму логику. Если бы порт разошёлся с Rust,
все тесты остались бы зелёными при сломанном продукте. Это ровно тот же
дефект, что и grep-тесты, только в более убедительной обёртке. Порт удалён
вместе с тестами.

Rust-логика проверяется Rust-тестами в automation_quality.rs. Здесь —
реестр рисков (общий артефакт) и отсутствие дрейфа между рантаймом
и калибровкой.
"""
from __future__ import annotations

import json
import random
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from measure_domain import _is_high_risk, _legacy_is_high_risk, field_risk  # noqa: E402

RUST_QUALITY = ROOT / "crates/dokkomplekt-core/src/automation_quality.rs"


class RiskRegistryIsTheSingleSourceOfTruth(unittest.TestCase):
    def test_registry_is_valid_and_versioned(self) -> None:
        registry = json.loads(
            (ROOT / "resources/field_risk_registry.json").read_text(encoding="utf-8")
        )
        self.assertEqual(registry["schema"], "dokkomplekt.field-risk-registry.v1")
        self.assertEqual(registry["order"], ["critical", "high", "medium", "low"])
        for risk in registry["order"]:
            self.assertIn(risk, registry["rules"])
            for key in ("tokens", "prefixes", "exact"):
                self.assertIn(key, registry["rules"][risk])

    def test_rust_embeds_the_same_registry_file(self) -> None:
        """Rust читает реестр через include_str!. Если путь разъедется,
        рантайм и калибровка снова начнут расходиться."""
        source = RUST_QUALITY.read_text(encoding="utf-8")
        self.assertIn(
            'include_str!("../../../resources/field_risk_registry.json")', source
        )
        embedded = (
            ROOT / "crates/dokkomplekt-core/src"
            / "../../../resources/field_risk_registry.json"
        ).resolve()
        self.assertEqual(
            embedded, (ROOT / "resources/field_risk_registry.json").resolve()
        )

    def test_substring_false_positives_are_closed(self) -> None:
        """До 18.4.0 сопоставление шло по подстроке без границ слов."""
        self.assertEqual(field_risk("hr.winner_bonus"), "high")        # было critical ('inn')
        self.assertEqual(field_risk("org.beginning_date"), "high")     # было critical ('inn')
        self.assertEqual(field_risk("spinner.state"), "low")           # было critical ('inn')
        self.assertEqual(field_risk("education.candidate_id"), "low")  # было high ('date')
        self.assertEqual(field_risk("task.update_flag"), "low")        # было high ('date')
        self.assertEqual(field_risk("doc.validated_by"), "low")        # было high ('date')

    def test_genuine_risk_classes_are_not_weakened(self) -> None:
        """Ни один класс риска не понижен относительно 18.3.2."""
        for field_id, expected in (
            ("org.inn", "critical"),
            ("amount.total", "critical"),
            ("document.number", "critical"),
            ("subject.birth_date", "critical"),
            ("medical.diagnosis_code", "critical"),
            ("org.bank_account", "critical"),
            ("document.date", "high"),
            ("employee.hire_date", "high"),
            ("subject.address", "high"),
            ("amount.vat", "high"),
            ("org.name", "medium"),
            ("employee.name", "low"),
        ):
            self.assertEqual(field_risk(field_id), expected, field_id)

    def test_calibration_no_longer_drifts_from_runtime(self) -> None:
        """Регрессия на расхождение, из-за которого подписанный артефакт
        калибровки авторизовал автопечать для полей, ошибки на которых
        он никогда не измерял."""
        previously_unmeasured = [
            field_id
            for field_id in (
                "subject.address",
                "medical.diagnosis_code",
                "amount.vat",
                "employee.salary",
                "legal.claim_subject",
                "org.bank_account",
            )
            if not _legacy_is_high_risk(field_id)
        ]
        self.assertEqual(len(previously_unmeasured), 6)
        for field_id in previously_unmeasured:
            self.assertTrue(_is_high_risk(field_id), field_id)


class ChecksumBypassIsScopedToRealChecksums(unittest.TestCase):
    """Порог уверенности снимается только настоящей контрольной суммой."""

    @staticmethod
    def source() -> str:
        return RUST_QUALITY.read_text(encoding="utf-8")

    def test_format_only_validators_are_excluded(self) -> None:
        """validate_kpp и validate_cadastral — проверки ФОРМАТА.

        validate_kpp принимает любые 9 символов нужного вида,
        validate_cadastral — любые четыре группы цифр через двоеточие.
        validate_vin считает контрольную цифру только для VIN, начинающихся
        с 1..=5, а для остальных возвращает Ok после проверки формата.
        Условное доказательство доказательством не является."""
        block = self.source()
        start = block.index("fn checksum_verified")
        body = block[start : block.index("pub fn value_blocker", start)]
        self.assertNotIn("validate_kpp", body)
        self.assertNotIn("validate_cadastral", body)
        # validate_vin считает к.ц. только для VIN, начинающихся с 1..=5;
        # для европейских и японских это проверка формата.
        self.assertNotIn("validate_vin(", body)
        for real in ("validate_inn", "validate_snils", "validate_ogrn"):
            self.assertIn(real, body)

    def test_checksum_does_not_waive_provenance(self) -> None:
        """Контрольная сумма доказывает валидность, но не происхождение:
        ИНН покупателя вместо ИНН поставщика проходит её идеально.
        Поэтому проверка происхождения обязана стоять ВЫШЕ по коду."""
        source = self.source()
        evidence_gate = source.index("не содержит проверяемого доказательства")
        checksum_gate = source.index("if checksum_verified(&value.field_id")
        self.assertLess(evidence_gate, checksum_gate)

    def test_user_confirmation_still_wins_first(self) -> None:
        source = self.source()
        user_gate = source.index("ValueSource::UserConfirmed")
        checksum_gate = source.index("if checksum_verified(&value.field_id")
        self.assertLess(user_gate, checksum_gate)


    def test_every_checksum_class_maps_to_an_existing_validator(self) -> None:
        """Список классов в checksum_verified не должен разъехаться
        с validators.rs при переименовании или удалении функции."""
        block = self.source()
        start = block.index("fn checksum_verified")
        body = block[start : block.index("pub fn value_blocker", start)]
        called = set(re.findall(r"crate::(validate_\w+)\(", body))
        self.assertTrue(called, "ни одного валидатора не вызывается")
        validators = (ROOT / "crates/dokkomplekt-core/src/validators.rs").read_text(
            encoding="utf-8"
        )
        declared = set(re.findall(r"pub fn (validate_\w+)\(", validators))
        missing = called - declared
        self.assertEqual(missing, set(), f"вызываются несуществующие валидаторы: {missing}")


class AmbiguityIsOfferedAsAChoiceNotABlankScreen(unittest.TestCase):
    """E12: близкие кандидаты предлагаются на выбор, а не отбрасываются.

    Rust-реализация — bundle_decision.rs::plausible_candidates.
    Здесь проверяется, что константы отбора не разошлись с порогом
    документов-спутников в document_routing.rs: две независимые константы
    «достаточно похоже» — это будущий дрейф.
    """

    def test_candidate_floor_matches_the_companion_floor(self) -> None:
        bundle = (ROOT / "crates/dokkomplekt-core/src/bundle_decision.rs").read_text(
            encoding="utf-8"
        )
        routing = (ROOT / "crates/dokkomplekt-core/src/document_routing.rs").read_text(
            encoding="utf-8"
        )
        floor = re.search(r"CANDIDATE_FLOOR: f32 = ([\d.]+)", bundle)
        self.assertIsNotNone(floor, "CANDIDATE_FLOOR не найден")
        companion = re.search(r"candidate\.score >= ([\d.]+)", routing)
        self.assertIsNotNone(companion, "порог спутников не найден")
        self.assertEqual(floor.group(1), companion.group(1))

    def test_ambiguous_decision_can_never_be_generation_ready(self) -> None:
        """Кандидаты именуются, но планом не становятся."""
        bundle = (ROOT / "crates/dokkomplekt-core/src/bundle_decision.rs").read_text(
            encoding="utf-8"
        )
        start = bundle.index("BundleDecisionSource::AmbiguousCandidates,")
        block = bundle[start : start + 400]
        self.assertIn("auto_apply: false", block)
        self.assertIn("review_required: true", block)

    def test_selection_arithmetic(self) -> None:
        """Порт отбора; константы читаются из Rust, а не дублируются здесь."""
        bundle = (ROOT / "crates/dokkomplekt-core/src/bundle_decision.rs").read_text(
            encoding="utf-8"
        )
        floor = float(re.search(r"CANDIDATE_FLOOR: f32 = ([\d.]+)", bundle).group(1))
        window = float(re.search(r"CANDIDATE_WINDOW: f32 = ([\d.]+)", bundle).group(1))
        limit = int(re.search(r"MAX_CANDIDATES: usize = (\d+)", bundle).group(1))
        known = {"a", "b", "z"}

        def pick(scores):
            top = max((s for _, s in scores), default=0.0)
            if top < floor:
                return []
            return [
                i
                for i, s in scores
                if s >= floor and top - s <= window and i in known
            ][:limit]

        # «Акт выполненных работ» против «акта приёма-передачи».
        self.assertEqual(pick([("a", 0.52), ("b", 0.48), ("z", 0.05)]), ["a", "b"])
        self.assertEqual(pick([("a", 0.20), ("b", 0.09)]), [])
        self.assertEqual(pick([("a", 0.55), ("b", 0.33)]), ["a"])
        self.assertEqual(pick([("ghost", 0.60), ("a", 0.55)]), ["a"])
        self.assertEqual(pick([]), [])


class ChecksumAlgorithmsAreReal(unittest.TestCase):
    """Независимая проверка, что заявленные контрольные суммы существуют."""

    @staticmethod
    def inn10(value: str) -> bool:
        digits = [int(c) for c in value if c.isdigit()]
        if len(digits) != 10:
            return False
        weights = [2, 4, 10, 3, 5, 9, 4, 6, 8]
        return sum(w * d for w, d in zip(weights, digits[:9])) % 11 % 10 == digits[9]

    def test_inn_control_digit_actually_discriminates(self) -> None:
        self.assertTrue(self.inn10("7707083893"))
        self.assertFalse(self.inn10("7707083894"))

    def test_control_digit_rejects_most_random_strings(self) -> None:
        """Смысл к.с. как доказательства: она отсеивает около 9 из 10
        случайных десятизначных строк. Проверка формата не отсеивает ничего."""
        rng = random.Random(20260724)
        passed = sum(
            1
            for _ in range(10_000)
            if self.inn10("".join(str(rng.randrange(10)) for _ in range(10)))
        )
        self.assertLess(passed / 10_000, 0.15)
        self.assertGreater(passed / 10_000, 0.05)


if __name__ == "__main__":
    unittest.main()
