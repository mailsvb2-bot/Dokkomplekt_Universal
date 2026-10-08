#!/usr/bin/env python3
"""Generate a Canon runtime-layout proof through the installed Dokkomplekt app.

The probe deliberately uses the application's hardware-gated --e2e-export-pdf
command so DOCX->PDF conversion goes through the production
convert_office_document_to_pdf owner and its packaged soffice resolver.
pdftoppm is used only to rasterize the produced PDF for visual verification.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from typing import Any

from PIL import Image

PROOF_SCHEMA = "dokkomplekt.performance-layout-proof.v1"
PRODUCER = "performance_layout_probe.py"
CLAIM = "runtime_layout_verified_application_production_converter_path"
APP_EVIDENCE_SCHEMA = "dokkomplekt.fpr17-pdf-export-e2e.v1"
FONT_EXTENSIONS = {".ttf", ".ttc", ".otf", ".otc", ".woff", ".woff2"}


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path}: root must be an object")
    return value


def _is_sha256(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(ch in "0123456789abcdef" for ch in value)
    )


def _dhash(path: Path, size: int = 16) -> str:
    image = Image.open(path).convert("L").resize((size + 1, size), Image.Resampling.LANCZOS)
    pixels = list(image.get_flattened_data())
    value = 0
    for row in range(size):
        offset = row * (size + 1)
        for col in range(size):
            value = (value << 1) | int(
                pixels[offset + col] > pixels[offset + col + 1]
            )
    return f"{value:0{size * size // 4}x}"


def _hamming(left: str, right: str) -> int:
    return (int(left, 16) ^ int(right, 16)).bit_count()


def _font_roots() -> list[Path]:
    roots: list[Path] = []
    if os.name == "nt":
        windir = os.environ.get("WINDIR")
        if windir:
            roots.append(Path(windir) / "Fonts")
    elif platform.system() == "Darwin":
        roots.extend([Path("/System/Library/Fonts"), Path("/Library/Fonts")])
        roots.append(Path.home() / "Library" / "Fonts")
    else:
        roots.extend([Path("/usr/share/fonts"), Path("/usr/local/share/fonts")])
        roots.append(Path.home() / ".local" / "share" / "fonts")
    return roots


def font_set_fingerprint() -> tuple[str, int]:
    content_hashes: list[str] = []
    seen_real_files: set[Path] = set()
    for root in _font_roots():
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*")):
            if path.suffix.lower() not in FONT_EXTENSIONS or not path.is_file():
                continue
            try:
                real = path.resolve(strict=True)
            except OSError:
                continue
            if real in seen_real_files:
                continue
            seen_real_files.add(real)
            content_hashes.append(_sha256(real))
    if not content_hashes:
        raise ValueError("no font files found for layout proof")
    digest = hashlib.sha256()
    for value in sorted(content_hashes):
        digest.update(value.encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest(), len(content_hashes)


def _baseline_entry(
    baseline_path: Path,
    corpus_manifest_path: Path,
    fixture_name: str,
    source_sha256: str,
) -> tuple[list[dict[str, Any]], str]:
    baseline = _load(baseline_path)
    manifest = _load(corpus_manifest_path)
    fixtures = baseline.get("fixtures")
    if baseline.get("schema") != 1 or not isinstance(fixtures, dict):
        raise ValueError("visual baseline schema is invalid")
    entry = fixtures.get(fixture_name)
    if not isinstance(entry, dict) or not isinstance(entry.get("pages"), list):
        raise ValueError(f"fixture is absent from visual baseline: {fixture_name}")
    manifest_fixtures = manifest.get("fixtures")
    if not isinstance(manifest_fixtures, dict):
        raise ValueError("visual corpus manifest fixtures are invalid")
    expected = manifest_fixtures.get(fixture_name)
    if not isinstance(expected, dict) or expected.get("sha256") != source_sha256:
        raise ValueError("source SHA-256 does not match visual corpus manifest")
    pages = entry["pages"]
    if not pages:
        raise ValueError("visual baseline page list is empty")
    for index, page in enumerate(pages):
        if (
            not isinstance(page, dict)
            or set(page) != {"width", "height", "dhash16"}
            or not isinstance(page.get("width"), int)
            or not isinstance(page.get("height"), int)
            or not isinstance(page.get("dhash16"), str)
        ):
            raise ValueError(f"visual baseline page {index} is invalid")
    return pages, _sha256(baseline_path)


def _rasterize(pdf_path: Path, output: Path, dpi: int) -> list[Path]:
    pdftoppm = shutil.which("pdftoppm")
    if not pdftoppm:
        raise ValueError("pdftoppm is required for runtime layout proof")
    output.mkdir(parents=True, exist_ok=True)
    prefix = output / "page"
    completed = subprocess.run(
        [pdftoppm, "-png", "-r", str(dpi), str(pdf_path), str(prefix)],
        shell=False,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
    )
    if completed.returncode != 0:
        raise ValueError(
            f"pdftoppm failed with exit code {completed.returncode}"
        )
    pages = sorted(output.glob("page-*.png"))
    if not pages:
        raise ValueError("pdftoppm produced no layout pages")
    return pages


def _verify_pages(
    pages: list[Path],
    expected_pages: list[dict[str, Any]],
    tolerance: int,
) -> list[dict[str, Any]]:
    if len(pages) != len(expected_pages):
        raise ValueError(
            f"layout page count changed: {len(pages)} != {len(expected_pages)}"
        )
    observed: list[dict[str, Any]] = []
    for index, (path, expected) in enumerate(zip(pages, expected_pages), 1):
        with Image.open(path) as image:
            actual = {
                "width": image.width,
                "height": image.height,
                "dhash16": _dhash(path),
            }
        if actual["width"] != expected["width"] or actual["height"] != expected["height"]:
            raise ValueError(f"layout page {index} dimensions changed")
        distance = _hamming(actual["dhash16"], expected["dhash16"])
        if distance > tolerance:
            raise ValueError(
                f"layout page {index} visual hash distance {distance} > {tolerance}"
            )
        actual["distance"] = distance
        observed.append(actual)
    return observed


def validate_proof(proof: dict[str, Any]) -> None:
    expected = {
        "schema", "producer", "claim", "proof_id", "application_sha256",
        "app_version", "source_sha256", "pdf_sha256", "converter_sha256",
        "converter_version", "conversion_duration_ms", "application_elapsed_ms",
        "layout_check_duration_ms", "os", "font_set_sha256", "font_file_count",
        "visual_baseline_sha256",
        "settings", "pages", "verdict",
    }
    if set(proof) != expected:
        raise ValueError("layout proof keys must be closed")
    if (
        proof.get("schema") != PROOF_SCHEMA
        or proof.get("producer") != PRODUCER
        or proof.get("claim") != CLAIM
        or proof.get("verdict") != "pass"
    ):
        raise ValueError("layout proof provenance/verdict is invalid")
    for key in (
        "application_sha256", "source_sha256", "pdf_sha256",
        "converter_sha256", "font_set_sha256", "visual_baseline_sha256",
    ):
        if not _is_sha256(proof.get(key)):
            raise ValueError(f"layout proof {key} must be lowercase SHA-256")
    proof_id = proof.get("proof_id")
    if (
        not isinstance(proof_id, str)
        or not proof_id
        or len(proof_id) > 128
        or not all(ch.isascii() and (ch.isalnum() or ch in "-_.") for ch in proof_id)
    ):
        raise ValueError("layout proof proof_id must be an opaque ASCII identifier")
    for key in (
        "conversion_duration_ms",
        "application_elapsed_ms",
        "layout_check_duration_ms",
    ):
        value = proof.get(key)
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or float(value) < 0
        ):
            raise ValueError(f"layout proof {key} must be non-negative")
    for key in ("app_version", "converter_version", "os"):
        if not isinstance(proof.get(key), str) or not proof[key].strip():
            raise ValueError(f"layout proof {key} must be non-empty")
    if not isinstance(proof.get("font_file_count"), int) or proof["font_file_count"] <= 0:
        raise ValueError("layout proof font_file_count must be positive")
    pages = proof.get("pages")
    if not isinstance(pages, list) or not pages:
        raise ValueError("layout proof pages must be non-empty")
    for index, page in enumerate(pages):
        if not isinstance(page, dict) or set(page) != {
            "width", "height", "dhash16", "distance"
        }:
            raise ValueError(f"layout proof page {index} keys must be closed")
        if (
            not isinstance(page.get("width"), int)
            or isinstance(page.get("width"), bool)
            or page["width"] <= 0
            or not isinstance(page.get("height"), int)
            or isinstance(page.get("height"), bool)
            or page["height"] <= 0
        ):
            raise ValueError(f"layout proof page {index} dimensions are invalid")
        dhash16 = page.get("dhash16")
        if (
            not isinstance(dhash16, str)
            or len(dhash16) != 64
            or any(ch not in "0123456789abcdef" for ch in dhash16)
        ):
            raise ValueError(f"layout proof page {index} dhash16 is invalid")
        if (
            not isinstance(page.get("distance"), int)
            or isinstance(page.get("distance"), bool)
            or page["distance"] < 0
        ):
            raise ValueError(f"layout proof page {index} distance is invalid")
    settings = proof.get("settings")
    if not isinstance(settings, dict) or set(settings) != {"dpi", "dhash_size", "tolerance"}:
        raise ValueError("layout proof settings must be closed")
    if (
        not isinstance(settings["dpi"], int)
        or settings["dpi"] <= 0
        or settings["dhash_size"] != 16
        or not isinstance(settings["tolerance"], int)
        or settings["tolerance"] < 0
    ):
        raise ValueError("layout proof settings values are invalid")


def build_proof(
    *,
    app_path: Path,
    source_path: Path,
    baseline_path: Path,
    corpus_manifest_path: Path,
    fixture_name: str,
    tolerance: int,
    dpi: int,
) -> dict[str, Any]:
    if not app_path.is_absolute() or not app_path.is_file():
        raise ValueError("application path must be an absolute file")
    if not source_path.is_absolute() or not source_path.is_file():
        raise ValueError("layout source path must be an absolute file")
    if tolerance < 0 or dpi <= 0:
        raise ValueError("layout tolerance/dpi are invalid")
    source_sha256 = _sha256(source_path)
    expected_pages, baseline_sha256 = _baseline_entry(
        baseline_path, corpus_manifest_path, fixture_name, source_sha256
    )
    font_set_sha256, font_file_count = font_set_fingerprint()

    with tempfile.TemporaryDirectory(prefix="dokkomplekt-runtime-layout-") as temp_raw:
        temp = Path(temp_raw)
        app_evidence = temp / "app-evidence.json"
        pdf_path = app_evidence.with_suffix(".pdf")
        env = os.environ.copy()
        env["DOKKOMPLEKT_RUN_HARDWARE_E2E"] = "1"
        started = time.monotonic()
        completed = subprocess.run(
            [
                str(app_path),
                f"--e2e-export-pdf={source_path}",
                f"--e2e-evidence={app_evidence}",
            ],
            shell=False,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=180,
            env=env,
        )
        application_elapsed_ms = (time.monotonic() - started) * 1000.0
        if completed.returncode != 0:
            raise ValueError(
                f"application layout command failed with exit code {completed.returncode}"
            )
        if not app_evidence.is_file() or not pdf_path.is_file():
            raise ValueError("application did not produce layout evidence and PDF")
        evidence = _load(app_evidence)
        required = {
            "schema", "action", "app_version", "source_sha256", "pdf_sha256",
            "pdf_signature_valid", "converter", "converter_sha256",
            "converter_version", "conversion_duration_ms",
        }
        if not required.issubset(evidence):
            raise ValueError("application PDF evidence lacks layout identity fields")
        if (
            evidence.get("schema") != APP_EVIDENCE_SCHEMA
            or evidence.get("action") != "export_pdf"
            or evidence.get("pdf_signature_valid") is not True
            or evidence.get("converter") != "production convert_office_document_to_pdf"
        ):
            raise ValueError("application PDF evidence provenance is invalid")
        if evidence.get("source_sha256") != source_sha256:
            raise ValueError("application evidence source SHA differs")
        if evidence.get("pdf_sha256") != _sha256(pdf_path):
            raise ValueError("application evidence PDF SHA differs")
        if not _is_sha256(evidence.get("converter_sha256")):
            raise ValueError("installed application converter SHA is invalid")
        if not isinstance(evidence.get("converter_version"), str) or not evidence["converter_version"].strip():
            raise ValueError("installed application converter version is missing")
        conversion_ms = evidence.get("conversion_duration_ms")
        if (
            not isinstance(conversion_ms, (int, float))
            or isinstance(conversion_ms, bool)
            or float(conversion_ms) < 0
        ):
            raise ValueError("installed application conversion duration is invalid")

        pages = _rasterize(pdf_path, temp / "pages", dpi)
        observed_pages = _verify_pages(pages, expected_pages, tolerance)

        layout_check_duration_ms = (time.monotonic() - started) * 1000.0
        proof = {
            "schema": PROOF_SCHEMA,
            "producer": PRODUCER,
            "claim": CLAIM,
            "proof_id": hashlib.sha256(
                (
                    _sha256(app_path)
                    + source_sha256
                    + evidence["pdf_sha256"]
                    + baseline_sha256
                ).encode("ascii")
            ).hexdigest()[:32],
            "application_sha256": _sha256(app_path),
            "app_version": evidence.get("app_version"),
            "source_sha256": source_sha256,
            "pdf_sha256": evidence["pdf_sha256"],
            "converter_sha256": evidence["converter_sha256"],
            "converter_version": evidence["converter_version"].strip(),
            "conversion_duration_ms": float(conversion_ms),
            "application_elapsed_ms": application_elapsed_ms,
            "layout_check_duration_ms": layout_check_duration_ms,
            "os": platform.platform(),
            "font_set_sha256": font_set_sha256,
            "font_file_count": font_file_count,
            "visual_baseline_sha256": baseline_sha256,
            "settings": {"dpi": dpi, "dhash_size": 16, "tolerance": tolerance},
            "pages": observed_pages,
            "verdict": "pass",
        }
        validate_proof(proof)
        return proof


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate installed-runtime layout proof")
    parser.add_argument("--app", required=True, type=Path)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument(
        "--baseline",
        default="tests/fixtures/docx/visual-golden.json",
        type=Path,
    )
    parser.add_argument(
        "--corpus-manifest",
        default="tests/fixtures/docx/corpus-manifest.json",
        type=Path,
    )
    parser.add_argument("--fixture-name", required=True)
    parser.add_argument("--tolerance", type=int, default=32)
    parser.add_argument("--dpi", type=int, default=120)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        proof = build_proof(
            app_path=args.app,
            source_path=args.source,
            baseline_path=args.baseline,
            corpus_manifest_path=args.corpus_manifest,
            fixture_name=args.fixture_name,
            tolerance=args.tolerance,
            dpi=args.dpi,
        )
    except (OSError, ValueError, json.JSONDecodeError, subprocess.SubprocessError) as exc:
        print(f"RUNTIME LAYOUT PROOF FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(proof, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        "RUNTIME LAYOUT PROOF PASS: "
        f"source={proof['source_sha256']} pages={len(proof['pages'])} "
        f"converter={proof['converter_version']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
