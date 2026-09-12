#!/usr/bin/env python3
"""Execute staged Windows sidecars before an installer may be released.

``core`` probes only the document-processing executables embedded by the stock
installer. ``full`` additionally probes llama.cpp. The GGUF itself is data and
is verified by the offline-runtime integrity gate.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path
from typing import Iterable
from xml.sax.saxutils import escape

try:
    from scripts._release_policy import validate_relative_runtime_path
    from scripts._runtime_profile import CORE_PROFILE, FULL_PROFILE, PROFILES, normalize_profile
except ModuleNotFoundError:
    from _release_policy import validate_relative_runtime_path
    from _runtime_profile import CORE_PROFILE, FULL_PROFILE, PROFILES, normalize_profile

ROOT = Path(__file__).resolve().parents[1]
TOOLS_ROOT = ROOT / "src-tauri" / "resources" / "tools"
TIMEOUT_SECONDS = 20
LIBREOFFICE_TIMEOUT_SECONDS = 120
SUMATRAPDF_TIMEOUT_SECONDS = 30


def safe_relative(value: str) -> Path:
    return Path(validate_relative_runtime_path(value, "staged runtime path"))


def load_status(target: str) -> tuple[Path, dict]:
    target_dir = (TOOLS_ROOT / target).resolve()
    target_dir.relative_to(TOOLS_ROOT.resolve())
    status_path = target_dir / "sidecar-status.json"
    if not status_path.is_file():
        raise FileNotFoundError(f"missing verified runtime status: {status_path}")
    status = json.loads(status_path.read_text("utf-8"))
    if status.get("target") != target or not isinstance(status.get("files"), list):
        raise ValueError("sidecar status does not match requested target")
    return target_dir, status


def tool_path(target_dir: Path, status: dict, tool: str, names: Iterable[str]) -> Path:
    expected = {name.lower() for name in names}
    for item in status["files"]:
        relative = safe_relative(str(item.get("path", "")))
        if str(item.get("tool", "")).lower() == tool and relative.name.lower() in expected:
            path = target_dir / relative
            if path.is_file():
                return path
    raise FileNotFoundError(f"runtime entry point is missing for {tool}: {sorted(expected)}")


def runtime_probes(
    target_dir: Path, status: dict, profile: str = FULL_PROFILE
) -> list[tuple[str, list[str], set[int]]]:
    normalize_profile(profile, semantic_model_required=(profile == FULL_PROFILE))
    probes = [
        ("Tesseract", [str(tool_path(target_dir, status, "tesseract", ["tesseract.exe"])), "--version"], {0}),
        ("Poppler pdftotext", [str(tool_path(target_dir, status, "poppler", ["pdftotext.exe"])), "-v"], {0, 1, 99}),
        ("Poppler pdftoppm", [str(tool_path(target_dir, status, "poppler", ["pdftoppm.exe"])), "-v"], {0, 1, 99}),
        ("LibreOffice", [str(tool_path(target_dir, status, "libreoffice", ["soffice.exe"]))], {0}),
        ("SumatraPDF", [str(tool_path(target_dir, status, "sumatrapdf", ["sumatrapdf.exe"]))], {0, 1}),
        ("7-Zip", [str(tool_path(target_dir, status, "7zip", ["7z.exe", "7zz.exe"])), "i"], {0}),
    ]
    if profile == FULL_PROFILE:
        probes.append(("llama.cpp", [str(tool_path(target_dir, status, "llama_cpp", ["llama-server.exe", "server.exe"])), "--version"], {0}))
    return probes


def _write_minimal_docx(path: Path) -> None:
    lines = [
        "DOKKOMPLEKT OFFLINE LIBREOFFICE RELEASE PROBE",
        "Russian: ПРИВЕТ МИР",
        "Headless DOCX to PDF conversion from the staged runtime.",
    ]
    paragraphs = "".join(
        '<w:p><w:r><w:t xml:space="preserve">' + escape(line) + "</w:t></w:r></w:p>"
        for line in lines
    )
    document_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        "<w:body>"
        + paragraphs
        + '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>'
        "</w:body></w:document>"
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="word/document.xml"/>'
        "</Relationships>"
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in (
            ("[Content_Types].xml", content_types),
            ("_rels/.rels", rels),
            ("word/document.xml", document_xml),
        ):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            archive.writestr(info, data.encode("utf-8"))


def _execute_libreoffice_probe(command: list[str]) -> None:
    soffice = Path(command[0]).resolve()
    runtime_root = soffice.parent.parent
    with tempfile.TemporaryDirectory(prefix="dokkomplekt-lo-probe-") as directory:
        work = Path(directory)
        profile_dir = work / "profile"
        output_dir = work / "output"
        input_path = work / "fixture.docx"
        profile_dir.mkdir()
        output_dir.mkdir()
        _write_minimal_docx(input_path)
        probe_command = [
            str(soffice),
            "--headless",
            "--nologo",
            "--nodefault",
            "--nofirststartwizard",
            f"-env:UserInstallation={profile_dir.resolve().as_uri()}",
            "--convert-to",
            "pdf:writer_pdf_Export",
            "--outdir",
            str(output_dir),
            str(input_path),
        ]
        completed = subprocess.run(
            probe_command,
            cwd=runtime_root,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=LIBREOFFICE_TIMEOUT_SECONDS,
            check=False,
            env={
                **os.environ,
                "DOKKOMPLEKT_RUNTIME_PROBE": "1",
                "SAL_USE_VCLPLUGIN": "svp",
            },
        )
        tail = completed.stdout[-1200:].strip()
        if completed.returncode != 0:
            raise RuntimeError(
                f"LibreOffice headless conversion failed (exit {completed.returncode}). Output: {tail}"
            )
        pdf_path = output_dir / "fixture.pdf"
        if not pdf_path.is_file():
            raise RuntimeError(f"LibreOffice produced no PDF. Output: {tail}")
        if pdf_path.stat().st_size < 1000:
            raise RuntimeError(
                f"LibreOffice produced an implausibly small PDF: {pdf_path.stat().st_size} bytes"
            )
        with pdf_path.open("rb") as stream:
            if not stream.read(8).startswith(b"%PDF-"):
                raise RuntimeError("LibreOffice output is not a PDF")
        if not any(profile_dir.iterdir()):
            raise RuntimeError("LibreOffice isolated profile was not initialized")
    print("RUNTIME PROBE OK: LibreOffice (headless DOCX->PDF)")


def _write_minimal_pdf(path: Path) -> None:
    content = b"BT /F1 24 Tf 72 720 Td (DOKKOMPLEKT SUMATRA RELEASE PROBE) Tj ET"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        b"<< /Length " + str(len(content)).encode("ascii") + b" >>\nstream\n" + content + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    data = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for index, body in enumerate(objects, start=1):
        offsets.append(len(data))
        data.extend(f"{index} 0 obj\n".encode("ascii"))
        data.extend(body)
        data.extend(b"\nendobj\n")
    xref_offset = len(data)
    data.extend(f"xref\n0 {len(objects) + 1}\n".encode("ascii"))
    data.extend(b"0000000000 65535 f \n")
    for offset in offsets[1:]:
        data.extend(f"{offset:010d} 00000 n \n".encode("ascii"))
    data.extend(
        (
            f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\n"
            f"startxref\n{xref_offset}\n%%EOF\n"
        ).encode("ascii")
    )
    path.write_bytes(bytes(data))


def _validate_sumatrapdf_output(output: str, sample_pdf: Path) -> None:
    normalized = output.replace("/", "\\")
    expected_path = str(sample_pdf).replace("/", "\\")
    required = {
        "sample path": expected_path.lower() in normalized.lower(),
        "page count": re.search(r"(?im)^page count:\s*1\s*$", output) is not None,
        "page load": re.search(r"(?im)^pageload\s+1:\s*[0-9.]+\s*ms\s*$", output) is not None,
        "page render": re.search(r"(?im)^pagerender\s+1:\s*[0-9.]+\s*ms\s*$", output) is not None,
        "benchmark finished": re.search(r"(?im)^Finished \(in [0-9.]+ ms\):", output) is not None,
    }
    missing = [name for name, passed in required.items() if not passed]
    if missing:
        raise RuntimeError(
            f"SumatraPDF benchmark lacks required render evidence: {missing}. Output: {output[-5000:]}"
        )
    forbidden = (
        "Error: failed to load page 1",
        "Error: failed to load ",
        "Error: failed to render page 1",
        "failed to create engine",
        "invalid page number",
    )
    observed = [marker for marker in forbidden if marker.lower() in output.lower()]
    if observed:
        raise RuntimeError(
            f"SumatraPDF benchmark reports render failures: {observed}. Output: {output[-5000:]}"
        )


def _execute_sumatrapdf_probe(command: list[str]) -> None:
    executable = Path(command[0]).resolve()
    with tempfile.TemporaryDirectory(prefix="dokkomplekt-sumatra-probe-") as directory:
        sample_pdf = Path(directory) / "synthetic-one-page.pdf"
        _write_minimal_pdf(sample_pdf)
        completed = subprocess.run(
            [str(executable), "-log", "-bench", str(sample_pdf), "1"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=SUMATRAPDF_TIMEOUT_SECONDS,
            check=False,
            creationflags=int(getattr(subprocess, "CREATE_NO_WINDOW", 0)),
            env={**os.environ, "DOKKOMPLEKT_RUNTIME_PROBE": "1"},
        )
        output = completed.stdout or ""
        if completed.returncode not in (0, 1):
            raise RuntimeError(
                f"SumatraPDF benchmark returned unexpected exit {completed.returncode}. Output: {output[-5000:]}"
            )
        if not output:
            raise RuntimeError("SumatraPDF benchmark produced no captured detailed output")
        _validate_sumatrapdf_output(output, sample_pdf)
    print("RUNTIME PROBE OK: SumatraPDF (one-page benchmark load+render)")


def _execute_probe(title: str, command: list[str], accepted_codes: set[int]) -> None:
    completed = subprocess.run(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=TIMEOUT_SECONDS,
        check=False,
        env={**os.environ, "DOKKOMPLEKT_RUNTIME_PROBE": "1"},
    )
    if completed.returncode not in accepted_codes:
        tail = completed.stdout[-1200:].strip()
        raise RuntimeError(
            f"{title} failed to start (exit {completed.returncode}). "
            f"The portable runtime may miss DLLs or use the wrong architecture. Output: {tail}"
        )
    print(f"RUNTIME PROBE OK: {title}")


def run_probe(title: str, command: list[str], accepted_codes: set[int] | None = None) -> None:
    accepted = accepted_codes or {0}
    if title == "LibreOffice":
        # A version-only startup is not a useful release proof and can hang on
        # first-start initialization. Exercise the actual document-processing
        # path with a throw-away profile and deterministic synthetic DOCX.
        _execute_libreoffice_probe(command)
        return
    if title == "SumatraPDF":
        # SumatraPDF 3.6.1 can keep its GUI-oriented -help path alive and its
        # benchmark has a historical successful rc=1. Prove actual PDF page
        # load/render from its detailed output instead of trusting either.
        _execute_sumatrapdf_probe(command)
        return
    _execute_probe(title, command, accepted)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", default="windows-x86_64")
    parser.add_argument("--profile", choices=PROFILES)
    args = parser.parse_args()
    if os.name != "nt":
        raise RuntimeError("executable runtime probing must run on the target Windows machine")

    target_dir, status = load_status(args.target)
    if args.profile:
        profile = args.profile
    elif status.get("runtime_profile"):
        profile = normalize_profile(
            status.get("runtime_profile"),
            semantic_model_required=status.get("semantic_model_required"),
        )
    else:
        profile = FULL_PROFILE
    probes = runtime_probes(target_dir, status, profile)
    for title, command, accepted in probes:
        run_probe(title, command, accepted)
    print(f"OFFLINE RUNTIME EXECUTION PROBE PASSED: target={args.target}; profile={profile}; probes={len(probes)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
