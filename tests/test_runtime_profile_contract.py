from __future__ import annotations

import unittest

from scripts._runtime_profile import (
    CORE_PROFILE,
    FULL_PROFILE,
    include_tool,
    normalize_profile,
    validate_profile_file_set,
)


CORE_FILES = [
    {"tool": "tesseract"},
    {"tool": "poppler"},
    {"tool": "libreoffice"},
    {"tool": "sumatrapdf"},
    {"tool": "7zip"},
]


class RuntimeProfileContractTests(unittest.TestCase):
    def test_core_is_explicit_and_rejects_semantic_flag(self) -> None:
        self.assertEqual(
            normalize_profile("core", semantic_model_required=False),
            CORE_PROFILE,
        )
        with self.assertRaises(ValueError):
            normalize_profile("core", semantic_model_required=True)

    def test_legacy_semantic_payload_maps_only_to_full(self) -> None:
        self.assertEqual(
            normalize_profile(None, semantic_model_required=True),
            FULL_PROFILE,
        )
        with self.assertRaises(ValueError):
            normalize_profile(None, semantic_model_required=False)

    def test_core_excludes_semantic_tools_but_keeps_document_runtime(self) -> None:
        for tool in ("tesseract", "poppler", "libreoffice", "sumatrapdf", "7zip"):
            self.assertTrue(include_tool(CORE_PROFILE, tool))
        self.assertFalse(include_tool(CORE_PROFILE, "llama_cpp"))
        self.assertFalse(include_tool(CORE_PROFILE, "semantic_model"))

    def test_core_requires_all_document_runtime_tools(self) -> None:
        validate_profile_file_set(CORE_PROFILE, CORE_FILES)
        with self.assertRaises(ValueError):
            validate_profile_file_set(CORE_PROFILE, CORE_FILES[:-1])

    def test_core_cannot_accidentally_embed_semantic_payload(self) -> None:
        with self.assertRaises(ValueError):
            validate_profile_file_set(
                CORE_PROFILE,
                [*CORE_FILES, {"tool": "semantic_model"}],
            )

    def test_full_requires_both_semantic_runtime_and_model(self) -> None:
        validate_profile_file_set(
            FULL_PROFILE,
            [*CORE_FILES, {"tool": "llama_cpp"}, {"tool": "semantic_model"}],
        )
        with self.assertRaises(ValueError):
            validate_profile_file_set(FULL_PROFILE, [*CORE_FILES, {"tool": "llama_cpp"}])


if __name__ == "__main__":
    unittest.main()
