from __future__ import annotations

import re
import unittest
from pathlib import Path

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def source(path: str) -> str:
    return project_text(path)


class V1808ZeroTouchHardeningContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.main = source("src-tauri/src/main.rs")
        cls.created_documents = source("crates/dokkomplekt-core/src/created_documents.rs")
        cls.morph = source("crates/dokkomplekt-morph/src/lib.rs")
        cls.intake = source("src-tauri/src/universal_intake.rs")

    def test_attention_report_uses_canonical_source_stem(self) -> None:
        self.assertIn("let report_name = attention_file_name(&stem);", self.main)
        self.assertNotIn("let report_name = attention_file_name(&file_name);", self.main)
        self.assertIn("legacy_attention_stem", self.main)
        self.assertIn("attention_file_name(legacy_attention_stem)", self.main)

    def test_placeholder_detection_uses_parser_and_supports_escaping(self) -> None:
        self.assertNotIn("fn has_unfilled_placeholder", self.created_documents)
        self.assertIn("angle_brackets_and_shift_operators_are_not_placeholders", self.created_documents)
        template_engine = source("crates/dokkomplekt-core/src/template_engine.rs")
        self.assertIn("ESCAPED_OPEN_SENTINEL", template_engine)
        self.assertIn("ESCAPED_CLOSE_SENTINEL", template_engine)
        self.assertIn("escaped_delimiters_are_literal_text_not_template_fields", template_engine)
        self.assertNotIn('contains("<<")', self.created_documents)
        self.assertNotIn('contains(">>")', self.created_documents)

    def test_renderer_errors_are_reported_from_structured_results(self) -> None:
        self.assertIn("result.unknown_fields", self.created_documents)
        self.assertIn("result.template_errors", self.created_documents)
        self.assertIn("неизвестный плейсхолдер", self.created_documents)
        self.assertIn("ошибка шаблона", self.created_documents)

    def test_non_windows_word_printing_converts_to_pdf_before_cups(self) -> None:
        self.assertIn("fn print_pdf_with_lp", self.main)
        self.assertIn('"--convert-to",', self.main)
        self.assertIn('"--outdir",', self.main)
        self.assertIn('convert_office_document_to_pdf(path, false)', self.main)
        self.assertIn("universal_intake::resolve_tool(\"soffice\")", self.main)
        self.assertIn("print_pdf_with_lp(&pdf, copies, preferences)", self.main)
        self.assertIn("pub(crate) fn resolve_tool", self.intake)

    def test_update_download_pins_the_ssrf_checked_dns_result(self) -> None:
        self.assertIn("struct ValidatedUpdateUrl", self.main)
        self.assertIn("addresses: Vec<SocketAddr>", self.main)
        self.assertIn("fn pinned_update_client", self.main)
        self.assertIn(".resolve_to_addrs(&validated.host, &validated.addresses)", self.main)
        self.assertIn("let client = pinned_update_client(&url)?;", self.main)

    def test_ci_runs_a_msrv_compatible_rustsec_audit(self) -> None:
        quality = source(".github/workflows/quality-gate.yml")
        installers = source(".github/workflows/build-installers.yml")
        for workflow in (quality, installers):
            self.assertIn("cargo install cargo-audit --locked --version 0.21.2", workflow)
            self.assertIn("cargo audit --deny warnings", workflow)
        self.assertIn("rust-dependency-audit", quality)

    def test_name_order_and_surname_ich_regressions_are_locked(self) -> None:
        self.assertIn("classify_person_name_parts", self.morph)
        self.assertIn("is_patronymic", self.morph)
        self.assertIn("surname_ending_in_ich_does_not_force_male_gender", self.morph)
        self.assertIn("given_name_first_order_is_detected_for_two_tokens", self.morph)
        self.assertIn('decline_person_name("Мицкевич Анна"', self.morph)
        self.assertIn('decline_person_name("Анна Петрова"', self.morph)


if __name__ == "__main__":
    unittest.main()
