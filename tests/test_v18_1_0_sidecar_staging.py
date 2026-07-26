from __future__ import annotations

import hashlib
import io
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
from contextlib import redirect_stdout

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "prepare_sidecars", ROOT / "scripts" / "prepare_sidecars.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class V1810SidecarStagingContracts(unittest.TestCase):
    def test_hash_verified_sidecar_is_staged_without_network(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "tesseract.exe"
            source.write_bytes(b"controlled-sidecar-fixture")
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            manifest = root / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "schema": 1,
                        "target": "windows-test",
                        "files": [
                            {
                                "tool": "tesseract",
                                "source": str(source),
                                "target": "tesseract/tesseract.exe",
                                "sha256": digest,
                                "executable": True,
                            }
                        ],
                    }
                ),
                "utf-8",
            )
            destination = root / "staged"
            with mock.patch.object(MODULE, "DEST_ROOT", destination), mock.patch.object(
                sys, "argv", ["prepare_sidecars.py", str(manifest), "--clean"]
            ):
                with redirect_stdout(io.StringIO()):
                    self.assertEqual(MODULE.main(), 0)
            staged = destination / "windows-test" / "tesseract" / "tesseract.exe"
            self.assertEqual(staged.read_bytes(), source.read_bytes())
            status = json.loads(
                (destination / "windows-test" / "sidecar-status.json").read_text("utf-8")
            )
            self.assertFalse(status["network_used"])
            self.assertEqual(status["files"][0]["sha256"], digest)

    def test_digest_mismatch_and_path_traversal_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "converter.bin"
            source.write_bytes(b"fixture")
            manifest = root / "manifest.json"
            manifest.write_text(
                json.dumps(
                    {
                        "schema": 1,
                        "target": "linux-test",
                        "files": [
                            {
                                "tool": "libreoffice",
                                "source": str(source),
                                "target": "../soffice",
                                "sha256": "0" * 64,
                            }
                        ],
                    }
                ),
                "utf-8",
            )
            with mock.patch.object(MODULE, "DEST_ROOT", root / "staged"), mock.patch.object(
                sys, "argv", ["prepare_sidecars.py", str(manifest)]
            ):
                with self.assertRaises(ValueError):
                    MODULE.main()


if __name__ == "__main__":
    unittest.main()
