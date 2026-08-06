from __future__ import annotations

import base64
import datetime as dt
import hashlib
import json
from pathlib import Path

from scripts.release_environment_preflight import check


def b64(value: bytes) -> str:
    return base64.b64encode(value).decode("ascii")


def public_build_env() -> dict[str, str]:
    return {
        "DOKKOMPLEKT_GATE_PUBKEY_B64": b64(b"g" * 32),
        "DOKKOMPLEKT_LICENSE_PUBKEY_B64": b64(b"l" * 32),
        "DOKKOMPLEKT_UPDATE_PUBKEY_B64": b64(b"u" * 32),
        "DOKKOMPLEKT_THRESHOLD_PUBKEY_B64": b64(b"t" * 32),
        "DOKKOMPLEKT_REFDATA_PUBKEY_B64": b64(b"r" * 32),
        "DOKKOMPLEKT_UPDATE_MANIFEST_URL": "https://updates.dokkomplekt.ru/update.json",
        "DOKKOMPLEKT_REFDATA_URL": "https://updates.dokkomplekt.ru/reference-data.json",
        "DOKKOMPLEKT_COMPONENTS_CATALOG_URL": "https://downloads.dokkomplekt.ru/catalog.json",
        "DOKKOMPLEKT_COMPONENTS_BASE_URL": "https://downloads.dokkomplekt.ru/components",
    }


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_runtime_manifest(tmp_path: Path) -> Path:
    license_file = tmp_path / "LICENSE.txt"
    license_file.write_text("reviewed license", encoding="utf-8")
    specs = {
        "tesseract": "tesseract/tesseract.exe",
        "poppler": "poppler/pdftotext.exe",
        "libreoffice": "libreoffice/program/soffice.exe",
        "sumatrapdf": "sumatrapdf/SumatraPDF.exe",
        "7zip": "7zip/7z.exe",
        "llama_cpp": "llama_cpp/llama-server.exe",
        "semantic_model": "semantic_model/model.gguf",
    }
    files = []
    inventory_tools: dict[str, list[str]] = {}
    for tool, target in specs.items():
        source = tmp_path / f"{tool}.bin"
        source.write_bytes(f"fixture:{tool}".encode())
        files.append(
            {
                "tool": tool,
                "source": str(source),
                "target": target,
                "sha256": sha256(source),
                "executable": tool != "semantic_model",
                "version": "1.0.0-reviewed",
                "source_url": f"https://downloads.dokkomplekt.ru/runtime/{tool}",
                "license": "Reviewed-License",
                "license_file": str(license_file),
                "license_sha256": sha256(license_file),
            }
        )
        inventory_tools.setdefault(tool, []).append(target)
    inventory = tmp_path / "runtime-inventory.json"
    inventory.write_text(
        json.dumps({"schema": 1, "tools": inventory_tools}), encoding="utf-8"
    )
    manifest = tmp_path / "sidecars.json"
    manifest.write_text(
        json.dumps(
            {
                "schema": 1,
                "target": "windows-x86_64",
                "supply_chain_locked": True,
                "files": files,
                "distribution_review": {
                    "complete_portable_tree": True,
                    "reviewer": "release-owner",
                    "reviewed_at": (dt.date.today() - dt.timedelta(days=1)).isoformat(),
                    "scope": "complete portable runtime",
                    "inventory_file": str(inventory),
                    "inventory_sha256": sha256(inventory),
                },
            }
        ),
        encoding="utf-8",
    )
    return manifest


def test_production_build_requires_real_compile_time_trust_anchors() -> None:
    assert check("production-build", public_build_env())["ok"] is True
    broken = public_build_env()
    broken.pop("DOKKOMPLEKT_LICENSE_PUBKEY_B64")
    broken["DOKKOMPLEKT_UPDATE_MANIFEST_URL"] = "https://updates.invalid/manifest.json"
    report = check("production-build", broken)
    assert report["ok"] is False
    errors = "\n".join(report["errors"])
    assert "DOKKOMPLEKT_LICENSE_PUBKEY_B64: missing" in errors
    assert "placeholder or local host is forbidden" in errors


def test_production_build_rejects_documentation_credentials_and_private_hosts() -> None:
    cases = (
        "https://updates.example.com/manifest.json",
        "https://user:secret@updates.dokkomplekt.ru/manifest.json",
        "https://127.0.0.1/manifest.json",
        "https://10.0.0.7/manifest.json",
        "https://updates.dokkomplekt.ru/manifest.json#fragment",
    )
    for value in cases:
        env = public_build_env()
        env["DOKKOMPLEKT_UPDATE_MANIFEST_URL"] = value
        report = check("production-build", env)
        assert report["ok"] is False, value


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
    assert "public HTTPS URL" in errors or "placeholder or local host" in errors
    assert "absolute runner-owned path" in errors


