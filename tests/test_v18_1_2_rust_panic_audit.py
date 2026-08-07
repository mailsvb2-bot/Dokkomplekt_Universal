from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "audit_rust_production_panics.py"


def load_module():
    spec = importlib.util.spec_from_file_location("audit_rust_production_panics", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RustPanicAuditTests(unittest.TestCase):
    def test_current_production_tree_has_no_direct_panic_shortcuts(self) -> None:
        self.assertEqual(load_module().violations(ROOT), [])

    def test_production_unwrap_is_found_but_test_unwrap_is_ignored(self) -> None:
        module = load_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "src").mkdir()
            (root / "tests").mkdir()
            (root / "src/lib.rs").write_text(
                "fn bad() { let _ = Some(1).unwrap(); }\n"
                "#[cfg(test)] mod tests { #[test] fn ok() { Some(1).unwrap(); } }\n",
                "utf-8",
            )
            (root / "tests/integration.rs").write_text("fn test() { Some(1).unwrap(); }", "utf-8")
            old_root = module.ROOT
            module.ROOT = root
            try:
                found = module.violations(root)
            finally:
                module.ROOT = old_root
            self.assertEqual(len(found), 1)
            self.assertIn("src/lib.rs:1", found[0])

    def test_multiline_cfg_test_module_with_custom_name_is_ignored(self) -> None:
        module = load_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text(
                "fn production() -> Option<u8> { Some(1) }\n"
                "#[cfg(test)]\n"
                "mod processing_guard_fencing_tests {\n"
                "    #[test]\n"
                "    fn ok() { Some(1).expect(\"test-only assertion\"); }\n"
                "}\n",
                "utf-8",
            )
            old_root = module.ROOT
            module.ROOT = root
            try:
                found = module.violations(root)
            finally:
                module.ROOT = old_root
            self.assertEqual(found, [])


if __name__ == "__main__":
    unittest.main()
