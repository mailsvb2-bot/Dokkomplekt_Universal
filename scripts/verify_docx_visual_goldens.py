#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path, PurePosixPath
from urllib.parse import unquote

from lxml import etree
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "tests" / "fixtures" / "docx"
MANIFEST = CORPUS / "corpus-manifest.json"
BASELINE = CORPUS / "visual-golden.json"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dhash(path: Path, size: int = 16) -> str:
    image = Image.open(path).convert("L").resize((size + 1, size), Image.Resampling.LANCZOS)
    pixels = list(image.get_flattened_data())
    bits = []
    for row in range(size):
        offset = row * (size + 1)
        bits.extend(pixels[offset + col] > pixels[offset + col + 1] for col in range(size))
    value = 0
    for bit in bits:
        value = (value << 1) | int(bit)
    return f"{value:0{size*size//4}x}"


def hamming(left: str, right: str) -> int:
    return (int(left, 16) ^ int(right, 16)).bit_count()


def verify_package(path: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        bad = archive.testzip()
        if bad:
            raise SystemExit(f"CRC failure in {path.name}: {bad}")
        names = set(archive.namelist())
        required = {"[Content_Types].xml", "_rels/.rels", "word/document.xml"}
        missing = sorted(required - names)
        if missing:
            raise SystemExit(f"Missing OOXML parts in {path.name}: {missing}")
        for name in sorted(names):
            if name.endswith(".xml") or name.endswith(".rels"):
                etree.fromstring(archive.read(name))
        for rel_name in sorted(name for name in names if name.endswith(".rels")):
            rel_root = etree.fromstring(archive.read(rel_name))
            rel_dir = PurePosixPath(rel_name).parent
            source_dir = rel_dir.parent if rel_dir.name == "_rels" else rel_dir
            for rel in rel_root.findall(f"{{{REL_NS}}}Relationship"):
                if rel.get("TargetMode") == "External":
                    continue
                target = unquote(rel.get("Target", ""))
                if not target:
                    raise SystemExit(f"Empty relationship target in {path.name}:{rel_name}")
                normalized = PurePosixPath(source_dir, target)
                parts = []
                for part in normalized.parts:
                    if part in ("", "."):
                        continue
                    if part == "..":
                        if not parts:
                            raise SystemExit(f"Relationship escapes package in {path.name}:{rel_name}")
                        parts.pop()
                    else:
                        parts.append(part)
                resolved = "/".join(parts)
                if resolved not in names:
                    raise SystemExit(f"Broken relationship in {path.name}:{rel_name} -> {resolved}")


def render(path: Path, output: Path) -> list[Path]:
    soffice = shutil.which("soffice") or shutil.which("libreoffice")
    pdftoppm = shutil.which("pdftoppm")
    if not soffice or not pdftoppm:
        raise SystemExit("LibreOffice and pdftoppm are required for visual DOCX regression")
    output.mkdir(parents=True, exist_ok=True)
    profile = output / "lo-profile"
    profile.mkdir()
    env = os.environ.copy()
    env["HOME"] = str(output / "home")
    Path(env["HOME"]).mkdir()
    subprocess.run(
        [
            soffice,
            f"-env:UserInstallation=file://{profile}",
            "--headless",
            "--convert-to",
            "pdf",
            "--outdir",
            str(output),
            str(path),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=120,
        env=env,
    )
    pdf = output / f"{path.stem}.pdf"
    if not pdf.is_file() or pdf.stat().st_size == 0:
        raise SystemExit(f"LibreOffice did not create PDF for {path.name}")
    prefix = output / "page"
    subprocess.run(
        [pdftoppm, "-png", "-r", "120", str(pdf), str(prefix)],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
    )
    pages = sorted(output.glob("page-*.png"))
    if not pages:
        raise SystemExit(f"No rendered pages for {path.name}")
    return pages


def build_observation() -> dict:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    observed = {"schema": 1, "renderer": "libreoffice+pdftoppm-120dpi", "fixtures": {}}
    with tempfile.TemporaryDirectory(prefix="dokkomplekt-docx-visual-") as tmp:
        temp = Path(tmp)
        for name, expected in sorted(manifest["fixtures"].items()):
            path = CORPUS / name
            if sha256(path) != expected["sha256"]:
                raise SystemExit(f"Fixture hash drift: {name}")
            verify_package(path)
            pages = render(path, temp / path.stem)
            page_info = []
            for page in pages:
                with Image.open(page) as image:
                    bbox = image.convert("L").point(lambda x: 255 if x < 248 else 0).getbbox()
                    if bbox is None:
                        raise SystemExit(f"Blank visual page: {name}/{page.name}")
                    page_info.append({
                        "width": image.width,
                        "height": image.height,
                        "dhash16": dhash(page),
                    })
            observed["fixtures"][name] = {"pages": page_info}
    return observed


def verify(observed: dict, baseline: dict, tolerance: int) -> None:
    if set(observed["fixtures"]) != set(baseline["fixtures"]):
        raise SystemExit("Visual baseline fixture set differs from corpus")
    failures = []
    for name in sorted(observed["fixtures"]):
        actual_pages = observed["fixtures"][name]["pages"]
        expected_pages = baseline["fixtures"][name]["pages"]
        if len(actual_pages) != len(expected_pages):
            failures.append(f"{name}: page count {len(actual_pages)} != {len(expected_pages)}")
            continue
        for index, (actual, expected) in enumerate(zip(actual_pages, expected_pages), 1):
            if actual["width"] != expected["width"] or actual["height"] != expected["height"]:
                failures.append(f"{name} page {index}: dimensions changed")
            distance = hamming(actual["dhash16"], expected["dhash16"])
            if distance > tolerance:
                failures.append(f"{name} page {index}: visual hash distance {distance} > {tolerance}")
    if failures:
        raise SystemExit("DOCX visual golden regression:\n- " + "\n- ".join(failures))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--tolerance", type=int, default=32)
    args = parser.parse_args()
    observed = build_observation()
    if args.update_baseline:
        BASELINE.write_text(json.dumps(observed, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(f"UPDATED: {BASELINE}")
        return 0
    if not BASELINE.is_file():
        raise SystemExit("Visual baseline is missing; run with --update-baseline on the controlled QA renderer")
    baseline = json.loads(BASELINE.read_text(encoding="utf-8"))
    verify(observed, baseline, args.tolerance)
    print(f"DOCX VISUAL GOLDENS PASSED: fixtures={len(observed['fixtures'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
