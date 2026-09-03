from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "probe_offline_runtime.py"
EXPECTED_TITLES = [
    "Tesseract",
    "Poppler pdftotext",
    "Poppler pdftoppm",
    "LibreOffice",
    "SumatraPDF",
    "7-Zip",
    "llama.cpp",
]


def load_module():
    spec = importlib.util.spec_from_file_location("probe_offline_runtime", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_external_runtime_probe_surface_is_exactly_seven_and_has_no_msgconvert() -> None:
    module = load_module()
    entries = [
        ("tesseract", "tesseract/tesseract.exe"),
        ("poppler", "poppler/bin/pdftotext.exe"),
        ("poppler", "poppler/bin/pdftoppm.exe"),
        ("libreoffice", "libreoffice/program/soffice.exe"),
        ("sumatrapdf", "sumatrapdf/SumatraPDF.exe"),
        ("7zip", "7zip/7z.exe"),
        ("llama_cpp", "llama_cpp/llama-server.exe"),
    ]

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        status = {"files": []}
        for tool, relative in entries:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture")
            status["files"].append({"tool": tool, "path": relative})

        probes = module.runtime_probes(root, status)

    assert [title for title, _, _ in probes] == EXPECTED_TITLES
    assert len(probes) == 7
    flattened = " ".join(part for _, command, _ in probes for part in command).lower()
    assert "msgconvert" not in flattened


def test_core_runtime_probe_surface_omits_semantic_server() -> None:
    module = load_module()
    entries = [
        ("tesseract", "tesseract/tesseract.exe"),
        ("poppler", "poppler/bin/pdftotext.exe"),
        ("poppler", "poppler/bin/pdftoppm.exe"),
        ("libreoffice", "libreoffice/program/soffice.exe"),
        ("sumatrapdf", "sumatrapdf/SumatraPDF.exe"),
        ("7zip", "7zip/7z.exe"),
    ]
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        status = {"files": []}
        for tool, relative in entries:
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture")
            status["files"].append({"tool": tool, "path": relative})
        probes = module.runtime_probes(root, status, "core")
    assert [title for title, _, _ in probes] == EXPECTED_TITLES[:-1]
    assert len(probes) == 6
