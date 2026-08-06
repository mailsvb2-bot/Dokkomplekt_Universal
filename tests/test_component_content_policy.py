from __future__ import annotations

import importlib.util
from pathlib import Path
import sys

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from scripts import build_component_packs
SPEC = importlib.util.spec_from_file_location(
    "validate_content_pack_policy", ROOT / "scripts" / "validate_content_pack.py"
)
assert SPEC and SPEC.loader
CONTENT_PACK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTENT_PACK)


@pytest.mark.parametrize(
    "value",
    (
        "C:/templates/template.docx",
        r"C:\templates\template.docx",
        "//server/share/template.docx",
        r"\\server\share\template.docx",
        "../template.docx",
        "templates/../template.docx",
        "",
    ),
)
def test_component_and_content_paths_reject_cross_platform_escapes(tmp_path: Path, value: str) -> None:
    with pytest.raises(ValueError):
        build_component_packs.safe_relative(value)
    with pytest.raises(ValueError):
        CONTENT_PACK.safe_file(tmp_path, value)


def test_component_and_content_paths_normalize_nested_backslashes(tmp_path: Path) -> None:
    expected = "templates/legal/contract.docx"
    assert build_component_packs.safe_relative(r"templates\legal\contract.docx").as_posix() == expected
    assert CONTENT_PACK.safe_file(tmp_path, r"templates\legal\contract.docx") == (
        tmp_path / expected
    ).resolve()


@pytest.mark.parametrize(
    "value",
    (
        "https://downloads.example.com/components",
        "https://127.0.0.1/components",
        "https://10.0.0.9/components",
        "https://user:secret@downloads.dokkomplekt.ru/components",
        "https://downloads.dokkomplekt.ru/components#fragment",
    ),
)
def test_component_catalog_base_url_reuses_production_url_policy(value: str) -> None:
    with pytest.raises(ValueError):
        build_component_packs.validate_public_https_url(value, "--base-url")
