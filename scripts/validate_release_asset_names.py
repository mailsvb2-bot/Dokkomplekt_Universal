#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

SCHEMA = "dokkomplekt.release-asset-name-validation.v1"


def validate(root: Path) -> dict[str, object]:
    files = sorted(path for path in root.rglob("*") if path.is_file())
    by_name: dict[str, list[str]] = defaultdict(list)
    for path in files:
        by_name[path.name.casefold()].append(path.relative_to(root).as_posix())
    duplicates = {
        paths[0].split("/")[-1]: paths
        for _key, paths in sorted(by_name.items())
        if len(paths) > 1
    }
    return {
        "schema": SCHEMA,
        "root": str(root),
        "asset_count": len(files),
        "valid": bool(files) and not duplicates,
        "duplicate_basenames": duplicates,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--json-report", type=Path)
    args = parser.parse_args()
    report = validate(args.root.resolve())
    rendered = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.json_report:
        args.json_report.parent.mkdir(parents=True, exist_ok=True)
        args.json_report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if report["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
