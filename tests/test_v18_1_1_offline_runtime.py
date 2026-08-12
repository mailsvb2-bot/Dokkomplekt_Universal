import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "assert_offline_runtime_ready.py"
SOURCE_CHECKPOINT = ROOT / "src-tauri" / "resources" / "tools" / "windows-x86_64" / "sidecar-status.json"


def load_module():
    spec = importlib.util.spec_from_file_location("assert_offline_runtime_ready", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OfflineRuntimeVerificationTests(unittest.TestCase):
    def stage(self, root: Path, include_model: bool = True) -> tuple[object, str, Path]:
        module = load_module()
        module.TOOLS_ROOT = root
        target = "windows-x86_64"
        target_dir = root / target
        files = [
            ("tesseract", "tesseract/tesseract.exe"),
            ("tesseract", "tesseract/tessdata/rus.traineddata"),
            ("tesseract", "tesseract/tessdata/eng.traineddata"),
            ("poppler", "poppler/bin/pdftotext.exe"),
            ("poppler", "poppler/bin/pdftoppm.exe"),
            ("libreoffice", "libreoffice/program/soffice.exe"),
            ("sumatrapdf", "sumatrapdf/SumatraPDF.exe"),
            ("7zip", "7zip/7z.exe"),
            ("7zip", "7zip/7z.dll"),
        ]
        if include_model:
            files.extend([
                ("llama_cpp", "llama_cpp/llama-server.exe"),
                ("semantic_model", "semantic_model/dokkomplekt-instruct.gguf"),
            ])
        entries = []
        for index, (tool, relative) in enumerate(files):
            path = target_dir / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(f"verified-{index}-{relative}".encode())
            entries.append({
                "tool": tool,
                "path": relative,
                "sha256": module.sha256_file(path),
                "size_bytes": path.stat().st_size,
            })
        (target_dir / "sidecar-status.json").write_text(json.dumps({
            "schema": 1,
            "target": target,
            "network_used": False,
            "files": entries,
        }), "utf-8")
        return module, target, target_dir

    def test_complete_runtime_and_model_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            module, target, _ = self.stage(Path(temporary))
            target_dir, status = module.load_status(target)
            tools = module.verify_entries(target_dir, status)
            module.verify_required_runtime(tools, True)
            self.assertEqual(
                set(tools),
                {
                    "tesseract",
                    "poppler",
                    "libreoffice",
                    "sumatrapdf",
                    "7zip",
                    "llama_cpp",
                    "semantic_model",
                },
            )
            self.assertNotIn("msgconvert", tools)

    def test_source_checkpoint_matches_native_msg_runtime_surface(self) -> None:
        status = json.loads(SOURCE_CHECKPOINT.read_text("utf-8"))
        missing = set(status["missing_required_tools"])

        self.assertFalse(status["ready"])
        self.assertFalse(status["supply_chain_locked"])
        self.assertNotIn("msgconvert", missing)
        self.assertEqual(
            missing,
            {
                "tesseract",
                "tessdata/rus",
                "tessdata/eng",
                "poppler/pdftotext",
                "poppler/pdftoppm",
                "libreoffice/soffice",
                "7zip",
                "sumatrapdf",
                "llama_cpp/llama-server",
                "approved_gguf_model",
            },
        )

    def test_missing_model_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            module, target, _ = self.stage(Path(temporary), include_model=False)
            target_dir, status = module.load_status(target)
            tools = module.verify_entries(target_dir, status)
            with self.assertRaisesRegex(ValueError, "llama_cpp"):
                module.verify_required_runtime(tools, True)

    def test_tampered_sidecar_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            module, target, target_dir = self.stage(Path(temporary))
            (target_dir / "sumatrapdf/SumatraPDF.exe").write_bytes(b"tampered")
            loaded_dir, status = module.load_status(target)
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                module.verify_entries(loaded_dir, status)


if __name__ == "__main__":
    unittest.main()
