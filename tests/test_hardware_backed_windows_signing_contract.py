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
    assert "[switch] $VerifyCertificateOnly" in text
    assert "Assert-SigningKeyOperational -Certificate $cert" in text
    assert ".SignData(" in text
    assert ".VerifyData(" in text
    assert "SIGNING KEY OPERATIONAL CHALLENGE PASSED" in text
    assert "SIGNING CERTIFICATE PREFLIGHT PASSED" in text


def test_production_workflows_do_not_receive_pfx_secrets() -> None:
    private = (ROOT / "ops/private-hardware-validation/windows-hardware-e2e.yml").read_text("utf-8")
    build = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")

    assert "DOKKOMPLEKT_RELEASE_MODE: production" in private
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND: ${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_BACKEND }}" in private
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT: ${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT }}" in private
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER: ${{ vars.DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER }}" in private
    for text in (private, build):
        assert "secrets.DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64" not in text
        assert "secrets.DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD" not in text

    hosted = private[private.index("  signed-runtime-build:") : private.index("  hardware-evidence:")]
    assert "scripts/sign_windows_release.ps1 -VerifyCertificateOnly" in hosted
    assert hosted.index("-VerifyCertificateOnly") < hosted.index("Fetch immutable pre-approved runtime bundle")
    assert "inputs.reboot_phase == 'prepare'" in hosted
    assert "--profile core" in hosted

    # The public release is a dispatcher/publisher only; it must never create a second Windows binary.
    assert "  windows-signed-offline:" not in build
    assert "runs-on: windows-latest" not in build
    assert "scripts/sign_windows_release.ps1 -ArtifactRoot" not in build

    hardware = private[private.index("  hardware-evidence:") :]
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_CERT_THUMBPRINT" not in hardware
    assert "DOKKOMPLEKT_WINDOWS_SIGNING_ALLOWED_PROVIDER" not in hardware
    assert "secrets." not in hardware


def test_release_hardware_gate_routes_only_through_private_dispatcher() -> None:
    workflow = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")
    hardware = workflow.split("  windows-hardware-e2e:\n", 1)[1].split("  linux-bundles:\n", 1)[0]
    assert "runs-on: ubuntu-24.04" in hardware
    assert "environment: windows-hardware-dispatch" in hardware
    assert "runs-on: [self-hosted, Windows, X64, dokkomplekt-hardware]" not in hardware
    assert "dispatch_private_hardware_validation.py" in hardware
    assert "--reboot-phase verify" in hardware
    assert "--reuse-latest-prepare" in hardware
    assert "DOKKOMPLEKT_HARDWARE_DISPATCH_TOKEN" in hardware
    assert "Dokkomplekt-Windows-Private-Validated" in hardware
    assert "gh run download" in hardware
    assert "windows_signed_handoff.py verify" in hardware
    assert "release-installers" in hardware


def test_prepare_verify_and_publication_share_one_canonical_windows_handoff() -> None:
    private = (ROOT / "ops/private-hardware-validation/windows-hardware-e2e.yml").read_text("utf-8")
    public = (ROOT / ".github/workflows/build-installers.yml").read_text("utf-8")

    hosted = private[private.index("  signed-runtime-build:") : private.index("  hardware-evidence:")]
    hardware = private[private.index("  hardware-evidence:") :]
    assert "inputs.reboot_phase == 'prepare'" in hosted
    assert "release-installers/thin" in hosted
    assert "release-installers/offline" in hosted
    assert "--profile core" in hosted
    assert "run-id: ${{ inputs.prepare_run_id }}" in hardware
    assert "inputs.reboot_phase == 'verify'" in hardware
    assert "signed-handoff/release-installers/offline" in hardware
    assert "signed-handoff/release-runtime" in hardware

    assert "  windows-signed-offline:" not in public
    assert "gh run download \"$prepare_run_id\"" in public
    assert "gh run download \"$verify_run_id\"" in public
    assert "windows_signed_handoff.py verify private-release/handoff" in public
    assert "Dokkomplekt-Windows-Private-Validated" in public
    assert "private-windows/private-release/handoff/release-installers" in public
