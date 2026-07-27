from __future__ import annotations

import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def public_key_bytes(seed: bytes) -> bytes:
    return Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes(
        encoding=serialization.Encoding.Raw,
        format=serialization.PublicFormat.Raw,
    )

ROOT = Path(__file__).resolve().parents[1]


class ComponentDeliveryContracts(unittest.TestCase):
    def text(self, relative: str) -> str:
        return (ROOT / relative).read_text("utf-8")

    def test_component_resolution_is_user_first_and_fail_closed(self) -> None:
        source = self.text("src-tauri/src/universal_intake.rs")
        manager = self.text("src-tauri/src/component_manager.rs")
        self.assertLess(
            source.index("resolve_trusted_component_tool"),
            source.index('DOKKOMPLEKT_TOOLS_DIR'),
        )
        for invariant in [
            "component-status.json",
            "component-files.json",
            "files_manifest_sha256",
            "validate_all_manifest_files",
            "sha256_file(&candidate)",
            "Path traversal",
            "Символические ссылки",
        ]:
            self.assertIn(invariant, manager)

    def test_download_guards_reuse_update_network_trust(self) -> None:
        manager = self.text("src-tauri/src/component_manager.rs")
        for invariant in [
            "validate_update_url",
            "pinned_update_client",
            "allowed_hosts",
            "Content-Length",
            "MAX_COMPONENT_ARCHIVE_BYTES",
            "Хеш компонента не совпал",
            "std::fs::rename(&stage_dir, &final_dir)",
            '"component://progress"',
        ]:
            self.assertIn(invariant, manager)

    def test_ui_gates_missing_dependencies_instead_of_only_showing_error(self) -> None:
        app = self.text("src/App.tsx")
        center = self.text("src/components/AutomationControlCenter.tsx")
        for invariant in [
            "ensureOptionalComponent",
            "Разовая загрузка; после установки компонент работает офлайн",
            "installComponent(id)",
            "ensureComponentForSource",
        ]:
            self.assertIn(invariant, app)
        for invariant in [
            "Дополнительные возможности",
            "component://progress",
            "Проверить подписанный каталог",
            "Скачать",
            "Удалить",
        ]:
            self.assertIn(invariant, center)

    def test_release_pipeline_builds_thin_offline_and_component_artifacts(self) -> None:
        workflow = self.text(".github/workflows/build-installers.yml")
        for invariant in [
            "build_component_packs.py",
            "tauri.thin.conf.json",
            "release-installers/thin",
            "release-installers/offline",
            "release-components/**",
            "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64",
            "DOKKOMPLEKT_UPDATE_PUBKEY_B64",
        ]:
            self.assertIn(invariant, workflow)

    def test_component_pack_builder_is_deterministic_and_signed(self) -> None:
        target = "contract-test-target"
        target_dir = ROOT / "src-tauri" / "resources" / "tools" / target
        output_dir = Path(tempfile.mkdtemp(prefix="dokkomplekt-components-"))
        try:
            (target_dir / "tesseract").mkdir(parents=True, exist_ok=True)
            (target_dir / "poppler" / "bin").mkdir(parents=True, exist_ok=True)
            files = {
                "tesseract/tesseract": b"fake-tesseract-v1",
                "poppler/bin/pdftotext": b"fake-pdftotext-v1",
                "poppler/bin/pdftoppm": b"fake-pdftoppm-v1",
            }
            status_files = []
            for relative, content in files.items():
                path = target_dir / relative
                path.write_bytes(content)
                status_files.append(
                    {
                        "tool": "tesseract" if relative.startswith("tesseract/") else "poppler",
                        "path": relative,
                        "sha256": hashlib.sha256(content).hexdigest(),
                        "executable": True,
                    }
                )
            (target_dir / "sidecar-status.json").write_text(
                json.dumps({"schema": 1, "target": target, "files": status_files}),
                "utf-8",
            )
            seed = bytes(range(32))
            env = {**os.environ, "DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64": base64.b64encode(seed).decode("ascii"), "DOKKOMPLEKT_UPDATE_PUBKEY_B64": base64.b64encode(public_key_bytes(seed)).decode("ascii")}
            command = [
                sys.executable,
                str(ROOT / "scripts" / "build_component_packs.py"),
                "--target", target,
                "--components", "ocr",
                "--app-min-version", "18.3.2",
                "--base-url", "https://downloads.example.com/dokkomplekt/18.3.0",
                "--out", str(output_dir),
                "--require-trusted-public-key",
            ]
            subprocess.run(command, cwd=ROOT, env=env, check=True, capture_output=True, text=True)
            pack = output_dir / f"ocr-{target}.zip"
            catalog_path = output_dir / "components-catalog.json"
            self.assertTrue(pack.is_file())
            self.assertTrue(catalog_path.is_file())
            with zipfile.ZipFile(pack) as archive:
                names = set(archive.namelist())
                self.assertIn("component-files.json", names)
                self.assertIn("tesseract/tesseract", names)
                manifest_bytes = archive.read("component-files.json")
                manifest = json.loads(manifest_bytes)
                self.assertEqual(manifest["component_id"], "ocr")
                self.assertEqual(manifest["target"], target)
            catalog = json.loads(catalog_path.read_text("utf-8"))
            self.assertRegex(catalog["payload"]["published_at"], r"^\d{4}-\d{2}-\d{2}T")
            descriptor = catalog["payload"]["components"][0]
            self.assertEqual(descriptor["sha256"], hashlib.sha256(pack.read_bytes()).hexdigest())
            self.assertEqual(descriptor["files_manifest_sha256"], hashlib.sha256(manifest_bytes).hexdigest())
            canonical = json.dumps(catalog["payload"], ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
            Ed25519PrivateKey.from_private_bytes(seed).public_key().verify(base64.b64decode(catalog["signature"]), canonical)

            first = hashlib.sha256(pack.read_bytes()).hexdigest()
            shutil.rmtree(output_dir)
            output_dir.mkdir()
            subprocess.run(command, cwd=ROOT, env=env, check=True, capture_output=True, text=True)
            self.assertEqual(first, hashlib.sha256((output_dir / pack.name).read_bytes()).hexdigest())
        finally:
            shutil.rmtree(target_dir, ignore_errors=True)
            shutil.rmtree(output_dir, ignore_errors=True)

    def test_builder_rejects_tampered_staged_file(self) -> None:
        source = self.text("scripts/build_component_packs.py")
        self.assertIn("staged file changed after verification", source)
        self.assertIn("unsafe component path", source)
        self.assertIn("DOKKOMPLEKT_UPDATE_PRIVATE_KEY_B64", source)


if __name__ == "__main__":
    unittest.main()
