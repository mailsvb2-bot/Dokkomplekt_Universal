from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path
from types import SimpleNamespace


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
    libreoffice = next(command for title, command, _ in probes if title == "LibreOffice")
    assert len(libreoffice) == 1
    assert libreoffice[0].lower().endswith("soffice.exe")
    sumatrapdf = next(command for title, command, _ in probes if title == "SumatraPDF")
    assert len(sumatrapdf) == 1
    assert sumatrapdf[0].lower().endswith("sumatrapdf.exe")


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


def test_libreoffice_probe_exercises_real_headless_docx_to_pdf(monkeypatch, tmp_path: Path) -> None:
    module = load_module()
    soffice = tmp_path / "libreoffice" / "program" / "soffice.exe"
    soffice.parent.mkdir(parents=True)
    soffice.write_bytes(b"fixture")
    probe_root = tmp_path / "probe-work"
    probe_root.mkdir()
    captured: dict[str, object] = {}

    class FixedTemporaryDirectory:
        def __init__(self, *args, **kwargs):
            pass

        def __enter__(self):
            return str(probe_root)

        def __exit__(self, exc_type, exc, tb):
            return False

    def fake_run(command, **kwargs):
        captured["command"] = list(command)
        captured["kwargs"] = kwargs
        assert "--convert-to" in command
        assert command[command.index("--convert-to") + 1] == "pdf:writer_pdf_Export"
        input_path = Path(command[-1])
        assert input_path.is_file()
        assert input_path.read_bytes()[:2] == b"PK"
        output_dir = Path(command[command.index("--outdir") + 1])
        output_dir.joinpath("fixture.pdf").write_bytes(b"%PDF-1.7\n" + b"x" * 1200)
        profile_dir = probe_root / "profile"
        profile_dir.joinpath("registrymodifications.xcu").write_text("fixture", encoding="utf-8")
        return SimpleNamespace(returncode=0, stdout="converted")

    monkeypatch.setattr(module.tempfile, "TemporaryDirectory", FixedTemporaryDirectory)
    monkeypatch.setattr(module.subprocess, "run", fake_run)
    module.run_probe("LibreOffice", [str(soffice)], {0})

    command = captured["command"]
    kwargs = captured["kwargs"]
    assert "--headless" in command
    assert "--nofirststartwizard" in command
    assert any(part.startswith("-env:UserInstallation=file:") for part in command)
    assert kwargs["timeout"] == module.LIBREOFFICE_TIMEOUT_SECONDS == 120
    assert kwargs["cwd"] == soffice.parent.parent.resolve()
    assert kwargs["env"]["SAL_USE_VCLPLUGIN"] == "svp"
    assert kwargs["env"]["DOKKOMPLEKT_RUNTIME_PROBE"] == "1"



def test_sumatrapdf_probe_exercises_real_one_page_benchmark_render(monkeypatch, tmp_path: Path) -> None:
    module = load_module()
    executable = tmp_path / "sumatrapdf" / "SumatraPDF.exe"
    executable.parent.mkdir(parents=True)
    executable.write_bytes(b"fixture")
    probe_root = tmp_path / "sumatra-probe-work"
    probe_root.mkdir()
    captured: dict[str, object] = {}

    class FixedTemporaryDirectory:
        def __init__(self, *args, **kwargs):
            pass

        def __enter__(self):
            return str(probe_root)

        def __exit__(self, exc_type, exc, tb):
            return False

    def fake_run(command, **kwargs):
        captured["command"] = list(command)
        captured["kwargs"] = kwargs
        assert command[1:3] == ["-log", "-bench"]
        assert command[-1] == "1"
        sample_pdf = Path(command[-2])
        payload = sample_pdf.read_bytes()
        assert payload.startswith(b"%PDF-1.4")
        assert payload.rstrip().endswith(b"%%EOF")
        output = "\n".join(
            [
                str(sample_pdf),
                "page count: 1",
                "pageload 1: 1.23 ms",
                "pagerender 1: 2.34 ms",
                f"Finished (in 3.45 ms): {sample_pdf}",
            ]
        )
        # SumatraPDF 3.6.1 is known to return rc=1 after a successful benchmark.
        return SimpleNamespace(returncode=1, stdout=output)

    monkeypatch.setattr(module.tempfile, "TemporaryDirectory", FixedTemporaryDirectory)
    monkeypatch.setattr(module.subprocess, "run", fake_run)
    module.run_probe("SumatraPDF", [str(executable)], {0, 1})

    command = captured["command"]
    kwargs = captured["kwargs"]
    assert command[1:3] == ["-log", "-bench"]
    assert kwargs["timeout"] == module.SUMATRAPDF_TIMEOUT_SECONDS == 30
    assert kwargs["creationflags"] == int(getattr(module.subprocess, "CREATE_NO_WINDOW", 0))
    assert kwargs["env"]["DOKKOMPLEKT_RUNTIME_PROBE"] == "1"


def test_sumatrapdf_probe_rejects_false_success_without_render_evidence(monkeypatch, tmp_path: Path) -> None:
    module = load_module()
    executable = tmp_path / "SumatraPDF.exe"
    executable.write_bytes(b"fixture")
    probe_root = tmp_path / "sumatra-negative-work"
    probe_root.mkdir()

    class FixedTemporaryDirectory:
        def __init__(self, *args, **kwargs):
            pass

        def __enter__(self):
            return str(probe_root)

        def __exit__(self, exc_type, exc, tb):
            return False

    def fake_run(command, **kwargs):
        sample_pdf = Path(command[-2])
        return SimpleNamespace(
            returncode=0,
            stdout=f"{sample_pdf}\npage count: 1\nError: failed to render page 1\n",
        )

    monkeypatch.setattr(module.tempfile, "TemporaryDirectory", FixedTemporaryDirectory)
    monkeypatch.setattr(module.subprocess, "run", fake_run)
    try:
        module.run_probe("SumatraPDF", [str(executable)], {0, 1})
    except RuntimeError as exc:
        message = str(exc)
        assert "render evidence" in message or "render failures" in message
    else:
        raise AssertionError("SumatraPDF probe accepted output without proven page rendering")
