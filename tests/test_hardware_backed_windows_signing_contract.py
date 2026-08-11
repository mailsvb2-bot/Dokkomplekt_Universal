from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load_hosted_preflight():
    path = ROOT / "scripts" / "verify_windows_hosted_signing_runner.py"
    spec = importlib.util.spec_from_file_location("hosted_signing_preflight", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_hosted_signer_accepts_hardware_backed_certificate_store_contract() -> None:
    module = _load_hosted_preflight()
    checked, errors = module.validate_windows_signing_backend(
        {
            "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND": "certificate-store",
            "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT": "A" * 40,
            "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER": "SafeNet Key Storage Provider",
        }
    )
    assert errors == []
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND" in checked


def test_hosted_signer_rejects_pfx_backend_and_pfx_material() -> None:
    module = _load_hosted_preflight()
    _, errors = module.validate_windows_signing_backend(
        {
            "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND": "pfx",
            "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT": "A" * 40,
            "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER": "SafeNet Key Storage Provider",
            "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": "ZmFrZQ==",
            "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD": "not-a-real-secret",
        }
    )
    assert any("requires 'certificate-store'" in item for item in errors)
    assert sum("forbidden" in item for item in errors) >= 2


def test_hosted_signer_rejects_known_software_key_provider() -> None:
    module = _load_hosted_preflight()
    _, errors = module.validate_windows_signing_backend(
        {
            "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND": "certificate-store",
            "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT": "A" * 40,
            "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER": "Microsoft Software Key Storage Provider",
        }
    )
    assert any("software-backed provider is forbidden" in item for item in errors)


def test_signing_script_forbids_exportable_production_pfx() -> None:
    text = (ROOT / "scripts" / "sign_windows_release.ps1").read_text(encoding="utf-8")
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND" in text
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT" in text
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER" in text
    assert "Production signing private key is exportable" in text
    assert "legacy pfx backend is forbidden in production" in text
    assert "-Exportable" not in text
    assert "SignerCertificate.Thumbprint" in text


def test_production_workflows_do_not_receive_pfx_secrets() -> None:
    private = (
        ROOT / "ops" / "private-hardware-validation" / "windows-hardware-e2e.yml"
    ).read_text(encoding="utf-8")
    build = (ROOT / ".github" / "workflows" / "build-installers.yml").read_text(
        encoding="utf-8"
    )

    for text in (private, build):
        assert "DOKKOMPLEKT_RELEASE_MODE: production" in text
        assert (
            "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND: "
            "${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND }}"
        ) in text
        assert (
            "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT: "
            "${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT }}"
        ) in text
        assert (
            "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER: "
            "${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER }}"
        ) in text
        assert "secrets.DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64" not in text
        assert "secrets.DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD" not in text

    hardware = private[private.index("  hardware-evidence:") :]
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT" not in hardware
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER" not in hardware
    assert "secrets." not in hardware
