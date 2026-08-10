from __future__ import annotations

import importlib.util
import re
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "release_source_identity.py"
SIGNED_EVIDENCE = ROOT / "scripts" / "write_windows_release_evidence.ps1"
HARDWARE_INDEX = ROOT / "scripts" / "write_windows_hardware_evidence_index.ps1"


def load_module():
    spec = importlib.util.spec_from_file_location("release_source_identity", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize(
    "origin",
    [
        "https://github.com/mailsvb2-bot/Dokkomplekt_Universal",
        "https://github.com/mailsvb2-bot/Dokkomplekt_Universal.git",
        "git@github.com:mailsvb2-bot/Dokkomplekt_Universal.git",
        "ssh://git@github.com/mailsvb2-bot/Dokkomplekt_Universal.git",
    ],
)
def test_canonical_github_origins_normalize(origin: str) -> None:
    module = load_module()
    assert module.normalize_github_origin(origin) == "mailsvb2-bot/Dokkomplekt_Universal"


@pytest.mark.parametrize(
    "origin",
    [
        "http://github.com/mailsvb2-bot/Dokkomplekt_Universal.git",
        "https://github.example.com/mailsvb2-bot/Dokkomplekt_Universal.git",
        "https://github.com/mailsvb2-bot/other.git",
        "https://user:secret@github.com/mailsvb2-bot/Dokkomplekt_Universal.git",
        "git@gitlab.com:mailsvb2-bot/Dokkomplekt_Universal.git",
    ],
)
def test_noncanonical_origins_fail_closed(origin: str) -> None:
    module = load_module()
    with pytest.raises(ValueError):
        module.normalize_github_origin(origin)


def test_current_checkout_identity_comes_from_git_head_and_origin() -> None:
    module = load_module()
    identity = module.resolve_identity(ROOT)
    expected_head = subprocess.check_output(
        ["git", "rev-parse", "--verify", "HEAD"], cwd=ROOT, text=True
    ).strip()
    assert identity["schema"] == "dokkomplekt.release-source-identity.v1"
    assert identity["source_repository"] == "mailsvb2-bot/Dokkomplekt_Universal"
    assert identity["release_sha"] == expected_head
    assert re.fullmatch(r"[0-9a-f]{40}", identity["release_sha"])


def test_evidence_writers_never_use_workflow_repository_sha_as_release_identity() -> None:
    signed = SIGNED_EVIDENCE.read_text(encoding="utf-8")
    hardware = HARDWARE_INDEX.read_text(encoding="utf-8")
    for source in (signed, hardware):
        assert "release_source_identity.py" in source
        assert "$env:GITHUB_SHA" not in source
        assert "source_repository" in source
        assert "release_sha" in source
    assert "Signed build evidence is not bound to the checked-out release SHA." in hardware
    assert "Signed build evidence is not bound to the checked-out source repository." in hardware


def test_hardware_index_preserves_single_backslash_path_normalization() -> None:
    source = HARDWARE_INDEX.read_text(encoding="utf-8")
    assert "$repoRoot.TrimEnd([char[]]@('\\', '/'))" in source
    assert ".Replace('\\', '/')" in source
    assert "$repoRoot.TrimEnd([char[]]@('\\\\', '/'))" not in source
    assert ".Replace('\\\\', '/')" not in source
