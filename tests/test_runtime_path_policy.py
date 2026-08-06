from __future__ import annotations

import pytest

from scripts import assert_offline_runtime_ready
from scripts import create_offline_runtime_bundle
from scripts import create_runtime_lock
from scripts import prepare_sidecars
from scripts import probe_offline_runtime
from scripts import release_environment_preflight
from scripts._release_policy import validate_relative_runtime_path


@pytest.mark.parametrize(
    "value",
    (
        "C:/runtime/tool.exe",
        r"C:\\runtime\\tool.exe",
        "//server/share/tool.exe",
        r"\\server\share\tool.exe",
        "../tool.exe",
        "tool/../escape.exe",
        "",
    ),
)
def test_cross_platform_runtime_paths_fail_closed(value: str) -> None:
    with pytest.raises(ValueError):
        validate_relative_runtime_path(value)
    for validator in (
        create_runtime_lock.safe_relative,
        prepare_sidecars.safe_relative,
        release_environment_preflight.safe_relative,
        probe_offline_runtime.safe_relative,
        assert_offline_runtime_ready.safe_relative,
    ):
        with pytest.raises(ValueError):
            validator(value)


def test_runtime_paths_normalize_backslashes() -> None:
    expected = "poppler/bin/pdftotext.exe"
    assert validate_relative_runtime_path(r"poppler\bin\pdftotext.exe") == expected
    assert create_runtime_lock.safe_relative(r"poppler\bin\pdftotext.exe") == expected
    assert prepare_sidecars.safe_relative(r"poppler\bin\pdftotext.exe").as_posix() == expected


@pytest.mark.parametrize(
    "value",
    (
        "https://downloads.example.com/tool.zip",
        "https://127.0.0.1/tool.zip",
        "https://10.10.0.7/tool.zip",
        "https://user:secret@downloads.dokkomplekt.ru/tool.zip",
        "https://downloads.dokkomplekt.ru/tool.zip#fragment",
    ),
)
def test_runtime_lock_rejects_nonproduction_provenance(value: str) -> None:
    with pytest.raises(ValueError):
        create_runtime_lock.validated_source_url({"source_url": value}, 0)


def test_runtime_lock_accepts_public_https_and_urn_provenance() -> None:
    assert create_runtime_lock.validated_source_url(
        {"source_url": "https://downloads.dokkomplekt.ru/runtime/tool.zip"}, 0
    ).startswith("https://")
    assert create_runtime_lock.validated_source_url(
        {"source_url": "urn:sha256:" + "a" * 64}, 0
    ).startswith("urn:sha256:")


def test_offline_bundle_rejects_windows_absolute_paths(monkeypatch: pytest.MonkeyPatch, tmp_path) -> None:
    target = "windows-x86_64"
    target_dir = tmp_path / target
    target_dir.mkdir(parents=True)
    status = {
        "files": [
            {
                "tool": "tesseract",
                "path": "C:/runtime/tesseract.exe",
                "sha256": "0" * 64,
            }
        ],
        "supply_chain_locked": True,
    }
    monkeypatch.setattr(
        create_offline_runtime_bundle,
        "load_verified_status",
        lambda *_args, **_kwargs: (target_dir, status),
    )
    with pytest.raises(ValueError, match="unsafe relative path"):
        create_offline_runtime_bundle.create_bundle(target, tmp_path / "out", False, True)
