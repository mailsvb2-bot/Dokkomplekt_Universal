from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "create_offline_runtime_bundle.py"


def load_module():
    spec = importlib.util.spec_from_file_location("create_offline_runtime_bundle", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OfflineRuntimeBundleTests(unittest.TestCase):
    def stage(self, root: Path):
        target = "windows-x86_64"
        target_dir = root / target
        files = []
        for index, (tool, relative) in enumerate(
            [
                ("tesseract", "tesseract/tesseract.exe"),
                ("tesseract", "tesseract/tessdata/rus.traineddata"),
                ("tesseract", "tesseract/tessdata/eng.traineddata"),
                ("poppler", "poppler/bin/pdftotext.exe"),
                ("poppler", "poppler/bin/pdftoppm.exe"),
                ("libreoffice", "libreoffice/program/soffice.exe"),
                ("sumatrapdf", "sumatrapdf/SumatraPDF.exe"),
                ("7zip", "7zip/7z.exe"),
                ("llama_cpp", "llama_cpp/llama-server.exe"),
                ("semantic_model", "semantic_model/model.gguf"),
            ]
        ):
            path = target_dir / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"runtime-{index}-{relative}".encode())
            files.append(
                {
                    "tool": tool,
                    "path": relative,
                    "sha256": __import__("hashlib").sha256(path.read_bytes()).hexdigest(),
                    "size_bytes": path.stat().st_size,
                }
            )
        return target, target_dir, {"schema": 1, "target": target, "network_used": False, "files": files}

    def test_bundle_is_deterministic_and_contains_verified_sbom(self) -> None:
        module = load_module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target, target_dir, status = self.stage(root / "tools")
            out1 = root / "out1"
            out2 = root / "out2"
            with mock.patch.object(module, "load_verified_status", return_value=(target_dir, status)):
                bundle1, payload1, metadata1 = module.create_bundle(target, out1, True)
                bundle2, payload2, metadata2 = module.create_bundle(target, out2, True)
            self.assertEqual(bundle1.read_bytes(), bundle2.read_bytes())
            self.assertEqual(metadata1["bundle_sha256"], metadata2["bundle_sha256"])
            self.assertEqual(json.loads(payload1.read_text()), json.loads(payload2.read_text()))
            with zipfile.ZipFile(bundle1) as archive:
                self.assertIsNone(archive.testzip())
                sbom = json.loads(archive.read("runtime-sbom.json"))
                self.assertTrue(sbom["semantic_model_required"])
                self.assertEqual(len(sbom["files"]), 10)
                self.assertIn(
                    "runtime/windows-x86_64/semantic_model/model.gguf",
                    archive.namelist(),
                )

    def test_release_signature_is_fail_closed_when_key_is_missing(self) -> None:
        module = load_module()
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(FileNotFoundError):
                module.sign_payload(Path(temporary) / "payload.json", Path(temporary) / "missing.pem")


if __name__ == "__main__":
    unittest.main()
