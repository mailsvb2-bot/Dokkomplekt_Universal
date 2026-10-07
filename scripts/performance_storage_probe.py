#!/usr/bin/env python3
"""Generate machine-observed storage-condition evidence for Canon E6.

The output intentionally contains no source path. For network_storage the
producer requires Windows, a UNC source path and GetDriveTypeW=DRIVE_REMOTE.
For slow_storage it performs the read itself under a configured throughput cap
and records the observed duration/throughput together with the source SHA.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import time
from typing import Any

SCHEMA = "dokkomplekt.performance-storage-probe.v1"
PRODUCER = "performance_storage_probe.py"
DRIVE_REMOTE = 4
DEFAULT_CHUNK_BYTES = 64 * 1024


def _sha256_and_count(path: Path, *, throttle_bytes_per_sec: int | None = None) -> tuple[str, int, float]:
    digest = hashlib.sha256()
    total = 0
    started = time.monotonic()
    with path.open("rb") as source:
        while True:
            chunk = source.read(DEFAULT_CHUNK_BYTES)
            if not chunk:
                break
            digest.update(chunk)
            total += len(chunk)
            if throttle_bytes_per_sec is not None:
                target_elapsed = total / throttle_bytes_per_sec
                delay = target_elapsed - (time.monotonic() - started)
                if delay > 0:
                    time.sleep(delay)
    elapsed = time.monotonic() - started
    return digest.hexdigest(), total, elapsed


def _network_probe(path: Path) -> dict[str, Any]:
    raw = str(path)
    if os.name != "nt":
        raise ValueError("network_storage probe requires Windows")
    if not raw.startswith("\\\\"):
        raise ValueError("network_storage probe requires a UNC source path")
    root = "\\\\" + raw.lstrip("\\").split("\\", 2)[0] + "\\" + raw.lstrip("\\").split("\\", 2)[1] + "\\"
    drive_type = ctypes.windll.kernel32.GetDriveTypeW(ctypes.c_wchar_p(root))
    if drive_type != DRIVE_REMOTE:
        raise ValueError("UNC source is not reported as DRIVE_REMOTE")
    source_sha256, byte_count, elapsed = _sha256_and_count(path)
    return {
        "schema": SCHEMA,
        "producer": PRODUCER,
        "storage_class": "network_storage",
        "source_sha256": source_sha256,
        "byte_count": byte_count,
        "duration_ms": elapsed * 1000.0,
        "observed_bytes_per_sec": (byte_count / elapsed) if elapsed > 0 else 0.0,
        "verification": {
            "path_kind": "unc",
            "windows_drive_type": "remote",
        },
    }


def _slow_probe(path: Path, throttle_bytes_per_sec: int) -> dict[str, Any]:
    if throttle_bytes_per_sec <= 0:
        raise ValueError("throttle_bytes_per_sec must be positive")
    source_sha256, byte_count, elapsed = _sha256_and_count(
        path, throttle_bytes_per_sec=throttle_bytes_per_sec
    )
    observed = (byte_count / elapsed) if elapsed > 0 else 0.0
    if byte_count > 0 and observed > throttle_bytes_per_sec * 1.10:
        raise ValueError("controlled throttle was not actually observed")
    return {
        "schema": SCHEMA,
        "producer": PRODUCER,
        "storage_class": "slow_storage",
        "source_sha256": source_sha256,
        "byte_count": byte_count,
        "duration_ms": elapsed * 1000.0,
        "observed_bytes_per_sec": observed,
        "verification": {
            "mode": "controlled_read_throttle",
            "configured_bytes_per_sec": throttle_bytes_per_sec,
        },
    }


def validate_probe(document: dict[str, Any]) -> None:
    expected = {
        "schema", "producer", "storage_class", "source_sha256", "byte_count",
        "duration_ms", "observed_bytes_per_sec", "verification",
    }
    if set(document) != expected:
        raise ValueError("storage probe keys must be closed")
    if document.get("schema") != SCHEMA or document.get("producer") != PRODUCER:
        raise ValueError("storage probe provenance is invalid")
    class_name = document.get("storage_class")
    if class_name not in {"slow_storage", "network_storage"}:
        raise ValueError("storage probe class is invalid")
    source_sha256 = document.get("source_sha256")
    if (
        not isinstance(source_sha256, str)
        or len(source_sha256) != 64
        or any(ch not in "0123456789abcdef" for ch in source_sha256)
    ):
        raise ValueError("storage probe source_sha256 is invalid")
    byte_count = document.get("byte_count")
    if not isinstance(byte_count, int) or isinstance(byte_count, bool) or byte_count < 0:
        raise ValueError("storage probe byte_count is invalid")
    for key in ("duration_ms", "observed_bytes_per_sec"):
        value = document.get(key)
        if (
            not isinstance(value, (int, float))
            or isinstance(value, bool)
            or not math.isfinite(float(value))
            or float(value) < 0
        ):
            raise ValueError(f"storage probe {key} is invalid")
    verification = document.get("verification")
    if not isinstance(verification, dict):
        raise ValueError("storage probe verification must be an object")
    if class_name == "network_storage":
        if verification != {"path_kind": "unc", "windows_drive_type": "remote"}:
            raise ValueError("network storage probe is not UNC/DRIVE_REMOTE verified")
    else:
        if set(verification) != {"mode", "configured_bytes_per_sec"}:
            raise ValueError("slow storage probe verification keys are invalid")
        if verification.get("mode") != "controlled_read_throttle":
            raise ValueError("slow storage probe did not use controlled throttle")
        cap = verification.get("configured_bytes_per_sec")
        if not isinstance(cap, int) or isinstance(cap, bool) or cap <= 0:
            raise ValueError("slow storage probe throttle cap is invalid")
        if byte_count > 0 and float(document["observed_bytes_per_sec"]) > cap * 1.10:
            raise ValueError("slow storage probe exceeds its declared throttle cap")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate verified E6 storage-condition probe evidence")
    parser.add_argument("--class", dest="class_name", choices=("slow_storage", "network_storage"), required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--throttle-bytes-per-sec", type=int)
    args = parser.parse_args(argv)
    try:
        if args.class_name == "network_storage":
            document = _network_probe(args.source)
        else:
            if args.throttle_bytes_per_sec is None:
                raise ValueError("slow_storage requires --throttle-bytes-per-sec")
            document = _slow_probe(args.source, args.throttle_bytes_per_sec)
        validate_probe(document)
    except (OSError, ValueError) as exc:
        print(f"STORAGE PROBE FAILED: {exc}")
        return 2
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"STORAGE PROBE OK: {document['storage_class']} sha256={document['source_sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
