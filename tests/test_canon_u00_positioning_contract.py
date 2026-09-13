import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

GENERAL_PRODUCT_SURFACES = (
    "README.md",
    "src/App.tsx",
    "package.json",
    "src-tauri/tauri.conf.json",
)

FORBIDDEN_GLOBAL_POSITIONING = (
    "медицинская программа",
    "программа для врачей",
    "медицинская система с дополнительными профессиями",
    "medical program",
    "software for doctors",
)

REQUIRED_EQUAL_DOMAIN_MODULES = (
    "pub mod accounting;",
    "pub mod custom;",
    "pub mod education;",
    "pub mod hr;",
    "pub mod legal;",
    "pub mod medical;",
)


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


class CanonU00PositioningContract(unittest.TestCase):
    def test_general_product_surfaces_do_not_position_product_as_medical_only(self) -> None:
        for relative in GENERAL_PRODUCT_SURFACES:
            text = read(relative).casefold()
            for forbidden in FORBIDDEN_GLOBAL_POSITIONING:
                self.assertNotIn(
                    forbidden.casefold(),
                    text,
                    f"U-00 violation in general product surface {relative!r}: {forbidden!r}",
                )

    def test_universal_core_keeps_medical_as_one_equal_domain_profile(self) -> None:
        domains = read("crates/dokkomplekt-core/src/domains/mod.rs")
        for declaration in REQUIRED_EQUAL_DOMAIN_MODULES:
            self.assertIn(declaration, domains)

        self.assertIn("domain-neutral", domains.casefold())


if __name__ == "__main__":
    unittest.main()
