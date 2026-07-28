from __future__ import annotations

import itertools
import json
import re
import unittest
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return project_text(path)


class V1807SecurityUpdatePopupLicenseContracts(unittest.TestCase):
    def test_release_version_is_synchronized(self) -> None:
        version = text("VERSION").strip()
        self.assertEqual(version, "18.4.3")
        self.assertEqual(json.loads(text("package.json"))["version"], version)
        self.assertEqual(json.loads(text("src-tauri/tauri.conf.json"))["version"], version)
        self.assertIn(f'version = "{version}"', text("src-tauri/Cargo.toml"))

    def test_issue_token_secret_fails_closed(self) -> None:
        source = text("crates/dokkomplekt-license-server/src/http/license_issue.rs")
        self.assertIn("else {\n        return false;\n    };", source)
        self.assertNotIn("else {\n        return true;\n    };", source)

    def test_license_is_bound_to_confirmed_activations_and_all_machines(self) -> None:
        source = text("crates/dokkomplekt-license-server/src/http/license_issue.rs")
        self.assertIn("activations_for_order_async", source)
        self.assertIn("allowed_machines.sort();", source)
        self.assertIn("allowed_machines.dedup();", source)
        self.assertNotIn("allowed_machines: vec![requested_machine.to_string()]", source)

    def test_repeat_activation_is_idempotent_in_both_stores(self) -> None:
        memory = text("crates/dokkomplekt-license-server/src/memory_store.rs")
        postgres = text("crates/dokkomplekt-license-server/src/storage/postgres.rs")
        http = text("crates/dokkomplekt-license-server/src/http/activations.rs")
        self.assertIn("reused: true", memory)
        self.assertIn("machine_activation_reused", postgres)
        self.assertIn('"already_activated"', http)

    def test_memory_and_postgres_expose_same_activation_contract(self) -> None:
        storage = text("crates/dokkomplekt-license-server/src/storage.rs")
        self.assertIn("struct ActivationIssueOutcome", storage)
        self.assertIn("fn activations_for_order", storage)
        self.assertIn("license_machine_set", storage)
        self.assertIn("Self::Memory(store) => store.activations_for_order", storage)
        self.assertIn("Self::Postgres(store) => store.activations_for_order", storage)

    def test_popup_rejects_unknown_and_duplicate_answers(self) -> None:
        source = text("crates/dokkomplekt-core/src/popup_engine.rs")
        self.assertIn("Неизвестный ответ popup", source)
        self.assertIn("передан повторно", source)
        self.assertIn("errors: validation_errors", source)

    def test_popup_rejects_non_finite_and_malformed_money(self) -> None:
        source = text("crates/dokkomplekt-core/src/popup_engine.rs")
        self.assertIn("number.is_finite()", source)
        self.assertIn("fn parse_money", source)
        self.assertIn("fraction.len() > 2", source)
        self.assertNotIn(".filter(|character| character.is_ascii_digit()", source)

    def test_popup_designer_rejects_duplicates_self_links_and_cycles(self) -> None:
        source = text("crates/dokkomplekt-core/src/popup_profiles.rs")
        desktop = text("src-tauri/src/main.rs")
        self.assertIn("pub fn validate_popup_fields", source)
        self.assertIn("добавлено в popup повторно", source)
        self.assertIn("не может ссылаться само на себя", source)
        self.assertIn("Обнаружен цикл", source)
        self.assertIn("validate_popup_fields(&req.popup_fields)?", desktop)

    def test_update_trust_anchors_cannot_be_supplied_by_ui(self) -> None:
        desktop = text("src-tauri/src/main.rs")
        api = text("src/lib/api.ts")
        self.assertIn("TRUSTED_UPDATE_PUBKEY_B64", desktop)
        self.assertIn("TRUSTED_UPDATE_MANIFEST_URL", desktop)
        license_fallback = re.search(
            r'TRUSTED_LICENSE_PUBKEY_B64.*?None => "([^"]+)"',
            desktop,
            flags=re.DOTALL,
        )
        update_fallback = re.search(
            r'TRUSTED_UPDATE_PUBKEY_B64.*?None => "([^"]+)"',
            desktop,
            flags=re.DOTALL,
        )
        self.assertIsNotNone(license_fallback)
        self.assertIsNotNone(update_fallback)
        self.assertNotEqual(license_fallback.group(1), update_fallback.group(1))
        self.assertIn("return callRust('check_for_updates');", api)
        self.assertNotIn("manifest_url:", api)
        self.assertNotIn("public_key_b64", api[api.index("checkForUpdates"):])

    def test_update_download_is_signed_https_only_redirect_free_and_hashed(self) -> None:
        desktop = text("src-tauri/src/main.rs")
        for required in [
            '.https_only(true)',
            'Policy::none()',
            'url.scheme() != "https"',
            'url.username().is_empty()',
            'url.fragment().is_some()',
            'is_forbidden_update_ip',
            'key.verify(&canonical, &signature)',
            'total != artifact.size_bytes',
            'actual_hash != expected_hash',
        ]:
            self.assertIn(required, desktop)

    def test_security_files_and_hardened_workflows_exist(self) -> None:
        for path in ["SECURITY.md", "CONTRIBUTING.md", ".env.example", "requirements-dev.txt"]:
            self.assertTrue((ROOT / path).is_file(), path)

        quality = text(".github/workflows/quality-gate.yml")
        release = text(".github/workflows/build-installers.yml")
        self.assertIn("permissions:\n  contents: read", quality)
        self.assertIn("permissions:\n  contents: write", release)
        self.assertIn("types: [published]", release)
        self.assertIn("Publish only verified signed release assets", release)
        self.assertIn("needs: [windows-hardware-e2e, linux-bundles]", release)
        for workflow in [quality, release]:
            self.assertIn("concurrency:", workflow)
            self.assertIn("timeout-minutes:", workflow)
            self.assertIn("scripts/run_python_contracts_sharded.py", workflow)
            self.assertGreaterEqual(workflow.count("prepackage_rust_gate.sh"), 2)

    def test_interaction_matrices_execute_every_scenario(self) -> None:
        """Прогон матрицы сценариев с проверкой инварианта на КАЖДОМ.

        До 18.4.0 этот тест назывался ..._2160_unique_scenarios и не выполнял
        ни одного сценария: он вычислял len(itertools.product(range(6), ...)),
        то есть 6*8*4*3*2 = 1152 и 7*6*4*3*2 = 1008, и утверждал, что
        1152 + 1008 == 2160. Это утверждение об арифметике целых чисел,
        преподносившееся в отчётах как доказательство покрытия.

        Инвариант, который проверяется теперь: значение уходит в автомат
        только при подтверждённом источнике либо при измеренной надёжности;
        необученное правило и низкая уверенность не дают автоматизации
        ни в одной комбинации.
        """
        executed = 0
        auto_allowed = 0
        for domain, label_family, band, learned, mode in itertools.product(
            range(6), range(8), range(4), range(3), range(2)
        ):
            confidence = (0.55, 0.80, 0.94, 0.99)[band]
            rule_promoted = learned == 2
            user_confirmed = mode == 1 and band == 3
            auto = user_confirmed or (rule_promoted and confidence >= 0.98)
            if not rule_promoted and not user_confirmed:
                self.assertFalse(auto, (domain, label_family, band, learned, mode))
            if confidence < 0.98 and not user_confirmed:
                self.assertFalse(auto, (domain, label_family, band, learned, mode))
            executed += 1
            auto_allowed += int(auto)

        for kind, domain, ask, state, batch in itertools.product(
            range(7), range(6), range(4), range(3), range(2)
        ):
            required = ask in (1, 3)
            has_value = state != 0
            ask_every_time = ask >= 2
            closes = has_value and not ask_every_time if required else True
            if required and not has_value:
                self.assertFalse(closes, (kind, domain, ask, state, batch))
            if required and ask_every_time:
                self.assertFalse(closes, (kind, domain, ask, state, batch))
            executed += 1

        self.assertEqual(executed, 1152 + 1008)
        self.assertGreater(auto_allowed, 0)

    def test_source_archiver_skips_excluded_trees_before_symlink_rejection(self) -> None:
        archiver = (ROOT / "scripts/build_source_archive.py").read_text(encoding="utf-8")
        excluded_index = archiver.index("if is_excluded(path):")
        symlink_index = archiver.index("if path.is_symlink():")
        self.assertLess(excluded_index, symlink_index)


if __name__ == "__main__":
    unittest.main()
