from pathlib import Path
import unittest

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return project_text(relative)


class GuidedWordScannerContracts(unittest.TestCase):
    def test_word_is_opened_waited_for_activated_and_closed_by_backend(self) -> None:
        main = read("src-tauri/src/main.rs")
        self.assertIn("fn activate_word_document", main)
        self.assertIn("GetActiveObject('Word.Application')", main)
        self.assertIn("AddSeconds({timeout})", main)
        self.assertIn("$target.Activate()", main)
        self.assertIn("shell_execute_path(&opened, \"open\").and_then", main)
        self.assertIn("fn close_word_scanner", main)
        self.assertIn("$target.Close", main)

    def test_source_scanner_keeps_word_open_for_retry_then_closes_after_confirm(self) -> None:
        app = read("src/App.tsx")
        self.assertIn("captureWordScanner(guidedScanner.session.session_id, false)", app)
        self.assertIn("async function retryGuidedScannerSelection", app)
        self.assertIn("activateWordScanner(current.session.session_id)", app)
        self.assertIn("closeWordScanner(current.session.session_id, false)", app)
        self.assertIn("Word закрыт автоматически", app)

    def test_simple_ui_hides_technical_choices_until_requested(self) -> None:
        modal = read("src/components/GuidedScannerModal.tsx")
        self.assertIn("Ничего настраивать и запоминать не нужно", modal)
        self.assertIn("Word не видно? Открыть документ ещё раз", modal)
        self.assertIn("Выделить другое", modal)
        self.assertIn("Да, всё правильно", modal)
        self.assertIn("Нет, это другое значение", modal)
        self.assertIn("Нужного варианта нет — создать своё поле", modal)
        self.assertIn("<details className=\"scannerAlternatives\"", modal)
        self.assertNotIn("<code>{selected.field_id}</code>", modal)

    def test_program_explains_real_and_suggested_destinations(self) -> None:
        modal = read("src/components/GuidedScannerModal.tsx")
        self.assertIn("Готовые места найдены", modal)
        self.assertIn("Предлагаемый документ", modal)
        self.assertIn("место в шаблоне можно показать тем же сканером", modal)

    def test_scanner_suggestions_cover_multiple_professions_without_making_core_medical(self) -> None:
        suggestions = read("src/lib/scannerSuggestions.ts")
        for expected in (
            "contract.number",
            "hr.order_number",
            "accounting.invoice_number",
            "education.student_name",
            "medical.case_number",
            "document.number",
        ):
            self.assertIn(expected, suggestions)
        modal = read("src/components/GuidedScannerModal.tsx")
        self.assertNotIn("пациент", modal.lower())
        self.assertNotIn("врач", modal.lower())

    def test_template_scanner_only_changes_a_safe_copy(self) -> None:
        main = read("src-tauri/src/main.rs")
        app = read("src/App.tsx")
        self.assertIn("scanner_copy_path", main)
        self.assertIn("std::fs::copy(&original, &copy)", main)
        self.assertIn("join(\"scanner-copies\")", main)
        self.assertIn("makeWorkingCopy", read("src/lib/api.ts"))
        self.assertIn("startWordScanner(document.template_path, 'template', true)", app)
        self.assertIn("Исходный Word-файл останется нетронутым", read("src/components/GuidedScannerModal.tsx"))

    def test_frontend_and_backend_command_surfaces_include_activation(self) -> None:
        api = read("src/lib/api.ts")
        main = read("src-tauri/src/main.rs")
        self.assertIn("export async function activateWordScanner", api)
        self.assertIn("'activate_word_scanner'", api)
        self.assertIn("fn activate_word_scanner", main)
        self.assertIn("activate_word_scanner,", main)


if __name__ == "__main__":
    unittest.main()