def test_runtime_preflight_rejects_unlocked_or_placeholder_manifest(tmp_path: Path) -> None:
    manifest = tmp_path / "sidecars.json"
    manifest.write_text("{}", encoding="utf-8")
    env = {
        **public_build_env(),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": b64(b"pfx"),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD": "secret",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64": b64(b"private"),
        "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64": b64(b"public"),
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64": b64(b"p" * 32),
        "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH": str(manifest.resolve()),
    }
    report = check("windows-runtime", env)
    assert report["ok"] is False
    errors = "\n".join(report["errors"])
    assert "manifest schema must be 1" in errors
    assert "supply_chain_locked must be true" in errors


def test_runtime_preflight_rejects_tampered_source_and_windows_absolute_target(tmp_path: Path) -> None:
    manifest = write_runtime_manifest(tmp_path)
    data = json.loads(manifest.read_text("utf-8"))
    data["files"][0]["source"] = str(tmp_path / "tampered.bin")
    (tmp_path / "tampered.bin").write_bytes(b"tampered")
    data["files"][1]["target"] = "C:/runtime/pdftotext.exe"
    manifest.write_text(json.dumps(data), encoding="utf-8")
    env = {
        **public_build_env(),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": b64(b"pfx"),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD": "secret",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64": b64(b"private"),
        "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64": b64(b"public"),
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64": b64(b"p" * 32),
        "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH": str(manifest.resolve()),
    }
    report = check("windows-runtime", env)
    assert report["ok"] is False
    errors = "\n".join(report["errors"])
    assert "files[0].sha256 mismatch" in errors
    assert "files[1].target is unsafe" in errors


def test_runtime_preflight_accepts_complete_runner_owned_configuration(tmp_path: Path) -> None:
    manifest = write_runtime_manifest(tmp_path)
    env = {
        **public_build_env(),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_B64": b64(b"pfx"),
        "DOKKOMPLEKT_WINDOWS_SIGNING_PFX_PASSWORD": "secret",
        "DOKKOMPLEKT_RUNTIME_SIGNING_KEY_PEM_B64": b64(b"private"),
        "DOKKOMPLEKT_RUNTIME_TRUSTED_PUBKEY_PEM_B64": b64(b"public"),
        "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64": b64(b"p" * 32),
        "DOKKOMPLEKT_SIDECAR_MANIFEST_PATH": str(manifest.resolve()),
        "DOKKOMPLEKT_TIMESTAMP_SERVER": "https://timestamp.dokkomplekt.ru",
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


def test_example_environment_lists_all_public_build_inputs_without_fake_endpoints() -> None:
    env_path = Path(__file__).resolve().parents[1] / ".env.example"
    values = {}
    for raw in env_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value

    for key in public_build_env():
        assert key in values, f"{key} must be documented in .env.example"

    for key in (
        "DOKKOMPLEKT_UPDATE_MANIFEST_URL",
        "DOKKOMPLEKT_COMPONENTS_CATALOG_URL",
        "DOKKOMPLEKT_COMPONENTS_BASE_URL",
        "DOKKOMPLEKT_REFDATA_URL",
        "DOKKOMPLEKT_QUEUE_MTLS_URL",
        "DOKKOMPLEKT_LICENSE_PUBLIC_URL",
    ):
        assert values[key] == "", f"{key} must stay blank until a real endpoint is supplied"

    env = env_path.read_text(encoding="utf-8").lower()
    for forbidden in ("updates.example.com", "licenses.example.com", "queue.example.internal"):
        assert forbidden not in env


def test_release_and_queue_docs_use_current_variable_names_and_explicit_placeholders() -> None:
    root = Path(__file__).resolve().parents[1]
    release = (root / "docs" / "PRODUCTION_RELEASE_BOOTSTRAP.md").read_text(encoding="utf-8")
    queue = (root / "docs" / "QUEUE_SERVICE_DEPLOYMENT.md").read_text(encoding="utf-8")
    assert "DOKKOMPLEKT_REFDATA_MANIFEST_URL" not in release
    assert "DOKKOMPLEKT_REFDATA_URL" in release
    assert "queue.example.internal" not in queue
    assert "https://queue.<YOUR_REAL_DOMAIN>:9443" in queue
