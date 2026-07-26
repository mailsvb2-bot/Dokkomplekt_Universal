#!/usr/bin/env python3
"""Run a real image-only PDF/table smoke through Poppler and Tesseract.

The fixture contains no PDF text layer. On Windows release runners this script
resolves executables and tessdata from the exact staged offline runtime, proving
that the packaged sidecars can render and OCR a table before NSIS is built.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = ROOT / "tests" / "fixtures" / "ocr"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def resolve_executable(explicit: str | None, runtime_root: Path | None, names: tuple[str, ...]) -> Path:
    if explicit:
        path = Path(explicit).resolve()
        if not path.is_file():
            raise FileNotFoundError(f"tool is missing: {path}")
        return path
    if runtime_root:
        lowered = {name.lower() for name in names}
        candidates = sorted(
            path for path in runtime_root.rglob("*")
            if path.is_file() and path.name.lower() in lowered
        )
        if len(candidates) != 1:
            raise RuntimeError(
                f"expected exactly one of {sorted(lowered)} under {runtime_root}, found {len(candidates)}"
            )
        return candidates[0]
    for name in names:
        found = shutil.which(name)
        if found:
            return Path(found).resolve()
    raise FileNotFoundError(f"tool is unavailable: {', '.join(names)}")


def resolve_tessdata(explicit: str | None, runtime_root: Path | None) -> Path | None:
    if explicit:
        path = Path(explicit).resolve()
        if not path.is_dir():
            raise FileNotFoundError(f"tessdata directory is missing: {path}")
        return path
    if runtime_root:
        rus = sorted(runtime_root.rglob("rus.traineddata"))
        eng = sorted(runtime_root.rglob("eng.traineddata"))
        common = sorted({path.parent.resolve() for path in rus}.intersection(path.parent.resolve() for path in eng))
        if len(common) != 1:
            raise RuntimeError(f"expected one shared rus+eng tessdata directory, found {len(common)}")
        return common[0]
    return None


def parse_tsv(tsv: str) -> list[dict[str, Any]]:
    grouped: dict[tuple[int, int, int, int], list[dict[str, Any]]] = defaultdict(list)
    for index, raw in enumerate(tsv.splitlines()):
        if index == 0 and raw.lower().startswith("level\t"):
            continue
        columns = raw.split("\t", 11)
        if len(columns) < 12 or columns[0] != "5" or not columns[11].strip():
            continue
        confidence = float(columns[10])
        if confidence < 0:
            continue
        word = {
            "page": int(columns[1]),
            "block": int(columns[2]),
            "paragraph": int(columns[3]),
            "line": int(columns[4]),
            "left": int(columns[6]),
            "width": int(columns[8]),
            "confidence": max(0.0, min(1.0, confidence / 100.0)),
            "text": columns[11].strip(),
        }
        grouped[(word["page"], word["block"], word["paragraph"], word["line"])].append(word)

    rows: list[dict[str, Any]] = []
    for key, words in sorted(grouped.items()):
        words.sort(key=lambda item: item["left"])
        rendered = ""
        previous_right: int | None = None
        previous_char_width = 8.0
        for word in words:
            if previous_right is not None:
                gap = max(0, word["left"] - previous_right)
                table_gap = max(previous_char_width * 3.5, 24.0)
                rendered += "\t" if gap >= table_gap else " "
            rendered += word["text"]
            previous_right = word["left"] + word["width"]
            previous_char_width = word["width"] / max(len(word["text"]), 1)
        cells = [cell.strip() for cell in rendered.split("\t") if cell.strip()]
        rows.append({
            "key": list(key),
            "text": rendered.strip(),
            "cells": cells,
            "item_kind": "table_row" if len(cells) >= 2 else "text_line",
            "confidence": sum(word["confidence"] for word in words) / max(len(words), 1),
        })
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime-root", type=Path)
    parser.add_argument("--pdftoppm")
    parser.add_argument("--pdftotext")
    parser.add_argument("--tesseract")
    parser.add_argument("--tessdata-dir")
    parser.add_argument("--output-json", type=Path)
    args = parser.parse_args()

    runtime_root = args.runtime_root.resolve() if args.runtime_root else None
    if runtime_root and not runtime_root.is_dir():
        raise FileNotFoundError(f"runtime root is missing: {runtime_root}")
    pdftoppm = resolve_executable(args.pdftoppm, runtime_root, ("pdftoppm.exe", "pdftoppm"))
    pdftotext = resolve_executable(args.pdftotext, runtime_root, ("pdftotext.exe", "pdftotext"))
    tesseract = resolve_executable(args.tesseract, runtime_root, ("tesseract.exe", "tesseract"))
    tessdata = resolve_tessdata(args.tessdata_dir, runtime_root)

    expected = json.loads((FIXTURE_ROOT / "scanned_table.expected.json").read_text("utf-8"))
    pdf = FIXTURE_ROOT / expected["pdf"]
    if sha256_file(pdf) != expected["pdf_sha256"]:
        raise RuntimeError("scanned PDF fixture SHA-256 mismatch")

    extracted = subprocess.run(
        [str(pdftotext), str(pdf), "-"],
        check=True,
        capture_output=True,
    ).stdout
    if expected.get("image_only") is True and extracted.strip():
        raise RuntimeError("scanned PDF fixture unexpectedly contains a text layer")

    with tempfile.TemporaryDirectory(prefix="dokkomplekt-ocr-golden-") as temporary:
        prefix = Path(temporary) / "page"
        subprocess.run(
            [str(pdftoppm), "-png", "-r", "200", "-singlefile", str(pdf), str(prefix)],
            check=True,
            capture_output=True,
        )
        image = prefix.with_suffix(".png")
        command = [str(tesseract), str(image), "stdout", "-l", "rus+eng"]
        if tessdata:
            command.extend(["--tessdata-dir", str(tessdata)])
        command.extend(["--psm", "6", "tsv"])
        completed = subprocess.run(command, check=True, capture_output=True)
        tsv = completed.stdout.decode("utf-8", errors="replace")

    rows = parse_tsv(tsv)
    table_rows = [row for row in rows if row["item_kind"] == "table_row"]
    combined = "\n".join(row["text"] for row in rows)
    missing = [token for token in expected["required_tokens"] if token not in combined]
    if len(table_rows) < int(expected["minimum_table_rows"]):
        raise RuntimeError(
            f"OCR table structure is insufficient: {len(table_rows)} rows, expected at least {expected['minimum_table_rows']}"
        )
    if missing:
        raise RuntimeError(f"OCR fixture is missing required grounded tokens: {missing}")

    result = {
        "schema": "dokkomplekt.scanned-table-smoke.v1",
        "result": "passed",
        "fixture_sha256": expected["pdf_sha256"],
        "table_row_count": len(table_rows),
        "recognized_row_count": len(rows),
        "required_tokens": expected["required_tokens"],
        "tools": {
            "pdftoppm": str(pdftoppm),
            "pdftotext": str(pdftotext),
            "tesseract": str(tesseract),
            "tessdata": str(tessdata) if tessdata else None,
        },
    }
    if args.output_json:
        output = args.output_json if args.output_json.is_absolute() else ROOT / args.output_json
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", "utf-8")
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
