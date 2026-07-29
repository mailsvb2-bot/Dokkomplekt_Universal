from __future__ import annotations

import base64
from pathlib import Path

from scripts.release_environment_preflight import check


def b64(value: bytes) -> str:
    return base64.b64encode(value).decode("ascii")


def test_runtime_preflight_rejects_missing_or_fake_delivery_configuration(tmp_path: Path) -> None:
    report = check(
        "windows-runtime",
        {
            "DOKKOMPLEKT_COMPONENTS_CATALOG_URL": "https://catalog.invalid",
            "DOKKOMPLEKT_COMPONENTS_BASE_URL": "http://example.test/files",
            "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH": "relative.json",
        },
    )
    assert report["ok"] is False
    errors = "\n".join(report["errors"])
    assert "missing" in errors
    assert "real HTTPS URL" in errors
    assert "absolute runner-owned path" in errors


def test_runtime_preflight_accepts_complete_runner_owned_configuration(tmp_path: Path) -> None:
    manifest = tmp_path / "sidecars.json"
    manifest.write_text("{}", encoding="utf-8")
    env = {
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": b64(b"pfx"),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD": "secret",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64": b64(b"private"),
        "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64": b64(b"public"),
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64": b64(b"update-private"),
        "DOKKOMPLEKT_UPDATE_PUBKEY_B64": b64(b"update-public"),
        "DOKKOMPLEKT_COMPONENTS_CATALOG_URL": "https://downloads.example.com/catalog.json",
        "DOKKOMPLEKT_COMPONENTS_BASE_URL": "https://downloads.example.com/components",
        "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH": str(manifest.resolve()),
        "DOKKOMPLEKT_TIMESTAMP_SERVER": "https://timestamp.example.com",
    }
    assert check("windows-runtime", env)["ok"] is True


def test_hardware_preflight_requires_printer_and_absolute_reboot_evidence(tmp_path: Path) -> None:
    good = check(
        "windows-hardware",
        {
            "DOKKOMPLEKT_TEST_PRINTER": "Microsoft Print to PDF",
            "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH": str((tmp_path / "reboot.json").resolve()),
        },
    )
    assert good["ok"] is True
    bad = check(
        "windows-hardware",
        {
            "DOKKOMPLEKT_TEST_PRINTER": "Printer",
            "DOKKOMPLEKT_REBOOT_EVIDENCE_PATH": "reboot.json",
        },
    )
    assert bad["ok"] is False
