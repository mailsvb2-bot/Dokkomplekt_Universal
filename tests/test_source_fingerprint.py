from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "source_fingerprint.py"
SPEC = importlib.util.spec_from_file_location("source_fingerprint", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SourceFingerprintScopeTest(unittest.TestCase):
    def test_generated_tauri_schemas_are_excluded(self) -> None:
        relative = [path.relative_to(ROOT).as_posix() for path in MODULE.iter_files()]
        self.assertFalse(any(item.startswith("src-tauri/gen/") for item in relative))
        self.assertIn("src-tauri/src/main.rs", relative)
        self.assertIn("crates/dokkomplekt-core/src/icd10_catalog.rs", relative)

    def test_generated_schema_changes_do_not_change_fingerprint(self) -> None:
        generated = ROOT / "src-tauri" / "gen" / "schemas" / "fingerprint-regression.json"
        generated.parent.mkdir(parents=True, exist_ok=True)
        before = MODULE.source_fingerprint()
        generated.write_text('{"generated": true}', encoding="utf-8")
        try:
            self.assertEqual(before, MODULE.source_fingerprint())
        finally:
            generated.unlink(missing_ok=True)


    def test_release_staged_sidecars_are_excluded(self) -> None:
        generated = ROOT / "src-tauri" / "resources" / "tools" / "windows-x86_64" / "fake-tool.exe"
        generated.parent.mkdir(parents=True, exist_ok=True)
        before = MODULE.source_fingerprint()
        generated.write_bytes(b"release-only-sidecar")
        try:
            self.assertEqual(before, MODULE.source_fingerprint())
        finally:
            generated.unlink(missing_ok=True)
            for parent in [generated.parent, generated.parent.parent, generated.parent.parent.parent]:
                try:
                    parent.rmdir()
                except OSError:
                    pass

if __name__ == "__main__":
    unittest.main()
