from __future__ import annotations

from pathlib import Path
import re
import unittest

from source_helpers import project_text

ROOT = Path(__file__).resolve().parents[1]


class ScannerWatcherPrintContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.suggestions = (ROOT / "src/lib/scannerSuggestions.ts").read_text(encoding="utf-8")
        cls.suggestion_tests = (ROOT / "src/lib/scannerSuggestions.test.ts").read_text(encoding="utf-8")
        cls.app = (ROOT / "src/App.tsx").read_text(encoding="utf-8")
        cls.watcher_sync = (ROOT / "src/hooks/useWatcherPreferenceSync.ts").read_text(encoding="utf-8")
        cls.api = (ROOT / "src/lib/api.ts").read_text(encoding="utf-8")
        cls.main = project_text("src-tauri/src/main.rs")
        cls.workspace = (ROOT / "src/components/Workspace.tsx").read_text(encoding="utf-8")
        cls.guided_modal = (ROOT / "src/components/GuidedScannerModal.tsx").read_text(encoding="utf-8")
        cls.runtime_validation = (ROOT / "src/lib/runtimeValidation.ts").read_text(encoding="utf-8")

    def test_short_scanner_keywords_use_token_boundaries(self) -> None:
        self.assertIn("containsTokenSequence", self.suggestions)
        self.assertIn(r"/[\p{L}\p{N}]+|№/gu", self.suggestions)
        self.assertNotIn("haystack.includes(keyword", self.suggestions)
        self.assertIn("Работник ознакомлен с договором. Адрес:", self.suggestion_tests)
        self.assertIn("not.toContain('«от»')", self.suggestion_tests)
        self.assertIn("not.toContain('«до»')", self.suggestion_tests)

    def test_ambiguous_scanner_result_is_not_auto_confirmed(self) -> None:
        body = re.search(
            r"export function bestScannerSuggestion\([^)]*\)[^{]*\{(?P<body>.*?)\n\}",
            self.suggestions,
            flags=re.DOTALL,
        )
        self.assertIsNotNone(body)
        text = body.group("body")
        self.assertIn("return first", text)
        self.assertIn("return null", text)
        self.assertIn("expect(bestScannerSuggestion(ambiguous)).toBeNull()", self.suggestion_tests)

    def test_uncertain_modal_does_not_display_first_suggestion_as_confirmed(self) -> None:
        self.assertNotIn("?? props.suggestions[0]", self.guided_modal)
        self.assertIn('open={!selected}', self.guided_modal)
        self.assertIn('!props.selectedFieldId.trim()', self.guided_modal)

    def test_learned_rules_affect_scanner_ranking(self) -> None:
        self.assertIn("learnedRules?: LearnedScannerRule[]", self.suggestions)
        self.assertIn("learnedEvidenceForField", self.suggestions)
        self.assertIn("listLearnedScannerRules", self.app)
        self.assertRegex(self.app, r"suggestScannerFields\(\{[\s\S]*?learnedRules,")

    def test_tie_breaking_uses_registry_order_not_title_alphabet(self) -> None:
        self.assertIn("left.order - right.order", self.suggestions)
        self.assertNotIn("title.localeCompare", self.suggestions)

    def test_non_windows_scanner_cfg_consumes_windows_only_arguments(self) -> None:
        self.assertIn('let _ = (&req.path, req.make_working_copy, &state, &app);', self.main)
        self.assertIn('let _ = &capture;', self.main)

    def test_unreadable_watcher_file_is_visible_and_retry_is_bounded(self) -> None:
        self.assertIn(' — НЕ ПРОЧИТАН.txt', self.main)
        self.assertIn("write_unreadable_source_note", self.main)
        self.assertIn("unreadable_note_blocks_retry(&unreadable_note, &path, now)", self.main)
        self.assertIn("retry_after_unix_ms=", self.main)
        self.assertIn('Some("content_change") => true', self.main)
        self.assertIn('Some("timed") =>', self.main)
        self.assertIn('_ => false', self.main)
        self.assertNotIn('status: "error".into()', self.main)
        self.assertGreaterEqual(self.main.count('status: "attention".into()'), 2)
        self.assertIn("'processed', 'attention', 'setup_needed', 'ignored'", self.runtime_validation)
        self.assertIn("временная будет повторена автоматически", self.main)
        self.assertIn("НЕ ПРОЧИТАН.txt", self.workspace)

    def test_watcher_receives_changed_print_preferences_without_reinstall(self) -> None:
        self.assertIn("updateBackgroundWatcherPreferences", self.api)
        self.assertIn("update_background_watcher_preferences", self.api)
        self.assertIn("fn update_background_watcher_preferences", self.main)
        self.assertIn("useWatcherPreferenceSync", self.app)
        self.assertIn("getBackgroundWatcherState", self.watcher_sync)
        self.assertIn("watcherPreferencesReady", self.watcher_sync)
        self.assertIn("outputPreferencesReady", self.watcher_sync)
        self.assertIn("folderNamingConfirmed", self.watcher_sync)
        self.assertRegex(
            self.watcher_sync,
            r"updateBackgroundWatcherPreferences\(outputRoot, folderParts, autoPrint, printCopies\)",
        )
        self.assertIn("последнюю подтверждённую конфигурацию", self.watcher_sync)
        self.assertIn("latest_runtime", self.main)
        self.assertIn("effective_copies", self.main)

    def test_word_copies_are_queued_in_one_com_print_call(self) -> None:
        self.assertIn("fn print_word_document_copies", self.main)
        self.assertIn("$doc.PrintOut", self.main)
        self.assertIn("{copies}", self.main)
        self.assertIn(
            "match print_path_copies(path, *requested_copies, preferences)",
            self.main,
        )
        self.assertIn("preferences: &PrintPreferences", self.main)

    def test_release_version_is_synchronized(self) -> None:
        expected = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
        self.assertEqual(expected, "18.4.3")


if __name__ == "__main__":
    unittest.main()
