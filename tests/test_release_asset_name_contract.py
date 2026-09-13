from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "validate_release_asset_names.py"
SPEC = importlib.util.spec_from_file_location("validate_release_asset_names", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
validate_release_asset_names = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validate_release_asset_names)


def test_release_asset_name_gate_accepts_unique_flat_names(tmp_path: Path) -> None:
    (tmp_path / "windows").mkdir()
    (tmp_path / "evidence").mkdir()
    (tmp_path / "windows" / "Dokkomplekt-Thin.exe").write_bytes(b"thin")
    (tmp_path / "windows" / "Dokkomplekt-Offline.exe").write_bytes(b"offline")
    (tmp_path / "evidence" / "Dokkomplekt-Evidence.zip").write_bytes(b"evidence")

    report = validate_release_asset_names.validate(tmp_path)

    assert report["valid"] is True
    assert report["asset_count"] == 3
    assert report["duplicate_basenames"] == {}


def test_release_asset_name_gate_rejects_duplicates_across_directories(tmp_path: Path) -> None:
    (tmp_path / "one").mkdir()
    (tmp_path / "two").mkdir()
    (tmp_path / "one" / "SIGNED_HANDOFF.json").write_text("one", encoding="utf-8")
    (tmp_path / "two" / "signed_handoff.JSON").write_text("two", encoding="utf-8")

    report = validate_release_asset_names.validate(tmp_path)

    assert report["valid"] is False
    duplicates = report["duplicate_basenames"]
    assert len(duplicates) == 1
    assert sorted(next(iter(duplicates.values()))) == ["one/SIGNED_HANDOFF.json", "two/signed_handoff.JSON"]


def test_release_workflow_packages_evidence_and_validates_flat_asset_namespace() -> None:
    workflow = Path(".github/workflows/build-installers.yml").read_text(encoding="utf-8")
    assert "Dokkomplekt-Universal-${version}-Windows-x64-Thin.exe" in workflow
    assert "Dokkomplekt-Universal-${version}-Windows-x64-Offline.exe" in workflow
    assert "Dokkomplekt-Production-Evidence-{os.environ['VERSION']}-{os.environ['RELEASE_SHA'][:12]}.zip" in workflow
    assert "validate_release_asset_names.py --root release" in workflow
    assert "publication-evidence/hardware" in workflow
    assert "release/evidence/hardware" not in workflow
