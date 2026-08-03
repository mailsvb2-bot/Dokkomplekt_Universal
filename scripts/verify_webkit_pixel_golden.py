#!/usr/bin/env python3
"""Compare a native WebKitGTK X11 capture with a committed visual golden profile."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def read_ppm(path: Path) -> tuple[int, int, list[tuple[int, int, int]]]:
    data = path.read_bytes()
    if not data.startswith(b"P6\n"):
        raise ValueError("only binary P6 PPM captures are supported")
    offset = 3
    tokens: list[bytes] = []
    while len(tokens) < 3:
        while offset < len(data) and chr(data[offset]).isspace():
            offset += 1
        if offset < len(data) and data[offset] == ord("#"):
            offset = data.index(b"\n", offset) + 1
            continue
        end = offset
        while end < len(data) and not chr(data[end]).isspace():
            end += 1
        tokens.append(data[offset:end])
        offset = end
    width, height, maximum = map(int, tokens)
    if maximum != 255:
        raise ValueError("PPM maximum channel value must be 255")
    if data[offset:offset + 2] == b"\r\n":
        offset += 2
    elif offset < len(data) and chr(data[offset]).isspace():
        offset += 1
    else:
        raise ValueError("PPM header is not terminated by whitespace")
    payload = data[offset:]
    expected = width * height * 3
    if len(payload) != expected:
        raise ValueError(f"PPM pixel payload has {len(payload)} bytes, expected {expected}")
    pixels = [tuple(payload[index:index + 3]) for index in range(0, expected, 3)]
    return width, height, pixels  # type: ignore[return-value]


def metrics(width: int, height: int, pixels: list[tuple[int, int, int]]) -> dict[str, float | int]:
    luminance = [(299 * red + 587 * green + 114 * blue) / 1000 for red, green, blue in pixels]
    mean = sum(luminance) / len(luminance)
    variance = sum((value - mean) ** 2 for value in luminance) / len(luminance)
    edge_hits = 0
    edge_total = 0
    threshold = 18
    for y in range(height):
        row = y * width
        for x in range(width):
            value = luminance[row + x]
            if x + 1 < width:
                edge_total += 1
                edge_hits += abs(value - luminance[row + x + 1]) >= threshold
            if y + 1 < height:
                edge_total += 1
                edge_hits += abs(value - luminance[row + width + x]) >= threshold
    grid_columns, grid_rows = 16, 10
    active_cells = 0
    for grid_y in range(grid_rows):
        top = grid_y * height // grid_rows
        bottom = max(top + 1, (grid_y + 1) * height // grid_rows)
        for grid_x in range(grid_columns):
            left = grid_x * width // grid_columns
            right = max(left + 1, (grid_x + 1) * width // grid_columns)
            values = [luminance[y * width + x] for y in range(top, bottom) for x in range(left, right)]
            cell_mean = sum(values) / len(values)
            cell_std = math.sqrt(sum((value - cell_mean) ** 2 for value in values) / len(values))
            active_cells += cell_std >= 4.0
    quantized = {(red // 8, green // 8, blue // 8) for red, green, blue in pixels}
    return {
        "width": width,
        "height": height,
        "quantized_colors": len(quantized),
        "luminance_mean": round(mean, 4),
        "luminance_std": round(math.sqrt(variance), 4),
        "dark_pixel_share": round(sum(value < 72 for value in luminance) / len(luminance), 6),
        "light_pixel_share": round(sum(value > 210 for value in luminance) / len(luminance), 6),
        "edge_density": round(edge_hits / max(1, edge_total), 6),
        "active_grid_cells": active_cells,
    }


def enforce(actual: dict[str, float | int], golden: dict[str, object]) -> list[str]:
    failures: list[str] = []
    for name, limits in golden["metrics"].items():  # type: ignore[index,union-attr]
        value = float(actual[name])
        minimum = limits.get("min")  # type: ignore[union-attr]
        maximum = limits.get("max")  # type: ignore[union-attr]
        if minimum is not None and value < float(minimum):
            failures.append(f"{name}={value} is below golden minimum {minimum}")
        if maximum is not None and value > float(maximum):
            failures.append(f"{name}={value} exceeds golden maximum {maximum}")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    width, height, pixels = read_ppm(args.image)
    actual = metrics(width, height, pixels)
    golden = json.loads(args.baseline.read_text(encoding="utf-8"))
    failures = enforce(actual, golden)
    report = {
        "schema": "dokkomplekt.webkit-pixel-golden.v1",
        "image": str(args.image),
        "baseline": str(args.baseline),
        "metrics": actual,
        "passed": not failures,
        "failures": failures,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    if failures:
        print("Native WebKitGTK pixel golden failed:")
        for failure in failures:
            print(f"- {failure}")
        return 1
    print(json.dumps(actual, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
