#!/usr/bin/env python3
"""Calibrate confidence buckets from specialist-final ground truth.

The script fails closed when the corpus is too small or no confidence threshold
can meet the requested error ceiling. It can optionally Ed25519-sign the exact
canonical payload; the private seed is never written to the output.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# Make sibling-tool imports deterministic when the script is launched from pytest,
# a packaged runner, or a working directory other than the repository root.
SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from measure_domain import SCHEMA, _domain_name, _entry, _field_rows, _load, _is_high_risk

OUTPUT_SCHEMA = "dokkomplekt.calibrated-thresholds.v1"


def canonical_bytes(value: dict[str, Any]) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def stable_holdout(entry_id: str, percent: int) -> bool:
    bucket = int.from_bytes(hashlib.sha256(entry_id.encode("utf-8")).digest()[:4], "big") % 100
    return bucket < percent


def choose_threshold(
    rows: list[tuple[str, float, bool]],
    *,
    max_error_rate: float,
    min_samples: int,
) -> tuple[float, int, int, float] | None:
    valid: list[tuple[float, int, int, float]] = []
    for threshold in sorted({confidence for _, confidence, _ in rows} | {1.0}):
        selected = [row for row in rows if row[1] >= threshold]
        if len(selected) < min_samples:
            continue
        errors = sum(1 for _, _, correct in selected if not correct)
        rate = errors / len(selected)
        if rate <= max_error_rate:
            valid.append((threshold, len(selected), errors, rate))
    return min(valid, key=lambda item: item[0]) if valid else None


def calibrate(
    data: dict[str, Any],
    *,
    domain: str,
    target_auto_error_rate: float,
    target_review_error_rate: float,
    min_auto_samples: int,
    min_review_samples: int,
    min_holdout_auto_samples: int,
    holdout_percent: int,
) -> dict[str, Any]:
    wanted = domain.lower()
    entries = [_entry(item) for item in data["entries"]]
    entries = [entry for entry in entries if _domain_name(entry.get("domain")) == wanted]
    training_entries = [entry for entry in entries if not stable_holdout(str(entry.get("entry_id", "")), holdout_percent)]
    holdout_entries = [entry for entry in entries if stable_holdout(str(entry.get("entry_id", "")), holdout_percent)]
    training_rows = [row for row in _field_rows(training_entries) if _is_high_risk(row[0])]
    holdout_rows = [row for row in _field_rows(holdout_entries) if _is_high_risk(row[0])]

    auto = choose_threshold(
        training_rows,
        max_error_rate=target_auto_error_rate,
        min_samples=min_auto_samples,
    )
    review = choose_threshold(
        training_rows,
        max_error_rate=target_review_error_rate,
        min_samples=min_review_samples,
    )
    if auto is None:
        raise ValueError("insufficient clean high-risk samples for an auto-print threshold")
    if review is None:
        raise ValueError("insufficient samples for a review threshold")
    auto_threshold, auto_n, auto_errors, auto_rate = auto
    review_threshold, review_n, review_errors, review_rate = review
    review_threshold = min(review_threshold, auto_threshold)

    holdout_selected = [row for row in holdout_rows if row[1] >= auto_threshold]
    holdout_errors = sum(1 for _, _, correct in holdout_selected if not correct)
    if len(holdout_selected) < min_holdout_auto_samples:
        raise ValueError(
            f"held-out auto bucket has {len(holdout_selected)} observations; "
            f"at least {min_holdout_auto_samples} are required"
        )
    holdout_rate = holdout_errors / len(holdout_selected)
    if holdout_rate > target_auto_error_rate:
        raise ValueError(
            f"held-out auto-bucket error rate {holdout_rate:.6f} exceeds target {target_auto_error_rate:.6f}"
        )

    corpus_digest = hashlib.sha256(canonical_bytes(data)).hexdigest()
    return {
        "schema": OUTPUT_SCHEMA,
        "domain": wanted,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "corpus_sha256": corpus_digest,
        "auto_min_confidence": auto_threshold,
        "review_min_confidence": review_threshold,
        "max_auto_error_rate": target_auto_error_rate,
        "training": {
            "entry_count": len(training_entries),
            "high_risk_observations": len(training_rows),
            "auto_bucket_observations": auto_n,
            "auto_bucket_errors": auto_errors,
            "auto_bucket_error_rate": auto_rate,
            "review_bucket_observations": review_n,
            "review_bucket_errors": review_errors,
            "review_bucket_error_rate": review_rate,
        },
        "holdout": {
            "entry_count": len(holdout_entries),
            "high_risk_observations": len(holdout_rows),
            "auto_bucket_observations": len(holdout_selected),
            "auto_bucket_errors": holdout_errors,
            "auto_bucket_error_rate": holdout_rate,
        },
        "policy": {
            "holdout_percent": holdout_percent,
            "min_auto_samples": min_auto_samples,
            "min_review_samples": min_review_samples,
            "min_holdout_auto_samples": min_holdout_auto_samples,
            "source_of_truth": "specialist_final_accepted",
        },
    }


def sign_payload(payload: dict[str, Any], seed_b64: str) -> dict[str, Any]:
    try:
        from ed25519_compat import SigningKey
    except ImportError as exc:
        raise ValueError("cryptography Ed25519 support is required for --sign") from exc
    try:
        seed = base64.b64decode(seed_b64, validate=True)
        key = SigningKey(seed)
    except Exception as exc:  # noqa: BLE001 - CLI validation boundary
        raise ValueError("signing key must be a base64 Ed25519 32-byte seed") from exc
    signature = key.sign(canonical_bytes(payload)).signature
    return {
        "payload": payload,
        "signature_alg": "ed25519",
        "signature_b64": base64.b64encode(signature).decode("ascii"),
        "public_key_b64": base64.b64encode(bytes(key.verify_key)).decode("ascii"),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("corpus", type=Path)
    parser.add_argument("--domain", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-auto-error-rate", type=float, default=0.005)
    parser.add_argument("--target-review-error-rate", type=float, default=0.05)
    parser.add_argument("--min-auto-samples", type=int, default=50)
    parser.add_argument("--min-review-samples", type=int, default=20)
    parser.add_argument("--min-holdout-auto-samples", type=int, default=10)
    parser.add_argument("--holdout-percent", type=int, default=20)
    parser.add_argument("--sign", action="store_true")
    parser.add_argument("--signing-key-env", default="DOKKOMPLEKT_THRESHOLD_PRIVATE_KEY_B64")
    args = parser.parse_args(argv)
    if not 0 <= args.target_auto_error_rate <= 1 or not 0 <= args.target_review_error_rate <= 1:
        print("ERROR: error-rate targets must be within [0,1]", file=sys.stderr)
        return 2
    if not 1 <= args.holdout_percent <= 50:
        print("ERROR: holdout percent must be within [1,50]", file=sys.stderr)
        return 2
    if args.min_auto_samples < 1 or args.min_review_samples < 1 or args.min_holdout_auto_samples < 1:
        print("ERROR: minimum sample counts must be positive", file=sys.stderr)
        return 2
    try:
        payload = calibrate(
            _load(args.corpus),
            domain=args.domain,
            target_auto_error_rate=args.target_auto_error_rate,
            target_review_error_rate=args.target_review_error_rate,
            min_auto_samples=args.min_auto_samples,
            min_review_samples=args.min_review_samples,
            min_holdout_auto_samples=args.min_holdout_auto_samples,
            holdout_percent=args.holdout_percent,
        )
        output: dict[str, Any] = payload
        if args.sign:
            seed = os.environ.get(args.signing_key_env, "").strip()
            if not seed:
                raise ValueError(f"{args.signing_key_env} is required for --sign")
            output = sign_payload(payload, seed)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix(args.output.suffix + ".tmp")
        temporary.write_text(json.dumps(output, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        temporary.replace(args.output)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"CALIBRATION FAILED CLOSED: {exc}", file=sys.stderr)
        return 1
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
